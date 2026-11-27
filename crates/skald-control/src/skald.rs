//! What the commands need from Skald, as two traits: [`Skald`], which the running app implements and the
//! socket server calls, and [`Machine`], which the binary implements for the command line to use while
//! Skald is not running.

use std::path::Path;

use skald_core::Config;
use skald_core::corrections::Counts;
use skald_core::history::History;
use skald_core::silence_gate::Reason;
use skald_core::vocabulary::Prompt;

use crate::protocol::{Error, Tokens};

/// Skald as the commands see it: its configuration, what its takes left behind, its microphones and
/// models, and its engine. The running app implements it; while Skald is not running, the command line
/// implements it itself from the files and a [`Machine`]. Every command is carried out on top of it, so a
/// command does the same whether Skald runs or not.
pub trait Skald {
    fn config(&self) -> Result<Config, Error>;

    /// Applies `change` to the configuration as it is now and keeps the result, so two changes never
    /// undo each other. When `change` fails, nothing changes. The file is written only when the
    /// configuration changed (D-19), and a running Skald applies the change at once.
    fn change_config(
        &self,
        change: &mut dyn FnMut(&mut Config) -> Result<(), Error>,
    ) -> Result<(), Error>;

    fn history(&self) -> Result<History, Error>;

    fn correction_counts(&self) -> Result<Counts, Error>;

    /// The tokens the prompt takes for the model in use, for the ordskatt meter; `None` without a loaded
    /// model to count with.
    fn prompt_tokens(&self, prompt: &Prompt) -> Option<Tokens>;

    fn microphones(&self) -> Result<Microphones, Error>;

    /// The models Skald offers, in the order it shows them.
    fn models(&self) -> Result<Vec<Model>, Error>;

    /// Transcribes a WAV file as a take, through the silence gate, with the configuration's language,
    /// ordskatt and known phrases. Corrections are not applied: the command applies them.
    fn transcribe(&self, wav: &Path) -> Result<Transcribed, Error>;
}

/// What the command line asks of this machine itself while Skald is not running. The binary implements
/// it with the same crates the app is built from. It writes nothing to standard output, which `skald mcp`
/// speaks its protocol on; anything worth saying goes to standard error.
pub trait Machine {
    /// The microphones connected now, and the one a take would use with the configuration's order and pin.
    fn microphones(&self, config: &Config) -> Result<Microphones, Error>;

    /// The models Skald offers, and which of them are downloaded.
    fn models(&self) -> Result<Vec<Model>, Error>;

    /// Loads the configuration's model in this process and transcribes a WAV file, as
    /// [`Skald::transcribe`] does.
    fn transcribe(&self, config: &Config, wav: &Path) -> Result<Transcribed, Error>;
}

/// Microphones by the names the system gives them, the form the configuration keeps them in.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Microphones {
    pub connected: Vec<String>,
    /// The microphone the next take uses; `None` when none is connected.
    pub active: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Model {
    /// The id the configuration keeps.
    pub id: String,
    /// The name the user sees, such as «NB-Whisper Large».
    pub name: String,
    pub downloaded: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Transcribed {
    /// The model's text, before corrections.
    Text(String),
    HeardNothing(Reason),
}
