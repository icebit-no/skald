//! The models Skald offers, each with its licence and who made it.

use std::borrow::Cow;
use std::path::{Path, PathBuf};

use crate::status::{self, Status};

/// A speech model in the talemodell choice (docs/design.md, Settings).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Model {
    /// What the configuration names the model by, and the name of its folder in the models folder.
    pub id: &'static str,
    pub name: &'static str,
    /// Shown as «beta» next to its name.
    pub beta: bool,
    pub availability: Availability,
    /// SPDX identifier.
    pub licence: &'static str,
    /// Who made the model, in bokmål, for the Om tab.
    pub credit: &'static str,
    /// The model card, shown as a link; Skald never fetches it.
    pub card: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Availability {
    Download(ModelFile),
    /// Listed, and offered once a file of it is hosted; where is still open.
    NotHosted,
    /// Greyed, and offered once its maker opens it to everyone.
    NotOpen,
}

/// One file to download, pinned: a URL at a fixed revision, its size and its SHA-256.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelFile {
    /// The file's name in its model's folder.
    pub name: Cow<'static, str>,
    /// Where it is fetched from, at a pinned revision. Nothing else is contacted but where this redirects
    /// to over HTTPS.
    pub url: Cow<'static, str>,
    /// Lowercase hex.
    pub sha256: Cow<'static, str>,
    /// In bytes.
    pub size: u64,
}

impl Model {
    /// The model's folder in the models folder, named by its id.
    pub fn dir(&self, models_dir: &Path) -> PathBuf {
        models_dir.join(self.id)
    }

    /// The file the engine loads, or `None` for a model this Skald cannot download.
    pub fn path(&self, models_dir: &Path) -> Option<PathBuf> {
        let file = self.file()?;
        Some(self.dir(models_dir).join(&*file.name))
    }

    pub fn file(&self) -> Option<&ModelFile> {
        match &self.availability {
            Availability::Download(file) => Some(file),
            Availability::NotHosted | Availability::NotOpen => None,
        }
    }

    /// How much of the model is on this machine, or `None` for a model this Skald cannot download.
    pub fn status(&self, models_dir: &Path) -> Option<Status> {
        Some(status::status(self.file()?, &self.dir(models_dir)))
    }
}

pub static MODELS: &[Model] = &[
    Model {
        id: "nb-whisper-large",
        name: "NB-Whisper Large",
        beta: false,
        // NbAiLab's own file (D-22). The revision and SHA-256 are the ones docs/research/engine-check.md
        // tested; the size is the repository's LFS record for that file.
        availability: Availability::Download(ModelFile {
            name: Cow::Borrowed("ggml-model.bin"),
            url: Cow::Borrowed(
                "https://huggingface.co/NbAiLab/nb-whisper-large/resolve/8c6249fdeeb4dcd05e5735a4c39640607eb6e4ac/ggml-model.bin",
            ),
            sha256: Cow::Borrowed(
                "0f2f66f22e11a7c7da3c582d8e5c89cb2c0011753ba9c7c9731e320a4ba33e76",
            ),
            size: 3_095_033_483,
        }),
        licence: "Apache-2.0",
        credit: "Laget av Nasjonalbiblioteket, videretrent fra Whisper large-v3 fra OpenAI.",
        card: "https://huggingface.co/NbAiLab/nb-whisper-large",
    },
    Model {
        id: "nb-asr-bokfink",
        name: "NB-ASR Bokfink",
        beta: true,
        // A GGUF converted from NbAiLab's weights (docs/research/bokfink-gguf.md); NbAiLab publishes none.
        availability: Availability::NotHosted,
        licence: "Apache-2.0",
        credit: "Laget av NB-ASR-prosjektet ved Nasjonalbiblioteket, bygd på Qwen3-ASR fra Alibabas Qwen-team.",
        card: "https://huggingface.co/NbAiLab/nb-asr-qwen06b-bokfink02-beta",
    },
    Model {
        id: "nordlyd",
        name: "Nordlyd",
        beta: false,
        availability: Availability::NotOpen,
        licence: "Apache-2.0",
        credit: "Laget av Nasjonalbiblioteket.",
        card: "https://huggingface.co/NbAiLab/nordlyd-asr-qwen06B-nov26preview",
    },
];

pub fn find(id: &str) -> Option<&'static Model> {
    MODELS.iter().find(|model| model.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_folder_names() {
        for (i, model) in MODELS.iter().enumerate() {
            assert!(
                model
                    .id
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
                "{}",
                model.id
            );
            assert!(MODELS[i + 1..].iter().all(|other| other.id != model.id));
        }
    }

    #[test]
    fn the_default_model_can_be_downloaded() {
        let model = find(skald_core::config::DEFAULT_MODEL).expect("in the registry");
        assert!(model.file().is_some());
        assert!(!model.beta);
    }

    #[test]
    fn the_configurations_bokfink_id_is_listed_as_beta() {
        let bokfink = find("nb-asr-bokfink").unwrap();
        assert!(bokfink.beta);
        assert_eq!(bokfink.availability, Availability::NotHosted);
        assert_eq!(find("nordlyd").unwrap().availability, Availability::NotOpen);
    }

    #[test]
    fn every_file_is_pinned_and_hashed() {
        for file in MODELS.iter().filter_map(Model::file) {
            assert!(
                file.url.starts_with("https://huggingface.co/"),
                "{}",
                file.url
            );
            let revision = file
                .url
                .split("/resolve/")
                .nth(1)
                .and_then(|rest| rest.split('/').next())
                .unwrap();
            assert!(is_lower_hex(revision, 40), "not a commit: {revision}");
            assert!(file.url.ends_with(&format!("/{}", file.name)));
            assert!(is_lower_hex(&file.sha256, 64), "{}", file.sha256);
            assert!(file.size > 0);
        }
    }

    #[test]
    fn every_model_credits_the_national_library() {
        for model in MODELS {
            assert!(model.credit.contains("Nasjonalbiblioteket"), "{}", model.id);
            assert_eq!(model.licence, "Apache-2.0");
        }
    }

    #[test]
    fn models_live_in_a_folder_named_by_id() {
        let models = Path::new("/models");
        let whisper = find("nb-whisper-large").unwrap();
        assert_eq!(
            whisper.path(models).unwrap(),
            Path::new("/models/nb-whisper-large/ggml-model.bin")
        );
        assert_eq!(find("nordlyd").unwrap().path(models), None);
    }

    fn is_lower_hex(text: &str, len: usize) -> bool {
        text.len() == len && text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
    }
}
