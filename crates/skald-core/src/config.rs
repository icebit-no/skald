//! The configuration: everything the settings window and `skald` can change, as one TOML document. Both
//! read it with [`Config::from_toml`] and write it with [`Config::to_toml`], so they change the same file
//! under the same rules.
//!
//! The file changes only when a setting does, never after a take: the command line and a person's editor
//! share it, and a write per take would race both. What a take changes, the history and how often each
//! correction applied, is kept apart from it.
//!
//! # Older and newer files
//!
//! - **A missing key takes its default**, at every level, so a file written by an older Skald loads.
//! - **A key this version does not know is kept**: it is read, has no effect, and is written back as it
//!   was, at the top, in a section and in each correction, so an older Skald that saves its settings does
//!   not delete those of a newer one. Comments are not kept.
//! - **A hold key or model this version does not know is kept as written**, for the same reason:
//!   [`HoldKey::Unknown`], and the model's id as a string.
//! - **A key keeps its type for good.** A setting that needs another type gets a new key, so a value of the
//!   wrong type is a mistake in the file: loading fails and says which key, rather than putting a default
//!   in place of what the user chose.
//!
//! An empty text is the default configuration.

use serde::{Deserialize, Serialize};

use crate::TomlError;
use crate::corrections::Corrections;
use crate::silence::Silence;
use crate::vocabulary::Vocabulary;

/// The id of the model a new configuration uses.
pub const DEFAULT_MODEL: &str = "nb-whisper-large";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub hold_key: HoldKey,
    pub language: Language,
    /// Microphone names in the user's order; the first one connected is used.
    pub microphones: Vec<String>,
    /// The microphone chosen in the menu or with `skald mic use`: it goes ahead of the order while it is
    /// connected, until «Bruk den første som er koblet til» clears it (D-20). Absent from the file when unset.
    pub pinned_microphone: Option<String>,
    /// The speech model's id. Any id is accepted, so a model only a newer Skald offers survives a save.
    pub model: String,
    /// «Kopier også teksten til utklippstavlen»: leave the inserted text on the clipboard instead of
    /// putting the previous clipboard back (D-9).
    pub copy_to_clipboard: bool,
    pub start_at_login: bool,
    /// The start and end sounds of a take.
    pub sounds: bool,
    /// «Overleggets utseende» (D-24).
    pub overlay_theme: OverlayTheme,
    pub vocabulary: Vocabulary,
    pub corrections: Corrections,
    pub silence: Silence,
    #[serde(flatten)]
    unknown: toml::Table,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            hold_key: HoldKey::default(),
            language: Language::default(),
            microphones: Vec::new(),
            pinned_microphone: None,
            model: DEFAULT_MODEL.to_owned(),
            copy_to_clipboard: false,
            start_at_login: false,
            sounds: true,
            overlay_theme: OverlayTheme::default(),
            vocabulary: Vocabulary::default(),
            corrections: Corrections::default(),
            silence: Silence::default(),
            unknown: toml::Table::new(),
        }
    }
}

impl Config {
    pub fn from_toml(text: &str) -> Result<Self, TomlError> {
        toml::from_str(text).map_err(TomlError::new)
    }

    /// Writes every setting, the defaults included, so the file shows what can be set.
    pub fn to_toml(&self) -> Result<String, TomlError> {
        toml::to_string(self).map_err(TomlError::new)
    }
}

/// The key held to talk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "String", into = "String")]
pub enum HoldKey {
    /// `right-command`
    RightCommand,
    /// `right-control`
    RightControl,
    /// A key this version does not know, written by a newer Skald, kept as written.
    Unknown(String),
}

impl Default for HoldKey {
    /// Right ⌘ on macOS; right Ctrl on Windows and Linux, where right Alt is AltGr.
    fn default() -> Self {
        if cfg!(target_os = "macos") {
            Self::RightCommand
        } else {
            Self::RightControl
        }
    }
}

impl From<String> for HoldKey {
    fn from(name: String) -> Self {
        match name.as_str() {
            "right-command" => Self::RightCommand,
            "right-control" => Self::RightControl,
            _ => Self::Unknown(name),
        }
    }
}

impl From<HoldKey> for String {
    fn from(key: HoldKey) -> Self {
        match key {
            HoldKey::RightCommand => "right-command".to_owned(),
            HoldKey::RightControl => "right-control".to_owned(),
            HoldKey::Unknown(name) => name,
        }
    }
}

/// How the overlay looks; the states and their texts are the same in every theme (D-24).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(from = "String", into = "String")]
pub enum OverlayTheme {
    /// `runepinne`
    #[default]
    Runepinne,
    /// `smidd-jern`
    SmiddJern,
    /// `kjedelig`: looks like the rest of the system.
    Kjedelig,
    /// A theme this version does not know, written by a newer Skald, kept as written; drawn as the default.
    Unknown(String),
}

impl From<String> for OverlayTheme {
    fn from(name: String) -> Self {
        match name.as_str() {
            "runepinne" => Self::Runepinne,
            "smidd-jern" => Self::SmiddJern,
            "kjedelig" => Self::Kjedelig,
            _ => Self::Unknown(name),
        }
    }
}

impl From<OverlayTheme> for String {
    fn from(theme: OverlayTheme) -> Self {
        match theme {
            OverlayTheme::Runepinne => "runepinne".to_owned(),
            OverlayTheme::SmiddJern => "smidd-jern".to_owned(),
            OverlayTheme::Kjedelig => "kjedelig".to_owned(),
            OverlayTheme::Unknown(name) => name,
        }
    }
}

/// The written standard of the text: Norwegian only (D-6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    /// `bokmal`; `bokmål` is read as well.
    #[default]
    #[serde(alias = "bokmål")]
    Bokmal,
    Nynorsk,
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn the_pin_and_the_theme_round_trip_and_an_unset_pin_is_left_out() {
        let config = Config {
            pinned_microphone: Some("HyperX QuadCast".to_owned()),
            overlay_theme: OverlayTheme::SmiddJern,
            ..Config::default()
        };
        let text = config.to_toml().unwrap();
        assert!(text.contains("pinned_microphone = \"HyperX QuadCast\""));
        assert!(text.contains("overlay_theme = \"smidd-jern\""));
        assert_eq!(Config::from_toml(&text).unwrap(), config);
        assert!(
            !Config::default()
                .to_toml()
                .unwrap()
                .contains("pinned_microphone")
        );
    }

    #[test]
    fn a_theme_from_a_newer_skald_is_kept_as_written() {
        let config = Config::from_toml("overlay_theme = \"nordlys\"").unwrap();
        assert_eq!(
            config.overlay_theme,
            OverlayTheme::Unknown("nordlys".to_owned())
        );
        assert!(
            config
                .to_toml()
                .unwrap()
                .contains("overlay_theme = \"nordlys\"")
        );
    }

    #[test]
    fn an_empty_file_is_the_default() {
        assert_eq!(Config::from_toml("").unwrap(), Config::default());
    }

    #[test]
    fn the_default_has_no_words_of_anyone() {
        let config = Config::default();
        assert!(config.vocabulary.sentence.is_empty());
        assert!(config.vocabulary.words.is_empty());
        assert!(config.corrections.rules.is_empty());
        assert!(config.microphones.is_empty());
        assert!(!config.copy_to_clipboard);
        assert!(config.sounds);
        assert_eq!(config.language, Language::Bokmal);
        assert_eq!(config.model, DEFAULT_MODEL);
    }

    #[test]
    fn the_default_hold_key_follows_the_platform() {
        let expected = if cfg!(target_os = "macos") {
            HoldKey::RightCommand
        } else {
            HoldKey::RightControl
        };
        assert_eq!(HoldKey::default(), expected);
    }

    #[test]
    fn round_trips_every_setting() {
        let mut config = Config {
            hold_key: HoldKey::RightControl,
            language: Language::Nynorsk,
            microphones: vec!["Shure MV7".into(), "MacBook Pro-mikrofon".into()],
            model: "nb-asr-bokfink".into(),
            copy_to_clipboard: true,
            start_at_login: true,
            sounds: false,
            ..Config::default()
        };
        config.vocabulary.sentence = "Vi merget pull-requesten i går.".into();
        config.vocabulary.add_word("PostgreSQL");
        config
            .corrections
            .add("Post gres SQL", "PostgreSQL")
            .unwrap();
        config.corrections.add("mercha", "merget").unwrap();
        config.silence.known_phrases.push("Å ja".into());

        let toml = config.to_toml().unwrap();
        assert_eq!(Config::from_toml(&toml).unwrap(), config, "{toml}");
    }

    #[test]
    fn writes_a_readable_file() {
        let mut config = Config {
            hold_key: HoldKey::RightCommand,
            ..Config::default()
        };
        config.vocabulary.sentence = "Vi merget pull-requesten.".into();
        config.vocabulary.add_word("PostgreSQL");
        config.corrections.add("mercha", "merget").unwrap();
        let toml = config.to_toml().unwrap();
        for line in [
            r#"hold_key = "right-command""#,
            r#"language = "bokmal""#,
            r#"model = "nb-whisper-large""#,
            "copy_to_clipboard = false",
            "[vocabulary]",
            r#"sentence = "Vi merget pull-requesten.""#,
            "[[corrections]]",
            r#"from = "mercha""#,
            "[silence]",
            "known_phrase_max_speech_ms = 350",
        ] {
            assert!(
                toml.lines().any(|l| l == line),
                "{line:?} missing from:\n{toml}"
            );
        }
    }

    #[test]
    fn an_old_file_missing_keys_loads_with_defaults() {
        let config = Config::from_toml(
            r#"
            language = "nynorsk"

            [vocabulary]
            words = ["PostgreSQL"]

            [[corrections]]
            from = "mercha"
            to = "merget"
            "#,
        )
        .unwrap();
        assert_eq!(config.language, Language::Nynorsk);
        assert_eq!(config.hold_key, HoldKey::default());
        assert_eq!(config.model, DEFAULT_MODEL);
        assert!(config.sounds);
        assert_eq!(config.vocabulary.sentence, "");
        assert_eq!(config.vocabulary.words, ["PostgreSQL"]);
        assert_eq!(config.corrections.rules[0].to, "merget");
        assert_eq!(config.silence, Silence::default());
    }

    #[test]
    fn a_new_file_with_unknown_keys_loads_and_keeps_them() {
        let text = r#"
            language = "nynorsk"
            future_flag = true
            future_choice = "kanskje"

            [vocabulary]
            words = ["PostgreSQL"]
            weights = [1, 2]

            [[corrections]]
            from = "mercha"
            to = "merget"
            enabled = false

            [silence]
            no_speech_threshold = 0.6

            [overlay]
            position = "top"
            since = 2027-01-01T08:00:00Z
            nested = { a = 1, b = ["x", "ø"] }
        "#;
        let config = Config::from_toml(text).unwrap();
        assert_eq!(config.language, Language::Nynorsk);
        assert_eq!(
            config.corrections.apply("mercha").text,
            "merget",
            "an unknown key changes nothing here"
        );

        let written = config.to_toml().unwrap();
        let reread = Config::from_toml(&written).unwrap();
        assert_eq!(reread, config, "{written}");

        let table: toml::Table = toml::from_str(&written).unwrap();
        assert_eq!(table["future_flag"].as_bool(), Some(true));
        assert_eq!(table["future_choice"].as_str(), Some("kanskje"));
        assert_eq!(table["vocabulary"]["weights"].as_array().unwrap().len(), 2);
        assert_eq!(table["corrections"][0]["enabled"].as_bool(), Some(false));
        assert_eq!(
            table["silence"]["no_speech_threshold"].as_float(),
            Some(0.6)
        );
        assert_eq!(table["overlay"]["position"].as_str(), Some("top"));
        assert!(table["overlay"]["since"].is_datetime());
        assert_eq!(table["overlay"]["nested"]["b"][1].as_str(), Some("ø"));
    }

    #[test]
    fn an_unknown_hold_key_is_kept_as_written() {
        let config = Config::from_toml(r#"hold_key = "right-option""#).unwrap();
        assert_eq!(config.hold_key, HoldKey::Unknown("right-option".into()));
        assert!(
            config
                .to_toml()
                .unwrap()
                .contains(r#"hold_key = "right-option""#)
        );
    }

    #[test]
    fn reads_bokmål_with_its_å() {
        let config = Config::from_toml(r#"language = "bokmål""#).unwrap();
        assert_eq!(config.language, Language::Bokmal);
    }

    #[test]
    fn a_value_of_the_wrong_type_fails_and_names_the_key() {
        let error = Config::from_toml("sounds = \"ja\"")
            .unwrap_err()
            .to_string();
        assert!(error.contains("sounds"), "{error}");

        let error = Config::from_toml(r#"language = "svensk""#)
            .unwrap_err()
            .to_string();
        assert!(error.contains("svensk"), "{error}");
    }

    #[test]
    fn a_correction_needs_from_and_to() {
        assert!(Config::from_toml("[[corrections]]\nfrom = \"mercha\"\n").is_err());
    }

    #[test]
    fn the_sections_work_once_loaded() {
        let config = Config::from_toml(
            r#"
            [vocabulary]
            sentence = "Vi merget pull-requesten."
            words = ["Skald", "PostgreSQL"]

            [[corrections]]
            from = "Post gres SQL"
            to = "PostgreSQL"
            "#,
        )
        .unwrap();
        assert_eq!(
            config.vocabulary.prompt().text(),
            "Vi merget pull-requesten. Skald, PostgreSQL."
        );
        assert_eq!(
            config.corrections.apply("Post gres SQL kjører.").text,
            "PostgreSQL kjører."
        );
        assert!(
            config
                .silence
                .is_known_phrase("Den satt.", Duration::from_millis(300))
        );
    }
}
