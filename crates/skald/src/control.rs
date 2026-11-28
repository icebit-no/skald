//! What the command line asks of Skald (skald-control, D-28): [`LocalMachine`], the machine itself while
//! Skald is not running, and [`Control`], the running Skald's answers on its socket. Both give the same
//! microphones and models, and transcribe a file the same way, so a command answers alike either way.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use skald_audio::Connected;
use skald_control::protocol::Tokens;
use skald_control::{Error, ErrorCode, Machine, Microphones, Model, Skald, Transcribed};
use skald_core::Config;
use skald_core::corrections::Counts;
use skald_core::history::History;
use skald_core::paths::Paths;
use skald_core::silence_gate::Decision;
use skald_core::state::State;
use skald_core::vocabulary::Prompt;
use skald_engine::{Decoding, Engine, PROMPT_TOKENS, Take, TakeOptions};
use skald_input::PressRefused;
use skald_models::{MODELS, Status};

use crate::dictation::microphone_order;
use crate::files;
use crate::models::ModelFiles;
use crate::pipeline::{WorkerHandle, route_engine_logs};
use crate::say::{Exit, Line, capture_reason};
use crate::wav;

/// This machine, for the command line while Skald is not running. It writes nothing to standard output,
/// which carries the command's answer, and for `skald mcp` its protocol; what it has to say goes to
/// standard error.
pub(crate) struct LocalMachine {
    paths: Paths,
}

impl LocalMachine {
    pub(crate) fn new(paths: Paths) -> Self {
        Self { paths }
    }
}

impl Machine for LocalMachine {
    fn microphones(&self, config: &Config) -> Result<Microphones, Error> {
        microphones(config)
    }

    fn models(&self) -> Result<Vec<Model>, Error> {
        Ok(models(&self.paths))
    }

    /// Loads the model in this process for the one file, and drops it before returning, as the engine
    /// requires before the process exits.
    fn transcribe(&self, config: &Config, wav: &Path) -> Result<Transcribed, Error> {
        let files = ModelFiles::locate(&self.paths, &config.model, None, None).map_err(stopped)?;
        files
            .check(|| eprintln!("{}", Line::CheckingModel))
            .map_err(stopped)?;
        let samples = wav::read(wav).map_err(|message| Error::new(ErrorCode::Invalid, message))?;
        route_engine_logs();
        let mut engine = Engine::load(&files.model, &files.vad)
            .map_err(|error| stopped(Exit::ModelLoad(error.to_string())))?;
        if computes_on_vulkan() && !skald_engine::gpu_available() {
            eprintln!("{}", Line::NoVulkan);
        }
        let prompt = config.vocabulary.prompt();
        let options = TakeOptions {
            language: config.language,
            prompt: &prompt,
            decoding: Decoding::Greedy,
        };
        let take = engine.take(
            &samples,
            wav::loudest_dbfs(&samples),
            &options,
            &config.silence,
        );
        drop(engine);
        take.map(transcribed)
            .map_err(|error| failed(error.to_string()))
    }
}

/// Whether this system's GPU is reached through Vulkan, so a machine without a Vulkan driver computes on the
/// CPU and is told so; a Mac has Metal.
pub(crate) fn computes_on_vulkan() -> bool {
    cfg!(any(windows, target_os = "linux"))
}

/// The settings in use. The loop puts each configuration it reads here; the socket reads them, and tells
/// the loop when it has written the file, so the change applies at once.
#[derive(Debug)]
pub(crate) struct Settings {
    config: Mutex<Arc<Config>>,
    written: AtomicBool,
}

impl Settings {
    pub(crate) fn new(config: Arc<Config>) -> Self {
        Self {
            config: Mutex::new(config),
            written: AtomicBool::new(false),
        }
    }

    pub(crate) fn current(&self) -> Arc<Config> {
        Arc::clone(&self.config.lock().unwrap_or_else(PoisonError::into_inner))
    }

    pub(crate) fn set(&self, config: Arc<Config>) {
        *self.config.lock().unwrap_or_else(PoisonError::into_inner) = config;
    }

    /// Whether the socket wrote the configuration file since last asked.
    pub(crate) fn take_written(&self) -> bool {
        self.written.swap(false, Ordering::AcqRel)
    }
}

/// The running Skald as the command line sees it over the socket.
pub(crate) struct Control {
    paths: Paths,
    settings: Arc<Settings>,
    worker: WorkerHandle,
    /// Held from reading the configuration file to writing it, so two commands at once do not lose each
    /// other's change.
    writing: Mutex<()>,
}

impl Control {
    pub(crate) fn new(paths: Paths, settings: Arc<Settings>, worker: WorkerHandle) -> Self {
        Self {
            paths,
            settings,
            worker,
            writing: Mutex::new(()),
        }
    }

    fn state(&self) -> Result<State, Error> {
        let path = self.paths.state_file();
        files::read_state(&path).map_err(|reason| {
            Error::new(
                ErrorCode::Unreadable,
                format!("Kan ikke lese {}: {reason}", path.display()),
            )
        })
    }
}

impl Skald for Control {
    fn config(&self) -> Result<Config, Error> {
        Ok((*self.settings.current()).clone())
    }

    /// Changes the file as it is now, not the settings last read from it, so an edit not yet picked up is
    /// kept, and a file broken by an edit in progress is left alone.
    fn change_config(
        &self,
        change: &mut dyn FnMut(&mut Config) -> Result<(), Error>,
    ) -> Result<(), Error> {
        let _writing = self.writing.lock().unwrap_or_else(PoisonError::into_inner);
        let path = self.paths.config_file();
        let before = files::read_config(&path)
            .map_err(|error| Error::new(ErrorCode::Unreadable, error.to_string()))?;
        let mut config = before.clone();
        change(&mut config)?;
        if config != before {
            files::write_config(&path, &config).map_err(|reason| {
                Error::new(ErrorCode::Io, format!("Kan ikke skrive til {reason}"))
            })?;
            self.settings.set(Arc::new(config));
            self.settings.written.store(true, Ordering::Release);
        }
        Ok(())
    }

    fn history(&self) -> Result<History, Error> {
        Ok(self.state()?.history)
    }

    fn correction_counts(&self) -> Result<Counts, Error> {
        Ok(self.state()?.counts)
    }

    fn prompt_tokens(&self, prompt: &Prompt) -> Option<Tokens> {
        let used = self.worker.count_tokens(&prompt.text())?;
        Some(Tokens {
            used,
            limit: PROMPT_TOKENS,
        })
    }

    fn microphones(&self) -> Result<Microphones, Error> {
        microphones(&self.settings.current())
    }

    fn models(&self) -> Result<Vec<Model>, Error> {
        Ok(models(&self.paths))
    }

    fn transcribe(&self, wav: &Path) -> Result<Transcribed, Error> {
        let samples = wav::read(wav).map_err(|message| Error::new(ErrorCode::Invalid, message))?;
        self.worker
            .transcribe(samples, self.settings.current())
            .map(transcribed)
            .map_err(failed)
    }

    fn press_hold_key(&self, down: bool) -> Result<(), Error> {
        skald_input::press_hold_key(down).map_err(refused)
    }

    fn toggle_hands_free(&self) -> Result<(), Error> {
        skald_input::toggle_hands_free().map_err(refused)
    }
}

/// Why a key binding's press reached no take, for the command line.
fn refused(refused: PressRefused) -> Error {
    match refused {
        PressRefused::KeyHeard => Error::new(
            ErrorCode::Invalid,
            "Her lytter Skald selv på tastaturet, så «skald start», «skald stop» og «skald toggle» \
             trengs ikke.",
        ),
        PressRefused::NotListening => {
            Error::new(ErrorCode::Failed, "Skald lytter ikke etter tasten nå.")
        }
    }
}

/// The microphones connected now, and the one a take would use with the configuration's order and pin.
fn microphones(config: &Config) -> Result<Microphones, Error> {
    let connected = skald_audio::connected_microphones().map_err(|error| {
        failed(format!(
            "Får ikke lest mikrofonene: {}.",
            capture_reason(&error)
        ))
    })?;
    Ok(microphones_in(&connected, config))
}

fn microphones_in(connected: &Connected, config: &Config) -> Microphones {
    Microphones {
        connected: connected
            .microphones
            .iter()
            .map(|microphone| microphone.name.clone())
            .collect(),
        active: skald_audio::choose(&microphone_order(config), connected)
            .map(|microphone| microphone.name.clone()),
    }
}

/// The registry's models, a file of the pinned size counting as downloaded: Skald checks one put there by
/// hand before it uses it.
fn models(paths: &Paths) -> Vec<Model> {
    let dir = paths.models_dir();
    MODELS
        .iter()
        .map(|model| Model {
            id: model.id.to_owned(),
            name: model.name.to_owned(),
            downloaded: matches!(model.status(&dir), Some(Status::Ready | Status::Unchecked)),
        })
        .collect()
}

fn transcribed(take: Take) -> Transcribed {
    match (take.decision, take.transcript) {
        (Decision::HeardNothing(reason), _) => Transcribed::HeardNothing(reason),
        (Decision::Insert, transcript) => {
            Transcribed::Text(transcript.map(|t| t.text).unwrap_or_default())
        }
    }
}

fn failed(message: String) -> Error {
    Error::new(ErrorCode::Failed, message)
}

/// A reason Skald would not start, as the command line's error.
fn stopped(exit: Exit) -> Error {
    let code = match exit {
        Exit::UnknownModel(_) | Exit::ModelMissing { .. } | Exit::VadMissing { .. } => {
            ErrorCode::NotFound
        }
        _ => ErrorCode::Failed,
    };
    Error::new(code, exit.to_string())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::mpsc;

    use skald_audio::{MicId, Microphone};
    use skald_core::config::Language;
    use skald_core::history::Entry;
    use skald_core::silence_gate::Reason;

    use super::*;
    use crate::files::tests::Scratch;
    use crate::pipeline::Worker;
    use crate::pipeline::tests::{FakeInserter, FakeModel, heard, quiet, silent};
    use skald_input::Inserted;

    struct Running {
        control: Control,
        settings: Arc<Settings>,
        worker: Worker,
        paths: Paths,
        _scratch: Scratch,
    }

    /// A running Skald whose model has loaded, so its counts answer.
    fn running(name: &str, takes: Vec<Take>) -> Running {
        let scratch = Scratch::new(name);
        let paths = Paths::in_one_folder(&scratch.0);
        let model = FakeModel::new(takes.into_iter().map(Ok));
        let (events, received) = mpsc::channel();
        let worker = Worker::spawn(
            move || Ok(model),
            FakeInserter::answering(Ok(Inserted::PreviousRestored)),
            quiet(),
            State::default(),
            paths.state_file(),
            events,
        )
        .unwrap();
        assert!(matches!(
            received.recv_timeout(std::time::Duration::from_secs(5)),
            Ok(crate::pipeline::WorkerEvent::Loaded { .. })
        ));
        let settings = Arc::new(Settings::new(Arc::new(Config::default())));
        let control = Control::new(paths.clone(), Arc::clone(&settings), worker.handle());
        Running {
            control,
            settings,
            worker,
            paths,
            _scratch: scratch,
        }
    }

    fn add_word(word: &'static str) -> impl FnMut(&mut Config) -> Result<(), Error> {
        move |config| {
            config.vocabulary.add_word(word);
            Ok(())
        }
    }

    #[test]
    fn a_change_writes_the_file_once_and_tells_the_loop() {
        let run = running("control-change", Vec::new());
        let path = run.paths.config_file();
        run.control.change_config(&mut add_word("merget")).unwrap();
        let written = fs::read_to_string(&path).unwrap();
        assert!(written.contains("merget"), "{written}");
        assert_eq!(run.settings.current().vocabulary.words, ["merget"]);
        assert!(run.settings.take_written());
        assert!(!run.settings.take_written(), "said once");
        assert_eq!(run.control.config().unwrap().vocabulary.words, ["merget"]);

        // The same change again changes nothing, so the file is not written.
        fs::write(&path, format!("# a comment\n{written}")).unwrap();
        run.control.change_config(&mut add_word("merget")).unwrap();
        assert!(
            fs::read_to_string(&path)
                .unwrap()
                .starts_with("# a comment")
        );
        assert!(!run.settings.take_written());
    }

    #[test]
    fn a_change_starts_from_the_file_and_leaves_a_broken_one_alone() {
        let run = running("control-file", Vec::new());
        let path = run.paths.config_file();
        fs::write(&path, "language = \"nynorsk\"\n").unwrap();
        run.control.change_config(&mut add_word("Skald")).unwrap();
        let config = run.settings.current();
        assert_eq!(
            config.language,
            Language::Nynorsk,
            "the edit not yet read is kept"
        );
        assert_eq!(config.vocabulary.words, ["Skald"]);

        fs::write(&path, "language = \"svensk\"\n").unwrap();
        let error = run
            .control
            .change_config(&mut add_word("merget"))
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::Unreadable);
        assert!(error.message.contains("svensk"), "{}", error.message);
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "language = \"svensk\"\n"
        );

        fs::write(&path, "language = \"bokmal\"\n").unwrap();
        let refused = run
            .control
            .change_config(&mut |_| Err(Error::new(ErrorCode::Invalid, "nei")));
        assert_eq!(refused.unwrap_err().code, ErrorCode::Invalid);
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "language = \"bokmal\"\n"
        );
    }

    #[test]
    fn the_history_and_counts_come_from_the_state_file() {
        let run = running("control-state", Vec::new());
        assert!(run.control.history().unwrap().latest().is_none());
        let mut state = State::default();
        state.history.push(Entry {
            text: "Hei.".into(),
            unix_ms: 1,
            microphone: "Shure MV7".into(),
        });
        files::write_state(&run.paths.state_file(), &state).unwrap();
        assert_eq!(
            run.control.history().unwrap().latest().unwrap().text,
            "Hei."
        );
        assert_eq!(run.control.correction_counts().unwrap(), Counts::default());

        fs::write(run.paths.state_file(), "[[texts]\n").unwrap();
        assert_eq!(
            run.control.history().unwrap_err().code,
            ErrorCode::Unreadable
        );
    }

    #[test]
    fn the_meter_and_a_file_go_through_the_loaded_model() {
        let run = running(
            "control-model",
            vec![
                heard("Hei, dette er en prøve."),
                silent(Reason::TooLittleSpeech),
            ],
        );
        let mut prompt_source = Config::default();
        prompt_source.vocabulary.sentence = "Vi merget den.".into();
        assert_eq!(
            run.control
                .prompt_tokens(&prompt_source.vocabulary.prompt()),
            Some(Tokens {
                used: 3,
                limit: PROMPT_TOKENS
            })
        );

        let wav_path = run.paths.models_dir().with_file_name("take.wav");
        crate::wav::tests::write_tone(&wav_path, 16_000, 1, false);
        assert_eq!(
            run.control.transcribe(&wav_path).unwrap(),
            Transcribed::Text("Hei, dette er en prøve.".into())
        );
        assert_eq!(
            run.control.transcribe(&wav_path).unwrap(),
            Transcribed::HeardNothing(Reason::TooLittleSpeech)
        );
        let error = run
            .control
            .transcribe(&wav_path.with_file_name("borte.wav"))
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::Invalid);

        run.worker.finish();
        assert_eq!(
            run.control
                .prompt_tokens(&prompt_source.vocabulary.prompt()),
            None
        );
    }

    #[test]
    fn the_command_line_talks_to_the_running_skald_over_its_socket() {
        let run = running("control-socket", Vec::new());
        let serving = skald_control::Server::bind(&run.paths.socket())
            .unwrap()
            .serve(Arc::new(Control::new(
                run.paths.clone(),
                Arc::clone(&run.settings),
                run.worker.handle(),
            )));
        let machine = LocalMachine::new(run.paths.clone());
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let args = ["vocab", "add", "merget", "--json"].map(std::ffi::OsString::from);
        let code = skald_control::run_with(args, &run.paths, &machine, &mut out, &mut err);
        let out = String::from_utf8(out).unwrap();
        assert_eq!(code, 0, "{out} {}", String::from_utf8_lossy(&err));
        assert!(out.contains("\"running\":true"), "{out}");
        assert!(run.settings.take_written());
        assert!(
            fs::read_to_string(run.paths.config_file())
                .unwrap()
                .contains("merget")
        );
        serving.stop();
        assert!(!run.paths.socket().exists());
    }

    #[test]
    fn the_models_are_the_registry_s_with_what_is_here() {
        let scratch = Scratch::new("control-models");
        let models = models(&Paths::in_one_folder(&scratch.0));
        let ids: Vec<&str> = models.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(ids, ["nb-whisper-large", "nb-asr-bokfink", "nordlyd"]);
        assert_eq!(models[0].name, "NB-Whisper Large");
        assert!(models.iter().all(|m| !m.downloaded));
    }

    #[test]
    fn the_microphone_in_use_follows_the_pin_and_the_order() {
        let mic = |name: &str| Microphone {
            id: MicId::new(format!("id-{name}")),
            name: name.to_owned(),
        };
        let connected = Connected {
            microphones: vec![mic("MacBook Pro-mikrofon"), mic("Yeti")],
            default: Some(MicId::new("id-MacBook Pro-mikrofon")),
        };
        let mut config = Config::default();
        assert_eq!(
            microphones_in(&connected, &config),
            Microphones {
                connected: vec!["MacBook Pro-mikrofon".into(), "Yeti".into()],
                active: Some("MacBook Pro-mikrofon".into()),
            }
        );
        config.microphones = vec!["Yeti".into()];
        assert_eq!(
            microphones_in(&connected, &config).active.as_deref(),
            Some("Yeti")
        );
        config.pinned_microphone = Some("MacBook Pro-mikrofon".into());
        assert_eq!(
            microphones_in(&connected, &config).active.as_deref(),
            Some("MacBook Pro-mikrofon")
        );
    }

    #[test]
    fn the_hold_key_is_pressed_only_where_a_key_binding_holds_it() {
        let run = running("control-hold-key", Vec::new());
        for refused in [
            run.control.press_hold_key(true).unwrap_err(),
            run.control.toggle_hands_free().unwrap_err(),
        ] {
            if cfg!(target_os = "linux") {
                // No listener runs in the test, so the press reaches nothing.
                assert_eq!(refused.code, ErrorCode::Failed);
            } else {
                assert_eq!(refused.code, ErrorCode::Invalid);
                assert!(
                    refused.message.contains("«skald start»"),
                    "{}",
                    refused.message
                );
            }
        }
    }

    #[test]
    fn a_missing_model_is_not_found_for_the_command_line() {
        let scratch = Scratch::new("control-machine");
        let machine = LocalMachine::new(Paths::in_one_folder(&scratch.0));
        let error = machine
            .transcribe(&Config::default(), &scratch.0.join("take.wav"))
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::NotFound);
        assert!(
            error.message.starts_with("Fant ikke talemodellen"),
            "{}",
            error.message
        );
    }
}
