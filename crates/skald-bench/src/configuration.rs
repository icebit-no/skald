//! What a take goes through in one configuration: the model, how it decodes, whether the silence around
//! the speech is cut, and the filters before the model.
//!
//! A configuration is written as words separated by commas, each setting one thing; what is not named
//! keeps what Skald does today, NB-Whisper Large decoded greedily with the silence cut and no filter
//! (`nb-whisper-large,greedy,trim`):
//!
//! | Setting | Words |
//! |---|---|
//! | Model | `nb-whisper-large`, `bokfink-bf16`, `bokfink-q8_0` |
//! | Decoding | `greedy`, `beam5` |
//! | Silence before and after the speech | `trim` (cut), `no-trim` (kept) |
//! | 80 Hz high-pass | `highpass`, `no-highpass` |
//! | RNNoise | `rnnoise`, `no-rnnoise` |
//! | Level normalisation | `normalise`, `no-normalise` |
//!
//! The filters run in that order: the high-pass takes the rumble away before RNNoise judges what is noise,
//! and normalisation sets the level of what is left.

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Model {
    NbWhisperLarge,
    BokfinkBf16,
    BokfinkQ8,
}

impl Model {
    pub const ALL: [Model; 3] = [Model::NbWhisperLarge, Model::BokfinkBf16, Model::BokfinkQ8];

    pub fn id(self) -> &'static str {
        match self {
            Self::NbWhisperLarge => "nb-whisper-large",
            Self::BokfinkBf16 => "bokfink-bf16",
            Self::BokfinkQ8 => "bokfink-q8_0",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|model| model.id() == id)
    }

    /// Why the bench cannot run the model, if it cannot, in bokmål.
    pub fn unavailable(self) -> Option<&'static str> {
        match self {
            Self::NbWhisperLarge => None,
            Self::BokfinkBf16 | Self::BokfinkQ8 => Some("motoren laster bare Whisper-modeller"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Decoding {
    Greedy,
    /// Beam search with five beams (D-16).
    Beam5,
}

impl Decoding {
    pub fn id(self) -> &'static str {
        match self {
            Self::Greedy => "greedy",
            Self::Beam5 => "beam5",
        }
    }

    pub fn unavailable(self) -> Option<&'static str> {
        match self {
            Self::Greedy => None,
            Self::Beam5 => Some("motoren dekoder bare grådig"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Configuration {
    pub model: Model,
    pub decoding: Decoding,
    /// The silence before and after the speech cut away, with the engine's margin kept, as Skald does.
    pub trim_silence: bool,
    pub high_pass: bool,
    pub rnnoise: bool,
    pub normalise_level: bool,
}

impl Default for Configuration {
    fn default() -> Self {
        Self {
            model: Model::NbWhisperLarge,
            decoding: Decoding::Greedy,
            trim_silence: true,
            high_pass: false,
            rnnoise: false,
            normalise_level: false,
        }
    }
}

impl Configuration {
    /// Reads a configuration from its words (see the module's table).
    pub fn parse(spec: &str) -> Result<Self, String> {
        let mut configuration = Self::default();
        let mut set = Vec::new();
        for word in spec.split(',').map(str::trim).filter(|w| !w.is_empty()) {
            let c = &mut configuration;
            let setting = match word {
                "greedy" | "beam5" => {
                    c.decoding = if word == "greedy" {
                        Decoding::Greedy
                    } else {
                        Decoding::Beam5
                    };
                    "dekodingen"
                }
                "trim" | "no-trim" => {
                    c.trim_silence = word == "trim";
                    "stillheten rundt talen"
                }
                "highpass" | "no-highpass" => {
                    c.high_pass = word == "highpass";
                    "høypassfilteret"
                }
                "rnnoise" | "no-rnnoise" => {
                    c.rnnoise = word == "rnnoise";
                    "RNNoise"
                }
                "normalise" | "no-normalise" => {
                    c.normalise_level = word == "normalise";
                    "nivånormaliseringen"
                }
                _ => match Model::from_id(word) {
                    Some(model) => {
                        c.model = model;
                        "modellen"
                    }
                    None => return Err(format!("«{word}» i «{spec}» er ingen innstilling.")),
                },
            };
            if set.contains(&setting) {
                return Err(format!("«{spec}» setter {setting} to ganger."));
            }
            set.push(setting);
        }
        Ok(configuration)
    }

    /// The configurations compared when none is named: what Skald does today, then each setting changed
    /// on its own.
    pub fn comparison() -> Vec<Self> {
        let today = Self::default();
        vec![
            today,
            Self {
                trim_silence: false,
                ..today
            },
            Self {
                high_pass: true,
                ..today
            },
            Self {
                rnnoise: true,
                ..today
            },
            Self {
                normalise_level: true,
                ..today
            },
            Self {
                decoding: Decoding::Beam5,
                ..today
            },
            Self {
                model: Model::BokfinkBf16,
                ..today
            },
            Self {
                model: Model::BokfinkQ8,
                ..today
            },
        ]
    }

    /// Why the bench cannot run this configuration, if it cannot, in bokmål.
    pub fn unavailable(&self) -> Option<&'static str> {
        self.model.unavailable().or(self.decoding.unavailable())
    }
}

/// The configuration's words: model, decoding and trim always, then the filters that are on, in the order
/// they run. [`Configuration::parse`] reads it back.
impl fmt::Display for Configuration {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{},{}", self.model.id(), self.decoding.id())?;
        f.write_str(if self.trim_silence {
            ",trim"
        } else {
            ",no-trim"
        })?;
        for (on, word) in [
            (self.high_pass, ",highpass"),
            (self.rnnoise, ",rnnoise"),
            (self.normalise_level, ",normalise"),
        ] {
            if on {
                f.write_str(word)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_named_is_what_skald_does_today() {
        let today = Configuration::parse("").unwrap();
        assert_eq!(today, Configuration::default());
        assert_eq!(today.to_string(), "nb-whisper-large,greedy,trim");
        assert_eq!(today.unavailable(), None);
    }

    #[test]
    fn words_change_one_setting_each() {
        let configuration = Configuration::parse("no-trim, highpass,rnnoise,normalise").unwrap();
        assert_eq!(
            configuration,
            Configuration {
                trim_silence: false,
                high_pass: true,
                rnnoise: true,
                normalise_level: true,
                ..Configuration::default()
            }
        );
        assert_eq!(
            configuration.to_string(),
            "nb-whisper-large,greedy,no-trim,highpass,rnnoise,normalise"
        );
    }

    #[test]
    fn the_name_reads_back_as_the_same_configuration() {
        for configuration in Configuration::comparison() {
            let name = configuration.to_string();
            assert_eq!(
                Configuration::parse(&name).unwrap(),
                configuration,
                "{name}"
            );
        }
    }

    #[test]
    fn unknown_and_repeated_words_are_errors() {
        assert!(
            Configuration::parse("highpas")
                .unwrap_err()
                .contains("«highpas» i «highpas» er ingen innstilling.")
        );
        assert!(
            Configuration::parse("bokfink-bf16,nb-whisper-large")
                .unwrap_err()
                .contains("setter modellen to ganger.")
        );
        assert!(Configuration::parse("trim,no-trim").is_err());
    }

    #[test]
    fn bokfink_and_beam_search_are_named_but_not_available() {
        let bokfink = Configuration::parse("bokfink-q8_0").unwrap();
        assert_eq!(bokfink.model, Model::BokfinkQ8);
        assert_eq!(
            bokfink.unavailable(),
            Some("motoren laster bare Whisper-modeller")
        );
        let beam = Configuration::parse("beam5").unwrap();
        assert_eq!(beam.unavailable(), Some("motoren dekoder bare grådig"));
    }

    #[test]
    fn the_comparison_changes_one_setting_at_a_time() {
        let comparison = Configuration::comparison();
        let today = comparison[0];
        assert_eq!(today, Configuration::default());
        for configuration in &comparison[1..] {
            let changed = [
                configuration.model != today.model,
                configuration.decoding != today.decoding,
                configuration.trim_silence != today.trim_silence,
                configuration.high_pass != today.high_pass,
                configuration.rnnoise != today.rnnoise,
                configuration.normalise_level != today.normalise_level,
            ];
            assert_eq!(changed.iter().filter(|&&c| c).count(), 1, "{configuration}");
        }
    }
}
