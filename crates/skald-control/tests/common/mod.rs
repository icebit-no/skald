//! A folder of Skald's own for each test, and stand-ins for the machine and the running app, so the
//! tests need no model, no microphone and no window; and every command with the JSON it prints, which
//! the command line and the MCP tools are both held to.

#![allow(dead_code)]

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use serde_json::json;
use skald_control::protocol::Tokens;
use skald_control::{Error, Machine, Microphones, Model, Skald, Transcribed};
use skald_core::Config;
use skald_core::corrections::Counts;
use skald_core::history::{Entry, History};
use skald_core::paths::Paths;
use skald_core::silence_gate::Reason;
use skald_core::state::State;
use skald_core::vocabulary::Prompt;

/// A folder under the system's temporary folder, removed when dropped.
pub struct Folder {
    pub path: PathBuf,
}

impl Folder {
    pub fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "skald-control-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Self { path }
    }

    pub fn paths(&self) -> Paths {
        Paths::in_one_folder(&self.path)
    }
}

impl Drop for Folder {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

pub const BUILT_IN: &str = "MacBook Pro-mikrofon";
pub const USB: &str = "Shure MV7";

/// Two microphones connected, NB-Whisper downloaded and Bokfink not, and a model that writes «mercha»
/// for «merget», or hears nothing in a file whose name says «stille».
pub struct FakeMachine;

impl Machine for FakeMachine {
    fn microphones(&self, config: &Config) -> Result<Microphones, Error> {
        let connected = vec![BUILT_IN.to_owned(), USB.to_owned()];
        let active = config
            .pinned_microphone
            .iter()
            .chain(&config.microphones)
            .find(|name| connected.contains(name))
            .or(connected.first())
            .cloned();
        Ok(Microphones { connected, active })
    }

    fn models(&self) -> Result<Vec<Model>, Error> {
        Ok(vec![
            Model {
                id: "nb-whisper-large".into(),
                name: "NB-Whisper Large".into(),
                downloaded: true,
            },
            Model {
                id: "nb-asr-bokfink".into(),
                name: "NB-ASR Bokfink".into(),
                downloaded: false,
            },
        ])
    }

    fn transcribe(&self, _config: &Config, wav: &Path) -> Result<Transcribed, Error> {
        if wav.to_string_lossy().contains("stille") {
            Ok(Transcribed::HeardNothing(Reason::TooLittleSpeech))
        } else {
            Ok(Transcribed::Text("Vi mercha pull-requesten.".into()))
        }
    }
}

/// The running app: its configuration and state in memory, with a model to count tokens with. It counts
/// one token per character, so the meter is easy to check.
pub struct FakeApp {
    pub config: Mutex<Config>,
    pub history: History,
    pub counts: Counts,
    pub changes: AtomicUsize,
    /// Each press (true) and release of the hold key, in order.
    pub presses: Mutex<Vec<bool>>,
    /// How often the hands-free chord was pressed.
    pub toggles: AtomicUsize,
}

impl FakeApp {
    pub fn new(config: Config, history: History, counts: Counts) -> Self {
        Self {
            config: Mutex::new(config),
            history,
            counts,
            changes: AtomicUsize::new(0),
            presses: Mutex::new(Vec::new()),
            toggles: AtomicUsize::new(0),
        }
    }
}

impl Skald for FakeApp {
    fn config(&self) -> Result<Config, Error> {
        Ok(self.config.lock().unwrap().clone())
    }

    fn change_config(
        &self,
        change: &mut dyn FnMut(&mut Config) -> Result<(), Error>,
    ) -> Result<(), Error> {
        let mut config = self.config.lock().unwrap();
        let mut changed = config.clone();
        change(&mut changed)?;
        if changed != *config {
            *config = changed;
            self.changes.fetch_add(1, Ordering::SeqCst);
        }
        Ok(())
    }

    fn history(&self) -> Result<History, Error> {
        Ok(self.history.clone())
    }

    fn correction_counts(&self) -> Result<Counts, Error> {
        Ok(self.counts.clone())
    }

    fn prompt_tokens(&self, prompt: &Prompt) -> Option<Tokens> {
        Some(Tokens {
            used: prompt.text().chars().count(),
            limit: 223,
        })
    }

    fn microphones(&self) -> Result<Microphones, Error> {
        FakeMachine.microphones(&self.config()?)
    }

    fn models(&self) -> Result<Vec<Model>, Error> {
        FakeMachine.models()
    }

    fn transcribe(&self, wav: &Path) -> Result<Transcribed, Error> {
        FakeMachine.transcribe(&self.config()?, wav)
    }

    fn press_hold_key(&self, down: bool) -> Result<(), Error> {
        self.presses.lock().unwrap().push(down);
        Ok(())
    }

    fn toggle_hands_free(&self) -> Result<(), Error> {
        self.toggles.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

/// What a command printed and how it ended.
#[derive(Debug)]
pub struct Ran {
    pub code: u8,
    pub out: String,
    pub err: String,
}

impl Ran {
    pub fn json(&self) -> serde_json::Value {
        serde_json::from_str(&self.out)
            .unwrap_or_else(|error| panic!("{error}: {:?} {:?}", self.out, self.err))
    }
}

pub fn skald(paths: &Paths, args: &[&str]) -> Ran {
    let args: Vec<OsString> = args.iter().map(OsString::from).collect();
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let code = skald_control::run_with(args, paths, &FakeMachine, &mut out, &mut err);
    Ran {
        code,
        out: String::from_utf8(out).unwrap(),
        err: String::from_utf8(err).unwrap(),
    }
}

pub const CONFIG: &str = r#"
future_flag = true
hold_key = "right-command"
language = "bokmal"
microphones = ["Shure MV7", "Yeti"]

[vocabulary]
sentence = "Vi merget pull-requesten i går."
words = ["Skald", "PostgreSQL"]
weights = [1, 2]

[[corrections]]
from = "mercha"
to = "merget"
enabled = true

[[corrections]]
from = "Post gres SQL"
to = "PostgreSQL"

[overlay]
position = "top"
"#;

pub fn state(config: &Config) -> State {
    let mut counts = Counts::default();
    config
        .corrections
        .apply_and_count("Vi mercha den. Han mercha.", &mut counts);
    let mut history = History::default();
    history.push(Entry {
        text: "Hei, dette er en prøve.".into(),
        unix_ms: 1_795_799_000_000,
        microphone: BUILT_IN.into(),
    });
    history.push(Entry {
        text: "Vi merget pull-requesten.".into(),
        unix_ms: 1_795_800_000_000,
        microphone: USB.into(),
    });
    State { history, counts }
}

/// Skald's folder with the configuration and state above, and two audio files.
pub fn folder() -> (Folder, Paths) {
    let folder = Folder::new();
    let paths = folder.paths();
    fs::write(paths.config_file(), CONFIG).unwrap();
    let config = Config::from_toml(CONFIG).unwrap();
    fs::write(paths.state_file(), state(&config).to_toml().unwrap()).unwrap();
    fs::write(folder.path.join("opptak.wav"), b"").unwrap();
    fs::write(folder.path.join("stille.wav"), b"").unwrap();
    (folder, paths)
}

/// The same folder with Skald running on it, holding the same configuration and state.
#[cfg(unix)]
pub fn running(paths: &Paths) -> (std::sync::Arc<FakeApp>, skald_control::Serving) {
    let config = Config::from_toml(CONFIG).unwrap();
    let State { history, counts } = state(&config);
    let app = std::sync::Arc::new(FakeApp::new(config, history, counts));
    let serving = skald_control::Server::bind(&paths.socket())
        .unwrap()
        .serve(app.clone());
    (app, serving)
}

pub fn wav(folder: &Folder, name: &str) -> String {
    folder.path.join(name).to_string_lossy().into_owned()
}

/// Each command with the JSON it prints while Skald is not running, in order: later ones see what the
/// earlier ones changed.
pub fn commands_and_json(folder: &Folder) -> Vec<(Vec<String>, serde_json::Value)> {
    let opptak = wav(folder, "opptak.wav");
    let stille = wav(folder, "stille.wav");
    let cases: Vec<(Vec<&str>, serde_json::Value)> = vec![
        (
            vec!["last"],
            json!({"running": false, "last": {
                "text": "Vi merget pull-requesten.", "unix_ms": 1_795_800_000_000_i64,
                "microphone": USB}}),
        ),
        (
            vec!["history"],
            json!({"running": false, "texts": [
                {"text": "Vi merget pull-requesten.", "unix_ms": 1_795_800_000_000_i64,
                 "microphone": USB},
                {"text": "Hei, dette er en prøve.", "unix_ms": 1_795_799_000_000_i64,
                 "microphone": BUILT_IN},
            ]}),
        ),
        (
            vec!["fix", "list"],
            json!({"running": false, "corrections": [
                {"from": "mercha", "to": "merget", "applied": 2},
                {"from": "Post gres SQL", "to": "PostgreSQL", "applied": 0},
            ]}),
        ),
        (
            vec!["fix", "add", "Mercha", "merge"],
            json!({"running": false, "from": "Mercha", "to": "merge", "previous": "merget"}),
        ),
        (
            vec!["fix", "add", "Hei", "Hallo"],
            json!({"running": false, "from": "Hei", "to": "Hallo", "previous": null}),
        ),
        (
            vec!["fix", "remove", "hei"],
            json!({"running": false, "from": "Hei", "to": "Hallo"}),
        ),
        (
            vec!["vocab", "list"],
            json!({"running": false, "sentence": "Vi merget pull-requesten i går.",
                   "words": ["Skald", "PostgreSQL"], "tokens": null}),
        ),
        (
            vec!["vocab", "add", "merget"],
            json!({"running": false, "word": "merget", "changed": true, "tokens": null}),
        ),
        (
            vec!["vocab", "add", " merget "],
            json!({"running": false, "word": "merget", "changed": false, "tokens": null}),
        ),
        (
            vec!["vocab", "remove", "Skald"],
            json!({"running": false, "word": "Skald", "changed": true, "tokens": null}),
        ),
        (
            vec!["vocab", "sentence", "Vi merget den."],
            json!({"running": false, "sentence": "Vi merget den.",
                   "words": ["PostgreSQL", "merget"], "tokens": null}),
        ),
        (
            vec!["config", "get"],
            json!({"running": false, "settings": {
                "hold_key": "right-command", "language": "bokmal", "overlay_theme": "runepinne",
                "copy_to_clipboard": false, "start_at_login": false, "sounds": true}}),
        ),
        (
            vec!["config", "set", "language", "nynorsk"],
            json!({"running": false, "key": "language", "value": "nynorsk"}),
        ),
        (
            vec!["config", "set", "sounds", "av"],
            json!({"running": false, "key": "sounds", "value": false}),
        ),
        (
            vec!["config", "set", "overlay-theme", "smidd-jern"],
            json!({"running": false, "key": "overlay_theme", "value": "smidd-jern"}),
        ),
        (
            vec!["config", "get", "language"],
            json!({"running": false, "key": "language", "value": "nynorsk"}),
        ),
        (
            vec!["mic", "list"],
            json!({"running": false, "pinned": null, "active": USB, "microphones": [
                {"name": USB, "position": 1, "connected": true, "pinned": false, "active": true},
                {"name": "Yeti", "position": 2, "connected": false, "pinned": false,
                 "active": false},
                {"name": BUILT_IN, "position": null, "connected": true, "pinned": false,
                 "active": false},
            ]}),
        ),
        (
            vec!["mic", "use", "macbook pro-mikrofon"],
            json!({"running": false, "pinned": BUILT_IN}),
        ),
        (
            vec!["mic", "list"],
            json!({"running": false, "pinned": BUILT_IN, "active": BUILT_IN, "microphones": [
                {"name": USB, "position": 1, "connected": true, "pinned": false, "active": false},
                {"name": "Yeti", "position": 2, "connected": false, "pinned": false,
                 "active": false},
                {"name": BUILT_IN, "position": null, "connected": true, "pinned": true,
                 "active": true},
            ]}),
        ),
        (
            vec!["mic", "use", "--first"],
            json!({"running": false, "pinned": null}),
        ),
        (
            vec!["mic", "order", BUILT_IN, "yeti", USB],
            json!({"running": false, "order": [BUILT_IN, "Yeti", USB]}),
        ),
        (
            vec!["model", "list"],
            json!({"running": false, "models": [
                {"id": "nb-whisper-large", "name": "NB-Whisper Large", "downloaded": true,
                 "active": true},
                {"id": "nb-asr-bokfink", "name": "NB-ASR Bokfink", "downloaded": false,
                 "active": false},
            ]}),
        ),
        (
            vec!["model", "use", "NB-Whisper Large"],
            json!({"running": false, "id": "nb-whisper-large", "name": "NB-Whisper Large"}),
        ),
        (
            vec!["transcribe", &opptak],
            json!({"running": false, "text": "Vi merge pull-requesten.",
                   "model_text": "Vi mercha pull-requesten.",
                   "corrections": [{"from": "Mercha", "to": "merge"}], "heard_nothing": null}),
        ),
        (
            vec!["transcribe", &stille],
            json!({"running": false, "text": null, "model_text": null, "corrections": [],
                   "heard_nothing": "too_little_speech"}),
        ),
    ];
    cases
        .into_iter()
        .map(|(words, expected)| {
            let mut args: Vec<String> = words.into_iter().map(str::to_owned).collect();
            args.push("--json".to_owned());
            (args, expected)
        })
        .collect()
}
