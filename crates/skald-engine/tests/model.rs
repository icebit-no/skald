//! Tests with the real models and clips. They are ignored by default, since CI has neither; run them with
//!
//! ```sh
//! SKALD_TEST_WHISPER=~/models/nb-whisper-large/ggml-model.bin \
//! SKALD_TEST_VAD=~/models/silero-vad-v6.2-F32.gguf \
//! SKALD_TEST_CLIPS=~/skald-clips SKALD_TEST_TAKES=~/skald-takes \
//!     cargo test -p skald-engine -- --ignored
//! ```
//!
//! The model files are those of docs/research/engine-check.md («What was run»). The clips come from
//! `research/clips/fetch.py --out $SKALD_TEST_CLIPS`, the takes from
//! `research/engine-check/make_clips.py vocab --out $SKALD_TEST_TAKES` and
//! `make_clips.py silence --out $SKALD_TEST_TAKES --fleurs $SKALD_TEST_CLIPS`.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use skald_core::config::Language;
use skald_core::silence::Silence;
use skald_core::silence_gate::{Decision, Reason};
use skald_core::vocabulary::{CountTokens, Prompt, Vocabulary};
use skald_engine::{Decoding, Engine, PROMPT_TOKENS, TakeOptions};

fn env_path(name: &str) -> PathBuf {
    let value = std::env::var_os(name)
        .unwrap_or_else(|| panic!("{name} is not set; see the top of tests/model.rs"));
    PathBuf::from(value)
}

/// The engine for one test, loaded fresh and dropped at its end: ggml's Metal backend aborts at exit while
/// a model is still loaded. The guard keeps one test on the GPU at a time.
fn engine() -> (MutexGuard<'static, ()>, Engine) {
    static GPU: Mutex<()> = Mutex::new(());
    let guard = GPU.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let engine = Engine::load(&env_path("SKALD_TEST_WHISPER"), &env_path("SKALD_TEST_VAD"))
        .expect("load the models");
    (guard, engine)
}

fn read_wav(path: &Path) -> Vec<f32> {
    let mut reader = hound::WavReader::open(path).unwrap_or_else(|e| panic!("{path:?}: {e}"));
    let spec = reader.spec();
    assert_eq!((spec.channels, spec.sample_rate), (1, 16_000), "{path:?}");
    let scale = (1i64 << (spec.bits_per_sample - 1)) as f32;
    reader
        .samples::<i32>()
        .map(|sample| sample.unwrap() as f32 / scale)
        .collect()
}

fn reference(wav: &Path) -> String {
    std::fs::read_to_string(wav.with_extension("txt"))
        .unwrap()
        .trim()
        .to_owned()
}

/// The loudest level reading as skald-audio's meter takes it: the RMS of each 60 ms, in dBFS.
fn loudest_dbfs(samples: &[f32]) -> f32 {
    samples
        .chunks(960)
        .map(|chunk| {
            let mean =
                chunk.iter().map(|&s| f64::from(s).powi(2)).sum::<f64>() / chunk.len() as f64;
            ((10.0 * mean.log10()) as f32).max(-100.0)
        })
        .fold(-100.0, f32::max)
}

fn wavs(dir: &Path, prefix: &str) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{dir:?}: {e}"))
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.extension().is_some_and(|ext| ext == "wav")
                && path
                    .file_name()
                    .is_some_and(|name| name.to_string_lossy().starts_with(prefix))
        })
        .collect();
    found.sort();
    assert!(!found.is_empty(), "no {prefix}*.wav in {dir:?}");
    found
}

fn research_prompt(name: &str) -> Prompt {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../research/engine-check/prompts")
        .join(name);
    let sentence = std::fs::read_to_string(path).unwrap().trim().to_owned();
    let mut vocabulary = Vocabulary::default();
    vocabulary.sentence = sentence;
    vocabulary.prompt()
}

fn options(prompt: &Prompt) -> TakeOptions<'_> {
    TakeOptions {
        language: Language::Bokmal,
        prompt,
        decoding: Decoding::Greedy,
    }
}

/// Lower-case words with the punctuation around them left out, as docs/research/engine-check.md scores
/// them: hyphens and decimal points stay.
fn words(text: &str) -> Vec<String> {
    text.split_whitespace()
        .map(|word| {
            word.trim_matches(|c: char| ".,!?;:«»\"“”()[]".contains(c))
                .to_lowercase()
        })
        .filter(|word| !word.is_empty())
        .collect()
}

fn word_error_rate(reference: &str, text: &str) -> f64 {
    let (reference, text) = (words(reference), words(text));
    let mut row: Vec<usize> = (0..=text.len()).collect();
    for (i, r) in reference.iter().enumerate() {
        let mut diagonal = row[0];
        row[0] = i + 1;
        for (j, t) in text.iter().enumerate() {
            let substitution = diagonal + usize::from(r != t);
            diagonal = row[j + 1];
            row[j + 1] = substitution.min(row[j] + 1).min(row[j + 1] + 1);
        }
    }
    row[text.len()] as f64 / reference.len() as f64
}

#[test]
#[ignore = "needs the models; see the top of the file"]
fn runs_on_metal_and_writes_the_engine_check_text() {
    let (_gpu, mut engine) = engine();
    if cfg!(target_os = "macos") {
        assert!(engine.transcriber().device().starts_with("MTL"));
    }
    // The whole clip, as docs/research/engine-check.md transcribed it (section 5).
    let samples = read_wav(&env_path("SKALD_TEST_CLIPS").join("fleurs-nb-1994.wav"));
    let empty = Vocabulary::default().prompt();
    let transcript = engine
        .transcriber_mut()
        .transcribe(&samples, &options(&empty))
        .unwrap();
    assert_eq!(
        transcript.text,
        "Hvis du allerede kan et romansk språk, vil det være lettere for deg å lære portugisisk."
    );
    assert!(transcript.complete);
    assert_eq!(transcript.prompt.tokens, 0);
}

#[test]
#[ignore = "needs the models; see the top of the file"]
fn counts_prompt_tokens_as_the_engine_check() {
    let (_gpu, engine) = engine();
    // docs/research/engine-check.md, section 2.
    assert_eq!(research_prompt("ordskatt.txt").tokens(&engine), 67);
    assert_eq!(research_prompt("ordskatt-long.txt").tokens(&engine), 213);
    assert_eq!(engine.count_tokens(""), 0);
    assert_eq!(
        engine.count_tokens("  Skald "),
        engine.count_tokens("Skald")
    );
}

#[test]
#[ignore = "needs the models; see the top of the file"]
fn a_prompt_over_the_budget_loses_its_front() {
    let (_gpu, mut engine) = engine();
    let mut vocabulary = Vocabulary::default();
    vocabulary.sentence = "Jeg har merget pull-requesten og deployet releasen.".into();
    for n in 0..120 {
        vocabulary.add_word(&format!("Verktøy{n}"));
    }
    vocabulary.add_word("PostgreSQL");
    let prompt = vocabulary.prompt();
    assert!(prompt.tokens(&engine) > PROMPT_TOKENS);

    let samples = read_wav(&env_path("SKALD_TEST_TAKES").join("vocab-01.wav"));
    let transcript = engine
        .transcriber_mut()
        .transcribe(&samples, &options(&prompt))
        .unwrap();
    assert!(transcript.prompt.tokens <= PROMPT_TOKENS);
    assert!(
        transcript.prompt.dropped > 1,
        "the sentence and the first words go"
    );
    assert!(transcript.prompt.text.ends_with("PostgreSQL."));
}

#[test]
#[ignore = "needs the models; see the top of the file"]
fn takes_without_speech_heard_nothing_and_skip_the_model() {
    let (_gpu, mut engine) = engine();
    let empty = Vocabulary::default().prompt();
    for wav in wavs(&env_path("SKALD_TEST_TAKES"), "silent-") {
        let samples = read_wav(&wav);
        let take = engine
            .take(
                &samples,
                loudest_dbfs(&samples),
                &options(&empty),
                &Silence::default(),
            )
            .unwrap();
        assert!(
            matches!(
                take.decision,
                Decision::HeardNothing(Reason::BelowSpeechLevel | Reason::TooLittleSpeech)
            ),
            "{wav:?}: {take:?}"
        );
        assert_eq!(take.transcript, None, "{wav:?}");
    }
}

#[test]
#[ignore = "needs the models; see the top of the file"]
fn takes_with_speech_are_inserted() {
    let (_gpu, mut engine) = engine();
    let empty = Vocabulary::default().prompt();
    let mut takes = wavs(&env_path("SKALD_TEST_CLIPS"), "fleurs-nb-");
    takes.extend(wavs(&env_path("SKALD_TEST_TAKES"), "word-"));
    for wav in takes {
        let samples = read_wav(&wav);
        let take = engine
            .take(
                &samples,
                loudest_dbfs(&samples),
                &options(&empty),
                &Silence::default(),
            )
            .unwrap();
        // A lone «Ja.» too: its 444 ms of speech is more than a known phrase may hold.
        assert_eq!(take.decision, Decision::Insert, "{wav:?}: {take:?}");
        assert!(take.transcript.is_some(), "{wav:?}");
        assert!(take.speech.duration().as_millis() >= 380, "{wav:?}");
    }
}

/// Cutting the silence around the speech costs no more than a word or two over the 20 clips
/// (`TRIM_MARGIN` has the survey).
#[test]
#[ignore = "needs the models; see the top of the file"]
fn cutting_the_silence_around_the_speech_keeps_the_words() {
    let (_gpu, mut engine) = engine();
    let empty = Vocabulary::default().prompt();
    let (mut whole_errors, mut trimmed_errors, mut reference_words) = (0.0, 0.0, 0);
    for wav in wavs(&env_path("SKALD_TEST_CLIPS"), "fleurs-nb-") {
        let samples = read_wav(&wav);
        let reference = reference(&wav);
        let n = words(&reference).len();
        let whole = engine
            .transcriber_mut()
            .transcribe(&samples, &options(&empty))
            .unwrap();
        let take = engine
            .take(
                &samples,
                loudest_dbfs(&samples),
                &options(&empty),
                &Silence::default(),
            )
            .unwrap();
        let trimmed = take.transcript.unwrap();
        whole_errors += word_error_rate(&reference, &whole.text) * n as f64;
        trimmed_errors += word_error_rate(&reference, &trimmed.text) * n as f64;
        reference_words += n;
    }
    eprintln!(
        "word errors over {reference_words} words: whole {whole_errors}, trimmed {trimmed_errors}"
    );
    assert!(trimmed_errors <= whole_errors + 2.0);
}

/// The take that broke with a full ordskatt carried into every window: 61 % word errors in stock
/// transcribe.cpp 0.5.1, 10.5 % in whisper.cpp (docs/research/engine-check.md, section 2).
#[test]
#[ignore = "needs the models; see the top of the file"]
fn a_long_take_keeps_its_text_with_a_full_ordskatt() {
    let (_gpu, mut engine) = engine();
    let wav = env_path("SKALD_TEST_TAKES").join("vocab-long.wav");
    let samples = read_wav(&wav);
    for name in ["ordskatt.txt", "ordskatt-long.txt"] {
        let prompt = research_prompt(name);
        let transcript = engine
            .transcriber_mut()
            .transcribe(&samples, &options(&prompt))
            .unwrap();
        let rate = word_error_rate(&reference(&wav), &transcript.text);
        eprintln!(
            "{name}: {:.1} % word errors\n{}",
            rate * 100.0,
            transcript.text
        );
        assert!(rate < 0.2, "{name}: {:.1} % word errors", rate * 100.0);
    }
}

#[test]
fn word_error_rate_counts_edits_over_reference_words() {
    assert_eq!(word_error_rate("Vi vet ikke.", "vi vet ikke"), 0.0);
    assert_eq!(
        word_error_rate("Vi vet ikke helt", "Du vet det ikke helt"),
        0.5
    );
    assert_eq!(
        words("«Hei», sa han: 2,4 GHz."),
        ["hei", "sa", "han", "2,4", "ghz"]
    );
}
