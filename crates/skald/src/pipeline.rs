//! What happens to a recording once the take has ended: the silence gate and the model, the corrections,
//! the insertion and the history. It runs on a thread of its own, which holds the model for as long as Skald
//! runs, so the keys stay live while the model works and takes are inserted in the order they were made.
//! The command line's questions that need the model, the ordskatt meter and a file to transcribe, are
//! answered on the same thread, between takes.

use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{self, Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use skald_audio::Recording;
use skald_core::Config;
use skald_core::history::Entry;
use skald_core::silence::Silence;
use skald_core::silence_gate::{Decision, Reason};
use skald_core::state::State;
use skald_core::vocabulary::{CountTokens, Prompt};
use skald_engine::{Decoding, Engine, Take, TakeOptions};
use skald_input::{InsertError, InsertOptions, Inserted};

use crate::files;
use crate::sounds::{Chime, Sound};
use crate::wav;

/// The speech model and the silence gate around it.
pub(crate) trait Transcribe {
    /// Where the model computes, such as «MTL0».
    fn device(&self) -> String;
    /// The tokens a text takes for the model, as the ordskatt meter counts them.
    fn count_tokens(&self, text: &str) -> usize;
    fn take(
        &mut self,
        samples: &[f32],
        loudest_dbfs: f32,
        options: &TakeOptions,
        silence: &Silence,
    ) -> Result<Take, skald_engine::Error>;
}

impl Transcribe for Engine {
    fn device(&self) -> String {
        self.transcriber().device()
    }

    fn count_tokens(&self, text: &str) -> usize {
        CountTokens::count_tokens(self, text)
    }

    fn take(
        &mut self,
        samples: &[f32],
        loudest_dbfs: f32,
        options: &TakeOptions,
        silence: &Silence,
    ) -> Result<Take, skald_engine::Error> {
        Engine::take(self, samples, loudest_dbfs, options, silence)
    }
}

pub(crate) trait Insert {
    fn insert(&mut self, text: &str, options: &InsertOptions) -> Result<Inserted, InsertError>;
}

#[cfg(target_os = "macos")]
impl Insert for skald_input::Inserter {
    fn insert(&mut self, text: &str, options: &InsertOptions) -> Result<Inserted, InsertError> {
        skald_input::Inserter::insert(self, text, options)
    }
}

/// A finished take, handed over to be transcribed.
pub(crate) struct Job {
    pub(crate) recording: Recording,
    /// The settings as they stood when the take ended.
    pub(crate) config: Arc<Config>,
    pub(crate) ended_at: Instant,
    pub(crate) unix_ms: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Outcome {
    Inserted {
        text: String,
        inserted: Inserted,
        /// False when the model stopped early, so the text may lack its end.
        complete: bool,
    },
    NotInserted {
        text: String,
        error: InsertError,
        complete: bool,
    },
    HeardNothing(Reason),
    Failed(String),
}

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Timings {
    pub(crate) audio: Duration,
    pub(crate) speech: Duration,
    pub(crate) vad: Duration,
    pub(crate) model: Option<skald_engine::Timings>,
    pub(crate) prompt_tokens: Option<usize>,
    /// Whole parts of the ordskatt dropped from its front to fit.
    pub(crate) prompt_dropped: usize,
    /// From the end of the take until the text was ready to insert.
    pub(crate) ready: Option<Duration>,
}

/// How one take went.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Report {
    pub(crate) microphone: String,
    pub(crate) outcome: Outcome,
    pub(crate) timings: Timings,
    /// Why the history could not be saved, naming the file.
    pub(crate) state_error: Option<String>,
}

pub(crate) struct Pipeline<T, I> {
    transcriber: T,
    inserter: I,
    chime: Arc<dyn Chime>,
    state: State,
    state_file: PathBuf,
}

impl<T: Transcribe, I: Insert> Pipeline<T, I> {
    pub(crate) fn new(
        transcriber: T,
        inserter: I,
        chime: Arc<dyn Chime>,
        state: State,
        state_file: PathBuf,
    ) -> Self {
        Self {
            transcriber,
            inserter,
            chime,
            state,
            state_file,
        }
    }

    /// Transcribes and inserts one take. A take with text updates the history and the correction counts
    /// and writes them to the state file, even when the insertion failed, so the text is not lost.
    pub(crate) fn process(&mut self, job: Job) -> Report {
        let Job {
            recording,
            config,
            ended_at,
            unix_ms,
        } = job;
        let mut report = Report {
            microphone: recording.microphone.name.clone(),
            outcome: Outcome::HeardNothing(Reason::NoWords),
            timings: Timings {
                audio: recording.duration(),
                ..Timings::default()
            },
            state_error: None,
        };

        let prompt = config.vocabulary.prompt();
        let options = TakeOptions {
            language: config.language,
            prompt: &prompt,
            decoding: Decoding::Greedy,
        };
        let take = match self.transcriber.take(
            &recording.samples,
            recording.loudest_dbfs(),
            &options,
            &config.silence,
        ) {
            Ok(take) => take,
            Err(error) => {
                report.outcome = Outcome::Failed(error.to_string());
                return report;
            }
        };
        report.timings.speech = take.speech.duration();
        report.timings.vad = take.vad_time;
        if let Some(transcript) = &take.transcript {
            report.timings.model = Some(transcript.timings);
            report.timings.prompt_tokens = Some(transcript.prompt.tokens);
            report.timings.prompt_dropped = transcript.prompt.dropped;
        }
        let (text, complete) = match (take.decision, &take.transcript) {
            (Decision::Insert, Some(transcript)) => (&transcript.text, transcript.complete),
            (Decision::HeardNothing(reason), _) => {
                report.outcome = Outcome::HeardNothing(reason);
                return report;
            }
            (Decision::Insert, None) => return report,
        };

        let text = config
            .corrections
            .apply_and_count(text, &mut self.state.counts);
        // The silence gate's rule for a text without a word, applied again after the corrections.
        if text.chars().any(char::is_alphanumeric) {
            report.timings.ready = Some(ended_at.elapsed());
            if config.sounds {
                self.chime.play(Sound::Inserted);
            }
            let options = InsertOptions {
                keep_text_on_clipboard: config.copy_to_clipboard,
                ..InsertOptions::default()
            };
            report.outcome = match self.inserter.insert(&text, &options) {
                Ok(inserted) => Outcome::Inserted {
                    text: text.clone(),
                    inserted,
                    complete,
                },
                Err(error) => Outcome::NotInserted {
                    text: text.clone(),
                    error,
                    complete,
                },
            };
            self.state.history.push(Entry {
                text,
                unix_ms,
                microphone: report.microphone.clone(),
            });
        }
        self.state.counts.retain(&config.corrections);
        report.state_error = files::write_state(&self.state_file, &self.state).err();
        report
    }
}

impl<T: Transcribe, I> Pipeline<T, I> {
    pub(crate) fn count_tokens(&self, prompt: &Prompt) -> usize {
        prompt.tokens(&|text: &str| self.transcriber.count_tokens(text))
    }

    /// Transcribes a file's samples as a take, through the silence gate, with the configuration's language,
    /// ordskatt and known phrases; nothing is corrected, inserted or kept.
    pub(crate) fn transcribe(&mut self, samples: &[f32], config: &Config) -> Result<Take, String> {
        let prompt = config.vocabulary.prompt();
        let options = TakeOptions {
            language: config.language,
            prompt: &prompt,
            decoding: Decoding::Greedy,
        };
        self.transcriber
            .take(
                samples,
                wav::loudest_dbfs(samples),
                &options,
                &config.silence,
            )
            .map_err(|error| error.to_string())
    }
}

#[derive(Debug)]
pub(crate) enum WorkerEvent {
    Loaded { device: String, took: Duration },
    LoadFailed(String),
    Done(Report),
}

/// What the worker is asked to do, in the order it was asked.
enum Work {
    Take(Job),
    CountTokens {
        prompt: Prompt,
        answer: Sender<usize>,
    },
    Transcribe {
        samples: Vec<f32>,
        config: Arc<Config>,
        answer: Sender<Result<Take, String>>,
    },
    Stop,
}

/// The thread that loads the model, then does the work handed to it one piece at a time.
pub(crate) struct Worker {
    work: Sender<Work>,
    thread: Option<JoinHandle<()>>,
}

impl Worker {
    /// Starts the thread, which reports on `events` when the model has loaded or could not, then once per
    /// take.
    pub(crate) fn spawn<T, I, L>(
        load: L,
        inserter: I,
        chime: Arc<dyn Chime>,
        state: State,
        state_file: PathBuf,
        events: Sender<WorkerEvent>,
    ) -> io::Result<Self>
    where
        T: Transcribe,
        I: Insert + Send + 'static,
        L: FnOnce() -> Result<T, String> + Send + 'static,
    {
        let (work, inbox) = mpsc::channel::<Work>();
        let thread = thread::Builder::new()
            .name("skald-pipeline".into())
            .spawn(move || {
                // A send fails only once the one asking has stopped listening, and then no one needs to
                // know.
                let started = Instant::now();
                let transcriber = match load() {
                    Ok(transcriber) => transcriber,
                    Err(reason) => {
                        let _ = events.send(WorkerEvent::LoadFailed(reason));
                        return;
                    }
                };
                let _ = events.send(WorkerEvent::Loaded {
                    device: transcriber.device(),
                    took: started.elapsed(),
                });
                let mut pipeline = Pipeline::new(transcriber, inserter, chime, state, state_file);
                for work in inbox {
                    match work {
                        Work::Take(job) => {
                            let _ = events.send(WorkerEvent::Done(pipeline.process(job)));
                        }
                        Work::CountTokens { prompt, answer } => {
                            let _ = answer.send(pipeline.count_tokens(&prompt));
                        }
                        Work::Transcribe {
                            samples,
                            config,
                            answer,
                        } => {
                            let _ = answer.send(pipeline.transcribe(&samples, &config));
                        }
                        Work::Stop => break,
                    }
                }
            })?;
        Ok(Self {
            work,
            thread: Some(thread),
        })
    }

    pub(crate) fn submit(&self, job: Job) {
        // Fails only when the model could not load, which ends Skald.
        let _ = self.work.send(Work::Take(job));
    }

    /// For the command line's questions, from any thread.
    pub(crate) fn handle(&self) -> WorkerHandle {
        WorkerHandle {
            work: self.work.clone(),
        }
    }

    /// Finishes the takes already handed over, then waits until the thread has dropped the model, which
    /// must happen before the process exits. A question asked after this gets no answer.
    pub(crate) fn finish(mut self) {
        self.join();
    }

    fn join(&mut self) {
        let _ = self.work.send(Work::Stop);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.join();
    }
}

/// Asks the worker what needs the model. Each question waits behind the work handed over before it, a
/// take being transcribed included.
#[derive(Clone)]
pub(crate) struct WorkerHandle {
    work: Sender<Work>,
}

impl WorkerHandle {
    /// `None` once the worker has stopped.
    pub(crate) fn count_tokens(&self, prompt: Prompt) -> Option<usize> {
        let (answer, answered) = mpsc::channel();
        self.work.send(Work::CountTokens { prompt, answer }).ok()?;
        answered.recv().ok()
    }

    pub(crate) fn transcribe(
        &self,
        samples: Vec<f32>,
        config: Arc<Config>,
    ) -> Result<Take, String> {
        let stopped = || "Talemodellen er ikke lastet.".to_owned();
        let (answer, answered) = mpsc::channel();
        self.work
            .send(Work::Transcribe {
                samples,
                config,
                answer,
            })
            .map_err(|_| stopped())?;
        answered.recv().map_err(|_| stopped())?
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::collections::VecDeque;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, Ordering};

    use skald_audio::{Ending, MicId, Microphone};
    use skald_core::config::Language;
    use skald_core::corrections::Correction;
    use skald_core::vocabulary::Fitted;
    use skald_engine::{Speech, Transcript};

    use super::*;
    use crate::files::tests::Scratch;
    use crate::sounds::tests::Played;

    pub(crate) fn quiet() -> Arc<dyn Chime> {
        Arc::new(Played::default())
    }

    pub(crate) fn heard(text: &str) -> Take {
        Take {
            decision: Decision::Insert,
            speech: Speech::new(std::iter::once(0..16_000).collect()),
            vad_time: Duration::from_millis(9),
            transcript: Some(Transcript {
                text: text.to_owned(),
                complete: true,
                prompt: Fitted {
                    text: String::new(),
                    tokens: 12,
                    dropped: 0,
                },
                timings: skald_engine::Timings::default(),
            }),
        }
    }

    pub(crate) fn silent(reason: Reason) -> Take {
        Take {
            decision: Decision::HeardNothing(reason),
            speech: Speech::default(),
            vad_time: Duration::ZERO,
            transcript: None,
        }
    }

    #[derive(Default)]
    struct Seen {
        language: Option<Language>,
        prompt: Option<String>,
        loudest: Option<f32>,
    }

    pub(crate) struct FakeModel {
        takes: VecDeque<Result<Take, skald_engine::Error>>,
        seen: Arc<Mutex<Seen>>,
        dropped: Arc<AtomicBool>,
    }

    impl FakeModel {
        pub(crate) fn new(
            takes: impl IntoIterator<Item = Result<Take, skald_engine::Error>>,
        ) -> Self {
            Self {
                takes: takes.into_iter().collect(),
                seen: Arc::default(),
                dropped: Arc::default(),
            }
        }
    }

    impl Transcribe for FakeModel {
        fn device(&self) -> String {
            "FAKE0".into()
        }

        fn count_tokens(&self, text: &str) -> usize {
            text.split_whitespace().count()
        }

        fn take(
            &mut self,
            _samples: &[f32],
            loudest_dbfs: f32,
            options: &TakeOptions,
            _silence: &Silence,
        ) -> Result<Take, skald_engine::Error> {
            let mut seen = self.seen.lock().unwrap();
            seen.language = Some(options.language);
            seen.prompt = Some(options.prompt.text());
            seen.loudest = Some(loudest_dbfs);
            self.takes.pop_front().expect("a take for each job")
        }
    }

    impl Drop for FakeModel {
        fn drop(&mut self) {
            self.dropped.store(true, Ordering::SeqCst);
        }
    }

    #[derive(Clone)]
    pub(crate) struct FakeInserter {
        result: Result<Inserted, InsertError>,
        calls: Arc<Mutex<Vec<(String, bool)>>>,
    }

    impl FakeInserter {
        pub(crate) fn answering(result: Result<Inserted, InsertError>) -> Self {
            Self {
                result,
                calls: Arc::default(),
            }
        }
    }

    impl Insert for FakeInserter {
        fn insert(&mut self, text: &str, options: &InsertOptions) -> Result<Inserted, InsertError> {
            self.calls
                .lock()
                .unwrap()
                .push((text.to_owned(), options.keep_text_on_clipboard));
            self.result.clone()
        }
    }

    fn config() -> Config {
        let mut config = Config::default();
        config.vocabulary.sentence = "Vi merget pull-requesten.".into();
        config.vocabulary.add_word("PostgreSQL");
        config.corrections.add("mercha", "merget").unwrap();
        config
            .corrections
            .add("Post gres SQL", "PostgreSQL")
            .unwrap();
        config
    }

    fn job(config: Config) -> Job {
        Job {
            recording: Recording {
                samples: vec![0.0; 32_000],
                levels: vec![-60.0, -18.5, -40.0],
                ending: Ending::Stopped,
                microphone: Microphone {
                    id: MicId::new("usb"),
                    name: "Shure MV7".into(),
                },
            },
            config: Arc::new(config),
            ended_at: Instant::now(),
            unix_ms: 1_795_800_000_000,
        }
    }

    #[test]
    fn speech_is_corrected_inserted_counted_and_kept_in_the_history() {
        let scratch = Scratch::new("pipeline-insert");
        let state_file = scratch.0.join("state.toml");
        let model = FakeModel::new([Ok(heard("Vi mercha Post gres SQL i går."))]);
        let seen = Arc::clone(&model.seen);
        let inserter = FakeInserter::answering(Ok(Inserted::PreviousRestored));
        let calls = Arc::clone(&inserter.calls);
        let mut pipeline = Pipeline::new(
            model,
            inserter,
            quiet(),
            State::default(),
            state_file.clone(),
        );

        let report = pipeline.process(job(config()));

        assert_eq!(
            report.outcome,
            Outcome::Inserted {
                text: "Vi merget PostgreSQL i går.".into(),
                inserted: Inserted::PreviousRestored,
                complete: true,
            }
        );
        assert_eq!(report.microphone, "Shure MV7");
        assert_eq!(report.state_error, None);
        assert_eq!(report.timings.audio, Duration::from_secs(2));
        assert_eq!(report.timings.speech, Duration::from_secs(1));
        assert_eq!(report.timings.prompt_tokens, Some(12));
        assert!(report.timings.ready.is_some());
        assert_eq!(
            *calls.lock().unwrap(),
            [("Vi merget PostgreSQL i går.".to_owned(), false)]
        );
        let seen = seen.lock().unwrap();
        assert_eq!(seen.language, Some(Language::Bokmal));
        assert_eq!(
            seen.prompt.as_deref(),
            Some("Vi merget pull-requesten. PostgreSQL.")
        );
        assert_eq!(seen.loudest, Some(-18.5));

        let state = files::read_state(&state_file).unwrap();
        let latest = state.history.latest().unwrap();
        assert_eq!(latest.text, "Vi merget PostgreSQL i går.");
        assert_eq!(latest.unix_ms, 1_795_800_000_000);
        assert_eq!(latest.microphone, "Shure MV7");
        assert_eq!(state.counts.get("mercha"), 1);
        assert_eq!(state.counts.get("post gres sql"), 1);
    }

    #[test]
    fn pop_plays_as_text_is_inserted_and_only_with_sounds_on() {
        let scratch = Scratch::new("pipeline-pop");
        let state_file = scratch.0.join("state.toml");
        let played = Played::default();
        let model = FakeModel::new([
            Ok(heard("Hei.")),
            Ok(silent(Reason::TooLittleSpeech)),
            Ok(heard("Hei igjen.")),
        ]);
        let inserter = FakeInserter::answering(Ok(Inserted::PreviousRestored));
        let mut pipeline = Pipeline::new(
            model,
            inserter,
            Arc::new(played.clone()),
            State::default(),
            state_file,
        );
        pipeline.process(job(config()));
        pipeline.process(job(config()));
        assert_eq!(played.sounds(), [Sound::Inserted], "not for silence");

        let mut quiet = config();
        quiet.sounds = false;
        pipeline.process(job(quiet));
        assert_eq!(played.sounds(), [Sound::Inserted]);
    }

    #[test]
    fn silence_inserts_nothing_and_leaves_the_state_alone() {
        let scratch = Scratch::new("pipeline-silence");
        let state_file = scratch.0.join("state.toml");
        for reason in [
            Reason::BelowSpeechLevel,
            Reason::TooLittleSpeech,
            Reason::NoWords,
            Reason::KnownPhrase,
        ] {
            let inserter = FakeInserter::answering(Ok(Inserted::PreviousRestored));
            let calls = Arc::clone(&inserter.calls);
            let model = FakeModel::new([Ok(silent(reason))]);
            let mut pipeline = Pipeline::new(
                model,
                inserter,
                quiet(),
                State::default(),
                state_file.clone(),
            );
            let report = pipeline.process(job(config()));
            assert_eq!(report.outcome, Outcome::HeardNothing(reason));
            assert_eq!(report.microphone, "Shure MV7");
            assert!(calls.lock().unwrap().is_empty());
            assert!(!state_file.exists());
        }
    }

    #[test]
    fn copying_to_the_clipboard_follows_the_setting() {
        let scratch = Scratch::new("pipeline-copy");
        let inserter = FakeInserter::answering(Ok(Inserted::TextKept));
        let calls = Arc::clone(&inserter.calls);
        let model = FakeModel::new([Ok(heard("Hei."))]);
        let mut pipeline = Pipeline::new(
            model,
            inserter,
            quiet(),
            State::default(),
            scratch.0.join("state.toml"),
        );
        let mut config = config();
        config.copy_to_clipboard = true;
        pipeline.process(job(config));
        assert_eq!(*calls.lock().unwrap(), [("Hei.".to_owned(), true)]);
    }

    #[test]
    fn a_failed_insertion_still_keeps_the_text_in_the_history() {
        let scratch = Scratch::new("pipeline-not-inserted");
        let state_file = scratch.0.join("state.toml");
        let model = FakeModel::new([Ok(heard("Vi mercha den."))]);
        let inserter = FakeInserter::answering(Err(InsertError::KeysHeld));
        let mut pipeline = Pipeline::new(
            model,
            inserter,
            quiet(),
            State::default(),
            state_file.clone(),
        );
        let report = pipeline.process(job(config()));
        assert_eq!(
            report.outcome,
            Outcome::NotInserted {
                text: "Vi merget den.".into(),
                error: InsertError::KeysHeld,
                complete: true,
            }
        );
        let state = files::read_state(&state_file).unwrap();
        assert_eq!(state.history.latest().unwrap().text, "Vi merget den.");
    }

    #[test]
    fn corrections_that_leave_nothing_heard_nothing() {
        let scratch = Scratch::new("pipeline-emptied");
        let state_file = scratch.0.join("state.toml");
        let mut config = config();
        config.corrections.rules.push(Correction::new("øh", ""));
        let model = FakeModel::new([Ok(heard("Øh."))]);
        let inserter = FakeInserter::answering(Ok(Inserted::PreviousRestored));
        let calls = Arc::clone(&inserter.calls);
        let mut pipeline = Pipeline::new(
            model,
            inserter,
            quiet(),
            State::default(),
            state_file.clone(),
        );
        let report = pipeline.process(job(config));
        assert_eq!(report.outcome, Outcome::HeardNothing(Reason::NoWords));
        assert!(calls.lock().unwrap().is_empty(), "«.» is not inserted");
        let state = files::read_state(&state_file).unwrap();
        assert!(state.history.latest().is_none());
        assert_eq!(state.counts.get("øh"), 1);
    }

    #[test]
    fn a_text_cut_short_by_the_model_says_so() {
        let scratch = Scratch::new("pipeline-incomplete");
        let mut take = heard("Hei, dette er");
        take.transcript.as_mut().unwrap().complete = false;
        let model = FakeModel::new([Ok(take)]);
        let inserter = FakeInserter::answering(Ok(Inserted::PreviousRestored));
        let mut pipeline = Pipeline::new(
            model,
            inserter,
            quiet(),
            State::default(),
            scratch.0.join("state.toml"),
        );
        let report = pipeline.process(job(config()));
        assert!(matches!(
            report.outcome,
            Outcome::Inserted {
                complete: false,
                ..
            }
        ));
    }

    #[test]
    fn a_model_error_is_reported_and_the_next_take_goes_on() {
        let scratch = Scratch::new("pipeline-error");
        let model = FakeModel::new([
            Err(skald_engine::Error::Transcribe {
                reason: "out of memory".into(),
            }),
            Ok(heard("Hei.")),
        ]);
        let inserter = FakeInserter::answering(Ok(Inserted::PreviousRestored));
        let mut pipeline = Pipeline::new(
            model,
            inserter,
            quiet(),
            State::default(),
            scratch.0.join("state.toml"),
        );
        assert_eq!(
            pipeline.process(job(config())).outcome,
            Outcome::Failed("transcription failed: out of memory".into())
        );
        assert!(matches!(
            pipeline.process(job(config())).outcome,
            Outcome::Inserted { .. }
        ));
    }

    #[test]
    fn counts_of_rules_no_longer_in_the_configuration_are_dropped() {
        let scratch = Scratch::new("pipeline-retain");
        let state_file = scratch.0.join("state.toml");
        let model = FakeModel::new([Ok(heard("Vi mercha den.")), Ok(heard("Hei."))]);
        let inserter = FakeInserter::answering(Ok(Inserted::PreviousRestored));
        let mut pipeline = Pipeline::new(
            model,
            inserter,
            quiet(),
            State::default(),
            state_file.clone(),
        );
        pipeline.process(job(config()));
        assert_eq!(
            files::read_state(&state_file).unwrap().counts.get("mercha"),
            1
        );

        let mut without = config();
        without.corrections.remove("mercha");
        pipeline.process(job(without));
        let state = files::read_state(&state_file).unwrap();
        assert_eq!(state.counts.get("mercha"), 0);
        assert_eq!(state.history.entries().len(), 2);
    }

    #[test]
    fn a_state_file_that_cannot_be_written_is_reported() {
        let scratch = Scratch::new("pipeline-unwritable");
        let blocker = scratch.0.join("file");
        std::fs::write(&blocker, b"").unwrap();
        let model = FakeModel::new([Ok(heard("Hei."))]);
        let inserter = FakeInserter::answering(Ok(Inserted::PreviousRestored));
        let state_file = blocker.join("state.toml");
        let mut pipeline = Pipeline::new(
            model,
            inserter,
            quiet(),
            State::default(),
            state_file.clone(),
        );
        let report = pipeline.process(job(config()));
        assert!(matches!(report.outcome, Outcome::Inserted { .. }));
        let error = report.state_error.unwrap();
        assert!(
            error.starts_with(&state_file.display().to_string()),
            "{error}"
        );
    }

    #[test]
    fn the_worker_finishes_its_jobs_and_drops_the_model_before_it_returns() {
        let scratch = Scratch::new("worker");
        let model = FakeModel::new([Ok(heard("Hei.")), Ok(silent(Reason::TooLittleSpeech))]);
        let dropped = Arc::clone(&model.dropped);
        let (events, received) = mpsc::channel();
        let worker = Worker::spawn(
            move || Ok(model),
            FakeInserter::answering(Ok(Inserted::PreviousRestored)),
            quiet(),
            State::default(),
            scratch.0.join("state.toml"),
            events,
        )
        .unwrap();
        worker.submit(job(config()));
        worker.submit(job(config()));
        worker.finish();
        assert!(dropped.load(Ordering::SeqCst));

        let events: Vec<WorkerEvent> = received.try_iter().collect();
        assert!(matches!(&events[0], WorkerEvent::Loaded { device, .. } if device == "FAKE0"));
        assert!(matches!(
            &events[1],
            WorkerEvent::Done(Report {
                outcome: Outcome::Inserted { .. },
                ..
            })
        ));
        assert!(matches!(
            &events[2],
            WorkerEvent::Done(Report {
                outcome: Outcome::HeardNothing(Reason::TooLittleSpeech),
                ..
            })
        ));
        assert_eq!(events.len(), 3);
    }

    #[test]
    fn the_worker_answers_the_command_line_between_takes() {
        let scratch = Scratch::new("worker-questions");
        let model = FakeModel::new([Ok(heard("Hei.")), Ok(silent(Reason::TooLittleSpeech))]);
        let seen = Arc::clone(&model.seen);
        let (events, _received) = mpsc::channel();
        let worker = Worker::spawn(
            move || Ok(model),
            FakeInserter::answering(Ok(Inserted::PreviousRestored)),
            quiet(),
            State::default(),
            scratch.0.join("state.toml"),
            events,
        )
        .unwrap();
        let handle = worker.handle();
        let config = config();
        assert_eq!(handle.count_tokens(config.vocabulary.prompt()), Some(4));

        let mut nynorsk = config.clone();
        nynorsk.language = Language::Nynorsk;
        let take = handle
            .transcribe(vec![0.5; 1600], Arc::new(nynorsk))
            .unwrap();
        assert_eq!(take.text(), Some("Hei."));
        assert_eq!(seen.lock().unwrap().language, Some(Language::Nynorsk));
        assert!(!scratch.0.join("state.toml").exists(), "nothing is kept");

        worker.submit(job(config.clone()));
        worker.finish();
        assert_eq!(handle.count_tokens(config.vocabulary.prompt()), None);
        assert!(handle.transcribe(Vec::new(), Arc::new(config)).is_err());
    }

    #[test]
    fn a_model_that_will_not_load_is_reported() {
        let (events, received) = mpsc::channel();
        let worker = Worker::spawn(
            || Err::<FakeModel, _>("could not load /m.bin: file not found".to_owned()),
            FakeInserter::answering(Ok(Inserted::PreviousRestored)),
            quiet(),
            State::default(),
            PathBuf::from("/nonexistent/state.toml"),
            events,
        )
        .unwrap();
        worker.submit(job(config()));
        worker.finish();
        let events: Vec<WorkerEvent> = received.try_iter().collect();
        assert!(matches!(
            &events[..],
            [WorkerEvent::LoadFailed(reason)] if reason.contains("file not found")
        ));
    }
}
