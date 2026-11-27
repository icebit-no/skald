//! The five latest texts, newest first, kept on this machine only (D-9). The caller reads the clock and
//! keeps the file; this module holds the list and its TOML form.

use serde::{Deserialize, Serialize};

use crate::TomlError;

/// Five, with no setting for the number (D-9).
pub const CAPACITY: usize = 5;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct History {
    /// Newest first.
    #[serde(default)]
    texts: Vec<Entry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    /// The text as it was inserted, after corrections, or as the user corrected it afterwards.
    pub text: String,
    /// When the take ended, in milliseconds since 1970-01-01 00:00 UTC.
    #[serde(default)]
    pub unix_ms: i64,
    /// The name of the microphone the take was recorded with.
    #[serde(default)]
    pub microphone: String,
}

impl History {
    /// Puts the entry first and lets the oldest go beyond five.
    pub fn push(&mut self, entry: Entry) {
        self.texts.insert(0, entry);
        self.texts.truncate(CAPACITY);
    }

    /// Newest first.
    pub fn entries(&self) -> &[Entry] {
        &self.texts
    }

    pub fn latest(&self) -> Option<&Entry> {
        self.texts.first()
    }

    /// Gives the entry that ended at `unix_ms` with the text `before` the text `after`, as when a word in
    /// it is corrected. The entry is found by both, so a take that arrived meanwhile is never the one
    /// changed. Returns whether there was such an entry.
    pub fn replace(&mut self, unix_ms: i64, before: &str, after: &str) -> bool {
        match self
            .texts
            .iter_mut()
            .find(|entry| entry.unix_ms == unix_ms && entry.text == before)
        {
            Some(entry) => {
                entry.text = after.to_owned();
                true
            }
            None => false,
        }
    }

    /// Reads a history in the order it was written, keeping the first five. An empty text is an empty
    /// history; keys this version does not know are ignored, since the history is rewritten after every
    /// take.
    pub fn from_toml(text: &str) -> Result<Self, TomlError> {
        let mut history: Self = toml::from_str(text).map_err(TomlError::new)?;
        history.texts.truncate(CAPACITY);
        Ok(history)
    }

    pub fn to_toml(&self) -> Result<String, TomlError> {
        toml::to_string(self).map_err(TomlError::new)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(text: &str, unix_ms: i64) -> Entry {
        Entry {
            text: text.to_owned(),
            unix_ms,
            microphone: "Innebygd mikrofon".to_owned(),
        }
    }

    #[test]
    fn keeps_the_five_latest_newest_first() {
        let mut history = History::default();
        assert!(history.latest().is_none());
        for n in 1..=7 {
            history.push(entry(&format!("Tekst {n}"), n));
        }
        let texts: Vec<&str> = history.entries().iter().map(|e| e.text.as_str()).collect();
        assert_eq!(
            texts,
            ["Tekst 7", "Tekst 6", "Tekst 5", "Tekst 4", "Tekst 3"]
        );
        assert_eq!(history.latest().unwrap().unix_ms, 7);
    }

    #[test]
    fn replaces_the_text_of_the_entry_named_by_its_time_and_text() {
        let mut history = History::default();
        history.push(entry("Vi mercha den.", 1));
        history.push(entry("Vi mercha den.", 2));
        assert!(history.replace(1, "Vi mercha den.", "Vi merget den."));
        let texts: Vec<&str> = history.entries().iter().map(|e| e.text.as_str()).collect();
        assert_eq!(texts, ["Vi mercha den.", "Vi merget den."]);
        assert_eq!(history.entries()[1].unix_ms, 1, "the entry keeps its place");

        assert!(
            !history.replace(1, "Vi mercha den.", "Noe annet."),
            "the text has changed since"
        );
        assert!(!history.replace(3, "Vi mercha den.", "Noe annet."));
        assert_eq!(history.entries()[1].text, "Vi merget den.");
    }

    #[test]
    fn round_trips_through_toml_with_æøå() {
        let mut history = History::default();
        history.push(Entry {
            text: "Vi merget pull-requesten.".to_owned(),
            unix_ms: 1_764_250_000_000,
            microphone: "Shure MV7".to_owned(),
        });
        history.push(Entry {
            text: "Hei, dette er en prøve med «æøå» og \"sitat\"\npå to linjer.".to_owned(),
            unix_ms: 1_764_250_060_000,
            microphone: "MacBook Pro-mikrofon".to_owned(),
        });
        let toml = history.to_toml().unwrap();
        assert_eq!(History::from_toml(&toml).unwrap(), history);
    }

    #[test]
    fn reads_an_empty_file_as_an_empty_history() {
        assert_eq!(History::from_toml("").unwrap(), History::default());
    }

    #[test]
    fn reads_entries_missing_a_time_or_microphone() {
        let history = History::from_toml(
            r#"
            [[texts]]
            text = "Hei, dette er en prøve"
            "#,
        )
        .unwrap();
        let latest = history.latest().unwrap();
        assert_eq!(latest.text, "Hei, dette er en prøve");
        assert_eq!(latest.unix_ms, 0);
        assert_eq!(latest.microphone, "");
    }

    #[test]
    fn reads_only_the_first_five_of_a_longer_file() {
        let file: String = (1..=7)
            .map(|n| format!("[[texts]]\ntext = \"Tekst {n}\"\nunix_ms = {n}\nfavoritt = true\n"))
            .collect();
        let history = History::from_toml(&file).unwrap();
        assert_eq!(history.entries().len(), CAPACITY);
        assert_eq!(history.latest().unwrap().text, "Tekst 1");
    }

    #[test]
    fn a_broken_file_says_where() {
        let error = History::from_toml("[[texts]]\ntext = \"uten slutt\n").unwrap_err();
        assert!(error.to_string().contains("line 2"), "{error}");
    }
}
