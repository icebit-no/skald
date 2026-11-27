//! Skald while it is not running: the configuration and state files, and the binary's [`Machine`] for
//! microphones, models and transcription.

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use skald_core::Config;
use skald_core::corrections::Counts;
use skald_core::history::History;
use skald_core::paths::Paths;
use skald_core::state::State;
use skald_core::vocabulary::Prompt;

use crate::protocol::{Error, ErrorCode, Tokens};
use crate::skald::{Machine, Microphones, Model, Skald, Transcribed};

pub(crate) struct Offline<'a> {
    paths: &'a Paths,
    machine: &'a dyn Machine,
}

impl<'a> Offline<'a> {
    pub(crate) fn new(paths: &'a Paths, machine: &'a dyn Machine) -> Self {
        Self { paths, machine }
    }

    fn state(&self) -> Result<State, Error> {
        let path = self.paths.state_file();
        let Some(text) = read_if_present(&path)? else {
            return Ok(State::default());
        };
        State::from_toml(&text).map_err(|error| unreadable(&path, error))
    }
}

impl Skald for Offline<'_> {
    fn config(&self) -> Result<Config, Error> {
        read_config(&self.paths.config_file())
    }

    /// Holds a lock on the configuration's folder from reading to writing, so two commands run at once
    /// do not lose each other's change. The file is replaced in one step, so Skald starting or an editor
    /// never reads half of it.
    fn change_config(
        &self,
        change: &mut dyn FnMut(&mut Config) -> Result<(), Error>,
    ) -> Result<(), Error> {
        let path = self.paths.config_file();
        let folder = path
            .parent()
            .expect("the configuration file is in a folder");
        fs::create_dir_all(folder).map_err(|error| cannot_write(folder, &error))?;
        let _lock = lock(folder)?;
        let before = read_config(&path)?;
        let mut config = before.clone();
        change(&mut config)?;
        if config != before {
            let text = config
                .to_toml()
                .map_err(|error| Error::new(ErrorCode::Failed, error.to_string()))?;
            replace(&path, &text).map_err(|error| cannot_write(&path, &error))?;
        }
        Ok(())
    }

    fn history(&self) -> Result<History, Error> {
        Ok(self.state()?.history)
    }

    fn correction_counts(&self) -> Result<Counts, Error> {
        Ok(self.state()?.counts)
    }

    /// Counting needs the model's tokenizer, which only a running Skald has loaded.
    fn prompt_tokens(&self, _prompt: &Prompt) -> Option<Tokens> {
        None
    }

    fn microphones(&self) -> Result<Microphones, Error> {
        self.machine.microphones(&self.config()?)
    }

    fn models(&self) -> Result<Vec<Model>, Error> {
        self.machine.models()
    }

    fn transcribe(&self, wav: &Path) -> Result<Transcribed, Error> {
        self.machine.transcribe(&self.config()?, wav)
    }

    /// The hold key belongs to the running Skald's takes, so with none running there is nothing to press.
    fn press_hold_key(&self, _down: bool) -> Result<(), Error> {
        Err(Error::new(
            ErrorCode::NotRunning,
            "Skald kjører ikke. «skald start» og «skald stop» virker bare mens Skald kjører.",
        ))
    }
}

/// The configuration in the file, or the default when there is no file yet.
fn read_config(path: &Path) -> Result<Config, Error> {
    match read_if_present(path)? {
        Some(text) => Config::from_toml(&text).map_err(|error| unreadable(path, error)),
        None => Ok(Config::default()),
    }
}

fn read_if_present(path: &Path) -> Result<Option<String>, Error> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(cannot_read(path, &error)),
    }
}

#[cfg(unix)]
fn lock(folder: &Path) -> Result<File, Error> {
    // A lock on the folder itself leaves no lock file behind among the user's files.
    let handle = File::open(folder).map_err(|error| cannot_write(folder, &error))?;
    handle
        .lock()
        .map_err(|error| cannot_write(folder, &error))?;
    Ok(handle)
}

#[cfg(not(unix))]
fn lock(folder: &Path) -> Result<File, Error> {
    let path = folder.join("config.lock");
    let handle = File::create(&path).map_err(|error| cannot_write(&path, &error))?;
    handle.lock().map_err(|error| cannot_write(&path, &error))?;
    Ok(handle)
}

/// Writes the text beside the file and renames it into place.
fn replace(path: &Path, text: &str) -> io::Result<()> {
    let name = path
        .file_name()
        .expect("the configuration file has a name")
        .to_string_lossy();
    let temporary: PathBuf = path.with_file_name(format!(".{name}.{}.tmp", std::process::id()));
    let written = File::create(&temporary).and_then(|mut file| {
        file.write_all(text.as_bytes())?;
        file.sync_all()
    });
    match written.and_then(|()| fs::rename(&temporary, path)) {
        Ok(()) => Ok(()),
        Err(error) => {
            let _ = fs::remove_file(&temporary);
            Err(error)
        }
    }
}

fn unreadable(path: &Path, error: impl std::fmt::Display) -> Error {
    Error::new(
        ErrorCode::Unreadable,
        format!("Kan ikke lese {}: {error}", path.display()),
    )
}

fn cannot_read(path: &Path, error: &io::Error) -> Error {
    Error::new(
        ErrorCode::Io,
        format!("Kan ikke lese {}: {error}", path.display()),
    )
}

fn cannot_write(path: &Path, error: &io::Error) -> Error {
    Error::new(
        ErrorCode::Io,
        format!("Kan ikke skrive til {}: {error}", path.display()),
    )
}
