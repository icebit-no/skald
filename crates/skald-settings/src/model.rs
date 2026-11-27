//! What the app hands the settings window: everything the four tabs show.

use std::collections::BTreeMap;
use std::fmt;
use std::rc::Rc;

use skald_core::Config;
use skald_core::state::State;
use skald_models::Status;

/// Everything the window shows. The app builds it from its own state and hands it over whenever any part
/// changes; the window keeps no setting of its own.
#[derive(Clone, Default)]
pub struct SettingsModel {
    pub config: Config,
    /// The history and how often each correction applied.
    pub state: State,
    pub microphones: Microphones,
    /// What of each model is on this machine, by the model's id. A model that can be downloaded and is
    /// missing here shows as not downloaded.
    pub models: BTreeMap<String, ModelState>,
    /// The ordskatt meter's counter; `None` while no model is loaded to count with.
    pub tokens: Option<TokenCounter>,
    /// Skald's version, shown in Om.
    pub version: String,
}

/// Microphones by the names the system gives them, the form the configuration keeps them in.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Microphones {
    pub connected: Vec<String>,
    /// The microphone a take uses now; `None` when none is connected.
    pub in_use: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelState {
    /// What the models folder holds.
    OnDisk(Status),
    /// A download is running. Once every byte has arrived the file is checked against its SHA-256.
    Downloading { received: u64 },
}

/// Counts the tokens a prompt takes for the model in use, and the most the model keeps.
#[derive(Clone)]
pub struct TokenCounter {
    pub limit: usize,
    count: Rc<dyn Fn(&str) -> usize>,
}

impl TokenCounter {
    pub fn new(limit: usize, count: impl Fn(&str) -> usize + 'static) -> Self {
        Self {
            limit,
            count: Rc::new(count),
        }
    }

    pub fn count(&self, text: &str) -> usize {
        (self.count)(text)
    }
}

impl fmt::Debug for TokenCounter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TokenCounter")
            .field("limit", &self.limit)
            .finish_non_exhaustive()
    }
}
