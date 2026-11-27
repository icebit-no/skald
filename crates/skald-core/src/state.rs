//! What a take changes, kept in the state file apart from the configuration (D-19): the history and how
//! often each correction applied. Skald rewrites the file after every take; the command line reads it when
//! Skald is not running.

use crate::TomlError;
use crate::corrections::Counts;
use crate::history::History;

/// The state file's two parts, each under its own top-level key (`texts` for the history, `applied` for the
/// counts), so either can be read alone.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct State {
    pub history: History,
    pub counts: Counts,
}

impl State {
    /// An empty text is an empty state; keys this version does not know are dropped, since the file is
    /// rewritten after every take.
    pub fn from_toml(text: &str) -> Result<Self, TomlError> {
        Ok(Self {
            history: History::from_toml(text)?,
            counts: Counts::from_toml(text)?,
        })
    }

    pub fn to_toml(&self) -> Result<String, TomlError> {
        let mut table: toml::Table =
            toml::from_str(&self.history.to_toml()?).map_err(TomlError::new)?;
        let counts: toml::Table =
            toml::from_str(&self.counts.to_toml()?).map_err(TomlError::new)?;
        table.extend(counts);
        toml::to_string(&table).map_err(TomlError::new)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::corrections::{Correction, Corrections};
    use crate::history::Entry;

    #[test]
    fn an_empty_file_is_an_empty_state() {
        assert_eq!(State::from_toml("").unwrap(), State::default());
    }

    #[test]
    fn history_and_counts_round_trip_through_one_file() {
        let corrections = Corrections {
            rules: vec![Correction::new("mercha", "merget")],
        };
        let mut state = State::default();
        let text = corrections.apply_and_count("Vi mercha den.", &mut state.counts);
        assert_eq!(text, "Vi merget den.");
        state.history.push(Entry {
            text,
            unix_ms: 1_795_800_000_000,
            microphone: "Innebygd mikrofon".to_owned(),
        });
        assert_eq!(state.counts.get("mercha"), 1);
        let written = state.to_toml().unwrap();
        assert_eq!(State::from_toml(&written).unwrap(), state);
        assert_eq!(History::from_toml(&written).unwrap(), state.history);
        assert_eq!(Counts::from_toml(&written).unwrap(), state.counts);
    }
}
