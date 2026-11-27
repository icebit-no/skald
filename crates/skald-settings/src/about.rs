//! Om: the version, the models and the engine with their licences and who made them, the source and
//! «Ingenting sendes fra maskinen» (docs/design.md, Settings).

use skald_models::MODELS;

use crate::model::SettingsModel;

pub const SOURCE: &str = "https://github.com/icebit-no/skald";
pub const NOTHING_LEAVES: &str = "Ingenting sendes fra maskinen";
/// What the network is used for, the only two things (CLAUDE.md).
pub const NETWORK: &str =
    "Nettet brukes bare til å laste ned en talemodell og til å se etter en ny versjon.";
pub const LICENCE: &str = "Fri programvare med åpen kildekode, under MIT-lisensen.";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AboutTab {
    /// «Skald 0.1.0»
    pub title: String,
    pub models: Vec<Credit>,
    pub engine: Vec<Credit>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Credit {
    pub name: &'static str,
    /// SPDX identifier.
    pub licence: &'static str,
    pub credit: &'static str,
    pub link: &'static str,
}

/// What runs the models, beside them in the app. Silero VAD ships inside the app (docs/design.md, First
/// run); transcribe.cpp carries ggml.
const ENGINE: &[Credit] = &[
    Credit {
        name: "transcribe.cpp",
        licence: "MIT",
        credit: "Kjører talemodellen på maskinen. Laget av handy-computer.",
        link: "https://github.com/handy-computer/transcribe.cpp",
    },
    Credit {
        name: "ggml",
        licence: "MIT",
        credit: "Regnebiblioteket under transcribe.cpp. Laget av Georgi Gerganov og ggml-prosjektet.",
        link: "https://github.com/ggml-org/ggml",
    },
    Credit {
        name: "Silero VAD",
        licence: "MIT",
        credit: "Finner talen i opptaket. Laget av Silero-teamet.",
        link: "https://github.com/snakers4/silero-vad",
    },
];

impl AboutTab {
    pub fn of(model: &SettingsModel) -> Self {
        Self {
            title: format!("Skald {}", model.version),
            models: MODELS
                .iter()
                .map(|entry| Credit {
                    name: entry.name,
                    licence: entry.licence,
                    credit: entry.credit,
                    link: entry.card,
                })
                .collect(),
            engine: ENGINE.to_vec(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_the_version_and_credits_the_national_library_for_its_models() {
        let tab = AboutTab::of(&SettingsModel {
            version: "0.1.0".into(),
            ..SettingsModel::default()
        });
        assert_eq!(tab.title, "Skald 0.1.0");
        assert_eq!(tab.models.len(), MODELS.len());
        for credit in &tab.models {
            assert!(credit.credit.contains("Nasjonalbiblioteket"), "{credit:?}");
            assert_eq!(credit.licence, "Apache-2.0");
            assert!(credit.link.starts_with("https://huggingface.co/NbAiLab/"));
        }
        let engine: Vec<&str> = tab.engine.iter().map(|credit| credit.name).collect();
        assert_eq!(engine, ["transcribe.cpp", "ggml", "Silero VAD"]);
        assert!(tab.engine.iter().all(|credit| credit.licence == "MIT"));
    }
}
