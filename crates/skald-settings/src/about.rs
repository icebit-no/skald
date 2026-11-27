//! Om: the version, the models and the engine with their licences and who made them, the source and
//! «Ingenting sendes fra maskinen» (docs/design.md, Settings).

use skald_models::MODELS;

use crate::model::{Compute, SettingsModel};

pub const SOURCE: &str = "https://github.com/icebit-no/skald";
pub const NOTHING_LEAVES: &str = "Ingenting sendes fra maskinen";
/// What the network is used for, the only two things (CLAUDE.md).
pub const NETWORK: &str =
    "Nettet brukes bare til å laste ned en talemodell og til å se etter en ny versjon.";
pub const LICENCE: &str = "Fri programvare med åpen kildekode, under MIT-lisensen.";
pub const ON_METAL: &str = "Talemodellen regner på skjermkortet med Metal.";
pub const ON_VULKAN: &str = "Talemodellen regner på skjermkortet med Vulkan.";
pub const ON_CPU: &str = "Talemodellen regner på prosessoren.";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AboutTab {
    /// «Skald 0.1.0»
    pub title: String,
    pub models: Vec<Credit>,
    pub engine: Vec<Credit>,
    /// What the model computes on, once one has loaded.
    pub compute: Option<&'static str>,
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
            compute: model.compute.map(|compute| match compute {
                Compute::Metal => ON_METAL,
                Compute::Vulkan => ON_VULKAN,
                Compute::Cpu => ON_CPU,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn says_what_the_model_computes_on_once_one_has_loaded() {
        let on = |compute| {
            AboutTab::of(&SettingsModel {
                compute,
                ..SettingsModel::default()
            })
            .compute
        };
        assert_eq!(on(None), None);
        assert_eq!(on(Some(Compute::Metal)), Some(ON_METAL));
        assert_eq!(on(Some(Compute::Vulkan)), Some(ON_VULKAN));
        assert_eq!(on(Some(Compute::Cpu)), Some(ON_CPU));
    }

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
