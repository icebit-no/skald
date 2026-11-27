//! Skald's files on disk: the configuration, read at start and again when it changes; the state, written
//! after a take; and the lock that lets one Skald at a time take dictation.

use std::fmt;
use std::fs::{self, File, TryLockError};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use skald_core::Config;
use skald_core::state::State;

/// A configuration file that could not be read, with the file and the reason, the line among it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ConfigError {
    pub(crate) path: PathBuf,
    pub(crate) reason: String,
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Feil i {}: {}",
            self.path.display(),
            self.reason.trim_end()
        )
    }
}

/// The configuration file as last read. Skald never writes it: it changes only when a setting does (D-19).
#[derive(Debug)]
pub(crate) struct ConfigFile {
    path: PathBuf,
    /// The modification time and length last seen, which tell a change without reading the file.
    stamp: Option<(SystemTime, u64)>,
    text: Option<String>,
}

impl ConfigFile {
    /// Reads the configuration at start. A missing file is the default configuration and is not created; a
    /// broken one is an error, so a setting the user made is never silently replaced by a default.
    pub(crate) fn open(path: PathBuf) -> Result<(Self, Config), ConfigError> {
        let mut file = Self {
            path,
            stamp: None,
            text: None,
        };
        let config = file.changed().transpose()?.unwrap_or_default();
        Ok((file, config))
    }

    pub(crate) fn exists(&self) -> bool {
        self.text.is_some()
    }

    /// The configuration again when the file has changed since it was last read. A file that disappears
    /// keeps the settings in use, as an editor may remove a file for a moment while saving it.
    pub(crate) fn changed(&mut self) -> Option<Result<Config, ConfigError>> {
        let stamp = fs::metadata(&self.path).ok().map(|meta| {
            (
                meta.modified().unwrap_or(SystemTime::UNIX_EPOCH),
                meta.len(),
            )
        })?;
        if self.stamp == Some(stamp) {
            return None;
        }
        self.stamp = Some(stamp);
        let text = match fs::read_to_string(&self.path) {
            Ok(text) => text,
            Err(error) if error.kind() == ErrorKind::NotFound => return None,
            Err(error) => return Some(Err(self.error(error))),
        };
        if self.text.as_deref() == Some(text.as_str()) {
            return None;
        }
        let config = Config::from_toml(&text).map_err(|error| self.error(error));
        // A broken text is remembered too, so it is reported once rather than at every look.
        self.text = Some(text);
        Some(config)
    }

    fn error(&self, reason: impl fmt::Display) -> ConfigError {
        ConfigError {
            path: self.path.clone(),
            reason: reason.to_string(),
        }
    }
}

/// The history and correction counts. A missing file is an empty state.
pub(crate) fn read_state(path: &Path) -> Result<State, String> {
    match fs::read_to_string(path) {
        Ok(text) => State::from_toml(&text).map_err(|error| error.to_string()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(State::default()),
        Err(error) => Err(error.to_string()),
    }
}

/// Writes the state through a file beside it that then takes its place, so a reader never meets half a
/// file. The error names the file.
pub(crate) fn write_state(path: &Path, state: &State) -> Result<(), String> {
    let failed = |error: &dyn fmt::Display| format!("{}: {error}", path.display());
    let text = state.to_toml().map_err(|error| failed(&error))?;
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|error| failed(&error))?;
    }
    let beside = path.with_extension("toml.new");
    fs::write(&beside, text).map_err(|error| failed(&error))?;
    fs::rename(&beside, path).map_err(|error| failed(&error))
}

/// Held while Skald takes dictation, so a second Skald started by mistake does not paste every take a
/// second time. The system releases it when the process ends, however it ends.
#[derive(Debug)]
pub(crate) struct InstanceLock {
    _file: File,
}

#[derive(Debug)]
pub(crate) enum LockError {
    Held,
    Failed(String),
}

pub(crate) fn lock(path: &Path) -> Result<InstanceLock, LockError> {
    let failed = |error: std::io::Error| LockError::Failed(format!("{}: {error}", path.display()));
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(failed)?;
    }
    let file = File::options()
        .create(true)
        .write(true)
        .truncate(false)
        .open(path)
        .map_err(failed)?;
    match file.try_lock() {
        Ok(()) => Ok(InstanceLock { _file: file }),
        Err(TryLockError::WouldBlock) => Err(LockError::Held),
        Err(TryLockError::Error(error)) => Err(failed(error)),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use skald_core::config::Language;
    use skald_core::corrections::{Correction, Corrections};
    use skald_core::history::Entry;

    /// A fresh folder of its own for one test, removed when dropped.
    pub(crate) struct Scratch(pub(crate) PathBuf);

    impl Scratch {
        pub(crate) fn new(name: &str) -> Self {
            let dir =
                std::env::temp_dir().join(format!("skald-test-{}-{name}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn a_missing_configuration_is_the_default_and_is_not_created() {
        let scratch = Scratch::new("config-missing");
        let path = scratch.0.join("config.toml");
        let (mut file, config) = ConfigFile::open(path.clone()).unwrap();
        assert_eq!(config, Config::default());
        assert!(!file.exists());
        assert!(file.changed().is_none());
        assert!(!path.exists());
    }

    #[test]
    fn a_broken_configuration_at_start_names_the_file_and_the_line() {
        let scratch = Scratch::new("config-broken");
        let path = scratch.0.join("config.toml");
        fs::write(&path, "language = \"nynorsk\"\nsounds = \"ja\"\n").unwrap();
        let error = ConfigFile::open(path.clone()).unwrap_err();
        assert_eq!(error.path, path);
        let shown = error.to_string();
        assert!(shown.starts_with("Feil i "), "{shown}");
        assert!(shown.contains("line 2"), "{shown}");
        assert!(shown.contains("sounds"), "{shown}");
    }

    #[test]
    fn a_change_is_read_once_and_a_broken_change_keeps_nothing_new() {
        let scratch = Scratch::new("config-change");
        let path = scratch.0.join("config.toml");
        fs::write(&path, "language = \"nynorsk\"\n").unwrap();
        let (mut file, config) = ConfigFile::open(path.clone()).unwrap();
        assert_eq!(config.language, Language::Nynorsk);
        assert!(file.exists());
        assert!(file.changed().is_none(), "unchanged");

        fs::write(&path, "language = \"bokmal\"\ncopy_to_clipboard = true\n").unwrap();
        let config = file.changed().unwrap().unwrap();
        assert_eq!(config.language, Language::Bokmal);
        assert!(config.copy_to_clipboard);
        assert!(file.changed().is_none(), "read once");

        fs::write(&path, "language = \"bokmal\"\ncopy_to_clipboard = true\n").unwrap();
        assert!(file.changed().is_none(), "the same text written again");

        fs::write(&path, "language = \"svensk\"\n").unwrap();
        let error = file.changed().unwrap().unwrap_err();
        assert!(error.to_string().contains("svensk"), "{error}");
        assert!(file.changed().is_none(), "reported once");

        fs::remove_file(&path).unwrap();
        assert!(
            file.changed().is_none(),
            "a removed file keeps the settings"
        );

        fs::write(&path, "language = \"nynorsk\"\n").unwrap();
        assert_eq!(file.changed().unwrap().unwrap().language, Language::Nynorsk);
    }

    #[test]
    fn a_configuration_made_after_start_is_read() {
        let scratch = Scratch::new("config-late");
        let path = scratch.0.join("config.toml");
        let (mut file, _) = ConfigFile::open(path.clone()).unwrap();
        fs::write(&path, "[vocabulary]\nwords = [\"Skald\"]\n").unwrap();
        let config = file.changed().unwrap().unwrap();
        assert_eq!(config.vocabulary.words, ["Skald"]);
    }

    #[test]
    fn the_state_round_trips_and_a_missing_file_is_empty() {
        let scratch = Scratch::new("state");
        let path = scratch.0.join("a").join("b").join("state.toml");
        assert_eq!(read_state(&path), Ok(State::default()));

        let corrections = Corrections {
            rules: vec![Correction::new("mercha", "merget")],
        };
        let mut state = State::default();
        let text = corrections.apply_and_count("Vi mercha den.", &mut state.counts);
        state.history.push(Entry {
            text,
            unix_ms: 1_795_800_000_000,
            microphone: "Shure MV7".into(),
        });
        write_state(&path, &state).unwrap();
        assert_eq!(read_state(&path), Ok(state));
        assert!(!path.with_extension("toml.new").exists());
    }

    #[test]
    fn a_broken_state_file_says_why() {
        let scratch = Scratch::new("state-broken");
        let path = scratch.0.join("state.toml");
        fs::write(&path, "[[texts]]\ntext = \"uten slutt\n").unwrap();
        assert!(read_state(&path).unwrap_err().contains("line 2"));
    }

    #[test]
    fn one_skald_at_a_time_holds_the_lock() {
        let scratch = Scratch::new("lock");
        let path = scratch.0.join("nested").join("state.lock");
        let held = lock(&path).unwrap();
        assert!(matches!(lock(&path), Err(LockError::Held)));
        drop(held);
        assert!(lock(&path).is_ok());
    }
}
