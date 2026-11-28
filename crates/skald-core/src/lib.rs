//! Configuration, vocabulary, corrections and history: the rules of a take, with no I/O, so every one of
//! them is tested without a microphone, a model or a window. Callers read and write the files, read the
//! clock and measure the audio, and pass the text, timestamps and durations in.

use std::fmt;

pub mod config;
pub mod corrections;
pub mod history;
pub mod hold_key;
pub mod paths;
pub mod silence;
pub mod silence_gate;
pub mod state;
mod text;
pub mod vocabulary;

pub use config::Config;

/// A TOML document that could not be read or written, with the reason and, when reading, the line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TomlError {
    message: String,
}

impl TomlError {
    fn new(error: impl fmt::Display) -> Self {
        Self {
            message: error.to_string(),
        }
    }
}

impl fmt::Display for TomlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for TomlError {}
