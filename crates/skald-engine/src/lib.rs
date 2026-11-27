//! Transcription through transcribe.cpp: NB-Whisper loaded once and kept while the app runs, Silero VAD
//! to find the speech in a take, the silence gate between them, and the model's tokenizer for the
//! ordskatt meter.
//!
//! A take is 16 kHz mono samples, full scale ±1.0. [`Engine::take`] runs it through the silence gate in
//! the order docs/design.md, Silence, gives, and transcribes it only when there is speech to transcribe.

use std::ffi::CString;
use std::fmt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use skald_core::config::Language;
use skald_core::silence::Silence;
use skald_core::silence_gate::{self, Decision};
use skald_core::vocabulary::{CountTokens, Fitted, Prompt};

mod vad;
mod whisper;

pub use vad::{Speech, TRIM_MARGIN, Vad};

/// The rate of the samples every take is given in.
pub const SAMPLE_RATE: u32 = 16_000;

/// The tokens the prompt may take (docs/design.md, Vocabulary and corrections): half of Whisper's
/// 448-token decoder window, less one, comes before each 30-second window's own text. Skald's build of
/// transcribe.cpp lets the prompt and the text before the window share that room, the prompt first
/// (D-23), so the whole of it is the prompt's.
pub const PROMPT_TOKENS: usize = 223;

/// How the model picks each next token.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Decoding {
    /// The likeliest token at temperature 0, falling back to higher temperatures when a window's text
    /// repeats itself or the model doubts it, as Whisper does. transcribe.cpp decodes Whisper no other
    /// way.
    #[default]
    Greedy,
}

/// What a take is transcribed with.
#[derive(Debug, Clone, Copy)]
pub struct TakeOptions<'a> {
    pub language: Language,
    /// The ordskatt, fitted to [`PROMPT_TOKENS`] before it is sent.
    pub prompt: &'a Prompt,
    pub decoding: Decoding,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Timings {
    pub mel: Duration,
    pub encode: Duration,
    pub decode: Duration,
    /// The whole call, which also covers preparing the prompt and copying the text.
    pub total: Duration,
}

/// What the model wrote for a take.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transcript {
    /// As the model wrote it, before corrections.
    pub text: String,
    /// False when transcription stopped early, at the output limit or because the text began to
    /// repeat; the text is what came before.
    pub complete: bool,
    /// The prompt as it was sent.
    pub prompt: Fitted,
    pub timings: Timings,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// A model file that could not be loaded: missing, unreadable, or not a model this engine runs.
    Load {
        path: PathBuf,
        reason: String,
    },
    Vad {
        reason: String,
    },
    Transcribe {
        reason: String,
    },
    /// A text for the model with a NUL character in it, which the model cannot be given.
    Nul,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Load { path, reason } => write!(f, "could not load {}: {reason}", path.display()),
            Self::Vad { reason } => write!(f, "speech detection failed: {reason}"),
            Self::Transcribe { reason } => write!(f, "transcription failed: {reason}"),
            Self::Nul => f.write_str("the text holds a NUL character"),
        }
    }
}

impl std::error::Error for Error {}

/// Sends the engine's own log lines to the `log` crate, where the app's logger shows or drops them;
/// without it they go to standard error. Call it once, before loading a model.
pub fn route_logs_to_log_crate() {
    transcribe_cpp::init_logging();
}

/// A Whisper model, loaded once and kept for every take.
///
/// Drop it before the process exits: the Metal backend aborts the process at exit when a model is still
/// loaded.
pub struct Transcriber {
    // Declared before the model, so it is dropped first.
    session: whisper::Session,
    model: whisper::Model,
}

impl fmt::Debug for Transcriber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Transcriber")
            .field("device", &self.device())
            .finish_non_exhaustive()
    }
}

impl Transcriber {
    /// Loads a Whisper model: NbAiLab's `ggml-model.bin` for NB-Whisper, or a Whisper GGUF.
    pub fn load(path: &Path) -> Result<Self, Error> {
        let model = whisper::Model::load(path)?;
        let arch = model.arch();
        if arch != "whisper" {
            return Err(Error::Load {
                path: path.to_owned(),
                reason: format!("a {arch} model, not Whisper"),
            });
        }
        let session = whisper::Session::new(&model)?;
        Ok(Self { session, model })
    }

    /// The device the model computes on: «MTL0», the GPU through Metal, on a Mac.
    pub fn device(&self) -> String {
        self.model.device()
    }

    /// Transcribes 16 kHz mono samples. The prompt is fitted to [`PROMPT_TOKENS`] by dropping whole parts
    /// from its front, and heads every 30-second window of a long take.
    pub fn transcribe(
        &mut self,
        samples: &[f32],
        options: &TakeOptions,
    ) -> Result<Transcript, Error> {
        let started = Instant::now();
        let Decoding::Greedy = options.decoding;
        let prompt = options.prompt.fit(PROMPT_TOKENS, self);
        let c_prompt = CString::new(prompt.text.as_str()).map_err(|_| Error::Nul)?;
        let language = CString::new(whisper_language(options.language)).map_err(|_| Error::Nul)?;
        let run = self.session.run(
            samples,
            &language,
            (!prompt.text.is_empty()).then_some(c_prompt.as_c_str()),
        )?;
        Ok(Transcript {
            text: run.text,
            complete: run.complete,
            prompt,
            timings: Timings {
                mel: run.mel,
                encode: run.encode,
                decode: run.decode,
                total: started.elapsed(),
            },
        })
    }
}

/// Counts as the model reads a Whisper prompt: the text trimmed, after one space. A text the tokenizer
/// cannot take counts as more than any budget, so it is never sent.
impl CountTokens for Transcriber {
    fn count_tokens(&self, text: &str) -> usize {
        let text = text.trim();
        if text.is_empty() {
            return 0;
        }
        self.model
            .tokenize(&format!(" {text}"))
            .map_or(usize::MAX, |tokens| tokens.len())
    }
}

/// NB-Whisper's language tokens: `no` writes bokmål, `nn` nynorsk.
fn whisper_language(language: Language) -> &'static str {
    match language {
        Language::Bokmal => "no",
        Language::Nynorsk => "nn",
    }
}

/// The speech model and VAD together, which take a recording to text or to «Hørte ingenting». Like the
/// [`Transcriber`] in it, it must be dropped before the process exits.
#[derive(Debug)]
pub struct Engine {
    transcriber: Transcriber,
    vad: Vad,
}

/// How a take ended.
#[derive(Debug, Clone, PartialEq)]
pub struct Take {
    pub decision: Decision,
    /// The speech VAD found, none when the take stayed below speech level and VAD did not run.
    pub speech: Speech,
    pub vad_time: Duration,
    /// None when the gate ended the take before the model ran.
    pub transcript: Option<Transcript>,
}

impl Take {
    /// The text to insert, before corrections; none when the take heard nothing.
    pub fn text(&self) -> Option<&str> {
        match (self.decision, &self.transcript) {
            (Decision::Insert, Some(transcript)) => Some(&transcript.text),
            _ => None,
        }
    }
}

impl Engine {
    pub fn load(model: &Path, vad_model: &Path) -> Result<Self, Error> {
        Ok(Self {
            transcriber: Transcriber::load(model)?,
            vad: Vad::load(vad_model)?,
        })
    }

    pub fn transcriber(&self) -> &Transcriber {
        &self.transcriber
    }

    /// The model alone, for transcribing without the silence gate, as a measurement does.
    pub fn transcriber_mut(&mut self) -> &mut Transcriber {
        &mut self.transcriber
    }

    /// Runs a take through the silence gate and, when it has speech, the model: the level first, then
    /// VAD, then the model on the take with the silence before and after the speech cut away, then the
    /// text. `loudest_dbfs` is the take's loudest level reading.
    pub fn take(
        &mut self,
        samples: &[f32],
        loudest_dbfs: f32,
        options: &TakeOptions,
        silence: &Silence,
    ) -> Result<Take, Error> {
        let mut take = Take {
            decision: Decision::Insert,
            speech: Speech::default(),
            vad_time: Duration::ZERO,
            transcript: None,
        };
        if let Err(reason) = silence_gate::check_level(loudest_dbfs) {
            take.decision = Decision::HeardNothing(reason);
            return Ok(take);
        }

        let started = Instant::now();
        take.speech = self.vad.detect(samples)?;
        take.vad_time = started.elapsed();
        let speech = take.speech.duration();
        if let Err(reason) = silence_gate::check_speech(speech) {
            take.decision = Decision::HeardNothing(reason);
            return Ok(take);
        }

        let transcript = self
            .transcriber
            .transcribe(take.speech.trim(samples), options)?;
        take.decision = silence_gate::decide(loudest_dbfs, speech, &transcript.text, silence);
        take.transcript = Some(transcript);
        Ok(take)
    }
}

impl CountTokens for Engine {
    fn count_tokens(&self, text: &str) -> usize {
        self.transcriber.count_tokens(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bokmal_and_nynorsk_have_their_own_language_codes() {
        assert_eq!(whisper_language(Language::Bokmal), "no");
        assert_eq!(whisper_language(Language::Nynorsk), "nn");
    }

    #[test]
    fn a_missing_model_file_is_a_load_error() {
        let path = Path::new("/nonexistent/skald/ggml-model.bin");
        match Transcriber::load(path) {
            Err(Error::Load { path: failed, .. }) => assert_eq!(failed, path),
            other => panic!("{other:?}"),
        }
        assert!(matches!(Vad::load(path), Err(Error::Load { .. })));
    }

    #[test]
    fn errors_say_what_failed() {
        let error = Error::Load {
            path: PathBuf::from("/models/x.bin"),
            reason: "file not found".into(),
        };
        assert_eq!(
            error.to_string(),
            "could not load /models/x.bin: file not found"
        );
    }
}
