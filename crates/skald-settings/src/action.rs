//! What the window asks of the app. A change to the configuration is a [`Change`], which the app applies
//! with [`Change::apply`] and then writes to the configuration file; the rest is work only the app can do.

use skald_core::Config;
use skald_core::config::{HoldKey, Language, OverlayTheme};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Change(Change),
    /// Fetch the model with this id, or go on from where its download stopped; the download checks a file
    /// already there before fetching anything.
    DownloadModel(String),
    StopDownload(String),
    /// A word corrected in a history entry. The entry is the one that ended at `unix_ms` with the text
    /// `before`, so a take that arrives meanwhile is never the one changed.
    CorrectHistory {
        unix_ms: i64,
        before: String,
        after: String,
    },
}

/// One change to the configuration. Each has a command in `skald` that makes the same change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    HoldKey(HoldKey),
    Language(Language),
    /// Microphone names in the order a take tries them.
    MicrophoneOrder(Vec<String>),
    /// Puts a microphone ahead of the order while it is connected; `None` removes the pin (D-20).
    PinMicrophone(Option<String>),
    /// The speech model's id.
    Model(String),
    OverlayTheme(OverlayTheme),
    CopyToClipboard(bool),
    StartAtLogin(bool),
    Sounds(bool),
    /// The sentence the ordskatt opens with; an empty one removes it.
    Sentence(String),
    /// Added last, where the model weighs it most.
    AddWord(String),
    RemoveWord(String),
    /// Adds a correction, or gives the one for the same words a new replacement.
    AddCorrection {
        from: String,
        to: String,
    },
    RemoveCorrection {
        from: String,
    },
    /// The correction for `before` now replaces `from` with `to`: one step, so the file never holds the
    /// configuration with the rule missing.
    EditCorrection {
        before: String,
        from: String,
        to: String,
    },
}

impl Change {
    /// Applies the change and returns whether the configuration changed, so the file is written only then
    /// (D-19).
    pub fn apply(&self, config: &mut Config) -> bool {
        let before = config.clone();
        match self {
            Self::HoldKey(key) => config.hold_key = key.clone(),
            Self::Language(language) => config.language = *language,
            Self::MicrophoneOrder(names) => config.microphones = names.clone(),
            Self::PinMicrophone(name) => config.pinned_microphone = name.clone(),
            Self::Model(id) => config.model = id.clone(),
            Self::OverlayTheme(theme) => config.overlay_theme = theme.clone(),
            Self::CopyToClipboard(on) => config.copy_to_clipboard = *on,
            Self::StartAtLogin(on) => config.start_at_login = *on,
            Self::Sounds(on) => config.sounds = *on,
            Self::Sentence(sentence) => config.vocabulary.sentence = sentence.trim().to_owned(),
            Self::AddWord(word) => {
                config.vocabulary.add_word(word);
            }
            Self::RemoveWord(word) => {
                config.vocabulary.remove_word(word);
            }
            Self::AddCorrection { from, to } => {
                // Blank words are refused by the tab before they get here.
                let _ = config.corrections.add(from, to);
            }
            Self::RemoveCorrection { from } => {
                config.corrections.remove(from);
            }
            Self::EditCorrection { before, from, to } => {
                let mut corrections = config.corrections.clone();
                corrections.remove(before);
                if corrections.add(from, to).is_ok() {
                    config.corrections = corrections;
                }
            }
        }
        *config != before
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_change_says_whether_it_changed_anything() {
        let mut config = Config::default();
        assert!(Change::Sounds(false).apply(&mut config));
        assert!(!config.sounds);
        assert!(!Change::Sounds(false).apply(&mut config), "already off");
        assert!(Change::Language(Language::Nynorsk).apply(&mut config));
        assert!(!Change::AddWord("  ".into()).apply(&mut config));
    }

    #[test]
    fn the_sentence_is_kept_without_spaces_around_it() {
        let mut config = Config::default();
        assert!(Change::Sentence("  Vi merget pull-requesten. ".into()).apply(&mut config));
        assert_eq!(config.vocabulary.sentence, "Vi merget pull-requesten.");
        assert!(Change::Sentence(String::new()).apply(&mut config));
        assert_eq!(config.vocabulary.sentence, "");
    }

    #[test]
    fn editing_a_correction_keeps_it_in_one_step() {
        let mut config = Config::default();
        Change::AddCorrection {
            from: "mercha".into(),
            to: "merget".into(),
        }
        .apply(&mut config);
        assert!(
            Change::EditCorrection {
                before: "mercha".into(),
                from: "merga".into(),
                to: "merget".into(),
            }
            .apply(&mut config)
        );
        assert_eq!(config.corrections.rules.len(), 1);
        assert_eq!(config.corrections.rules[0].from, "merga");

        assert!(
            !Change::EditCorrection {
                before: "merga".into(),
                from: " ".into(),
                to: "merget".into(),
            }
            .apply(&mut config),
            "a rule with nothing to replace is refused and the old one stays"
        );
        assert_eq!(config.corrections.rules[0].from, "merga");
    }

    #[test]
    fn the_pin_is_set_and_removed() {
        let mut config = Config::default();
        assert!(Change::PinMicrophone(Some("Shure MV7".into())).apply(&mut config));
        assert_eq!(config.pinned_microphone.as_deref(), Some("Shure MV7"));
        assert!(Change::PinMicrophone(None).apply(&mut config));
        assert_eq!(config.pinned_microphone, None);
    }
}
