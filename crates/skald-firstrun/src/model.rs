//! What the app hands the window: the model to download and how far it has come, the permissions, and the
//! test dictation.

use skald_core::config::HoldKey;
use skald_models::{Model, Status};

use crate::download::Download;

/// Everything the window shows. The app keeps one, changes it as the download, the permissions and the
/// test move on, and hands it to the window each time.
#[derive(Debug, Clone, PartialEq)]
pub struct FirstRunModel {
    pub platform: Platform,
    pub hold_key: HoldKey,
    pub model: ModelCard,
    pub download: Download,
    pub microphone: Access,
    /// Asked for on macOS only.
    pub accessibility: Access,
    pub test: TestDictation,
}

impl FirstRunModel {
    /// For this machine, with the default hold key and both permissions not yet asked for.
    pub fn new(model: ModelCard, status: Status) -> Self {
        Self {
            platform: Platform::CURRENT,
            hold_key: HoldKey::default(),
            model,
            download: Download::from_status(status),
            microphone: Access::NotAsked,
            accessibility: Access::NotAsked,
            test: TestDictation::NotStarted,
        }
    }

    pub fn permissions_granted(&self) -> bool {
        self.microphone == Access::Granted
            && (!self.platform.asks_for_accessibility() || self.accessibility == Access::Granted)
    }

    /// The model is in place and checked, and every permission is granted: Skald can take dictation.
    pub fn ready(&self) -> bool {
        self.download == Download::Ready && self.permissions_granted()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    MacOs,
    Windows,
    Linux,
}

impl Platform {
    pub const CURRENT: Platform = if cfg!(target_os = "macos") {
        Platform::MacOs
    } else if cfg!(windows) {
        Platform::Windows
    } else {
        Platform::Linux
    };

    pub fn asks_for_accessibility(self) -> bool {
        self == Platform::MacOs
    }
}

/// Whether the system lets Skald use something it guards.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    Granted,
    /// Not granted, and the system can still be asked: it shows its own prompt.
    NotAsked,
    /// Refused; only the system's settings can undo it.
    Denied,
    /// A policy on the machine forbids it, and the user cannot change that.
    Restricted,
}

/// The model as step one presents it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelCard {
    pub name: &'static str,
    /// In bytes.
    pub size: u64,
    /// SPDX identifier.
    pub licence: &'static str,
    pub credit: &'static str,
    /// The pinned SHA-256 the file is checked against, lowercase hex.
    pub sha256: String,
}

impl ModelCard {
    /// `None` for a model this Skald cannot download.
    pub fn of(model: &Model) -> Option<Self> {
        let file = model.file()?;
        Some(Self {
            name: model.name,
            size: file.size,
            licence: model.licence,
            credit: model.credit,
            sha256: file.sha256.to_string(),
        })
    }
}

/// The test dictation in step three, as the app's pipeline runs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TestDictation {
    NotStarted,
    /// The model loads before the first take, which takes a few seconds.
    Loading,
    /// Waiting for the hold key.
    Listening,
    Recording,
    Transcribing,
    Heard {
        text: String,
    },
    HeardNothing {
        microphone: String,
    },
    /// The test could not run; the reason is in bokmål, as the app tells the user elsewhere.
    Failed {
        reason: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn whisper() -> ModelCard {
        ModelCard::of(skald_models::find("nb-whisper-large").unwrap()).unwrap()
    }

    #[test]
    fn the_card_carries_the_registrys_name_size_licence_and_credit() {
        let card = whisper();
        assert_eq!(card.name, "NB-Whisper Large");
        assert_eq!(card.size, 3_095_033_483);
        assert_eq!(card.licence, "Apache-2.0");
        assert!(card.credit.contains("Nasjonalbiblioteket"));
        assert_eq!(card.sha256.len(), 64);
        assert_eq!(
            ModelCard::of(skald_models::find("nordlyd").unwrap()),
            None,
            "a model that cannot be downloaded has no first run"
        );
    }

    #[test]
    fn ready_needs_the_checked_model_and_every_permission_the_platform_asks_for() {
        let mut model = FirstRunModel::new(whisper(), Status::Ready);
        model.platform = Platform::MacOs;
        model.microphone = Access::Granted;
        assert!(!model.ready(), "Accessibility is still missing on macOS");
        model.accessibility = Access::Granted;
        assert!(model.ready());

        model.platform = Platform::Linux;
        model.accessibility = Access::Denied;
        assert!(model.ready(), "Linux asks for no Accessibility");

        model.download = Download::Stopped { received: 10 };
        assert!(!model.ready());
    }
}
