//! A folder of Skald's own for each test, and stand-ins for the machine and the running app, so the
//! tests need no model, no microphone and no window.

#![allow(dead_code)]

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use skald_control::protocol::Tokens;
use skald_control::{Error, Machine, Microphones, Model, Skald, Transcribed};
use skald_core::Config;
use skald_core::corrections::Counts;
use skald_core::history::History;
use skald_core::paths::Paths;
use skald_core::silence_gate::Reason;
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
}

impl FakeApp {
    pub fn new(config: Config, history: History, counts: Counts) -> Self {
        Self {
            config: Mutex::new(config),
            history,
            counts,
            changes: AtomicUsize::new(0),
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
