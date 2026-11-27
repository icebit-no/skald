//! What the app hands the window: the model to download and how far it has come, the test dictation, and
//! how to read the permissions.

use std::fmt;

use skald_core::config::HoldKey;
use skald_models::{Model, Status};

use crate::download::Download;

/// What the window shows of the app's work. The app keeps one, changes it as the download and the test
/// move on, and hands it to the window each time.
#[derive(Debug, Clone, PartialEq)]
pub struct FirstRunModel {
    pub platform: Platform,
    pub hold_key: HoldKey,
    pub model: ModelCard,
    pub download: Download,
    pub test: TestDictation,
    /// The machine has no GPU with a Vulkan driver, so the model will compute on the CPU, and the model's
    /// step says so.
    pub no_vulkan: bool,
}

/// What the model's step says when [`FirstRunModel::no_vulkan`] holds.
pub const NO_VULKAN: &str = "Fant ingen skjermkortdriver med Vulkan, så talemodellen regner på prosessoren og bruker lengre tid.";

impl FirstRunModel {
    /// For this machine, with the default hold key.
    pub fn new(model: ModelCard, status: Status) -> Self {
        Self {
            platform: Platform::CURRENT,
            hold_key: HoldKey::default(),
            model,
            download: Download::from_status(status),
            test: TestDictation::NotStarted,
            no_vulkan: false,
        }
    }

    /// The model is in place and checked, and every permission is granted: Skald can take dictation.
    pub fn ready(&self, permissions: &Permissions) -> bool {
        self.download == Download::Ready && permissions.granted(self.platform)
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

/// How the window reads the permissions, which it does when it opens and each time it comes to the
/// front, as when the user comes back from the system's settings. The app passes in the system's own
/// checks; asking never shows a prompt.
pub struct PermissionChecks {
    pub microphone: Box<dyn Fn() -> Access>,
    /// Whether Accessibility is granted. Asked on macOS only.
    pub accessibility: Box<dyn Fn() -> bool>,
}

impl PermissionChecks {
    /// `asked_for_accessibility` says whether this window has asked the system for Accessibility.
    pub fn read(&self, platform: Platform, asked_for_accessibility: bool) -> Permissions {
        let accessibility = if platform.asks_for_accessibility() {
            Permissions::accessibility((self.accessibility)(), asked_for_accessibility)
        } else {
            Access::Granted
        };
        Permissions {
            microphone: (self.microphone)(),
            accessibility,
        }
    }
}

impl fmt::Debug for PermissionChecks {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PermissionChecks").finish_non_exhaustive()
    }
}

/// The permissions as the window last read them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Permissions {
    pub microphone: Access,
    pub accessibility: Access,
}

impl Permissions {
    pub fn granted(&self, platform: Platform) -> bool {
        self.microphone == Access::Granted
            && (!platform.asks_for_accessibility() || self.accessibility == Access::Granted)
    }

    /// macOS says only whether Accessibility is granted, not whether the user was ever asked. Until the
    /// window has asked, the system's prompt can still help; after that, only System Settings can.
    pub fn accessibility(granted: bool, asked: bool) -> Access {
        match (granted, asked) {
            (true, _) => Access::Granted,
            (false, false) => Access::NotAsked,
            (false, true) => Access::Denied,
        }
    }
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
    /// The take's text, as it was inserted.
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
    use std::cell::Cell;
    use std::rc::Rc;

    use super::*;

    fn whisper() -> ModelCard {
        ModelCard::of(skald_models::find("nb-whisper-large").unwrap()).unwrap()
    }

    const GRANTED: Permissions = Permissions {
        microphone: Access::Granted,
        accessibility: Access::Granted,
    };

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
        let mut permissions = GRANTED;
        permissions.accessibility = Access::NotAsked;
        assert!(
            !model.ready(&permissions),
            "Accessibility is still missing on macOS"
        );
        assert!(model.ready(&GRANTED));

        model.platform = Platform::Linux;
        assert!(model.ready(&permissions), "Linux asks for no Accessibility");

        model.download = Download::Stopped { received: 10 };
        assert!(!model.ready(&GRANTED));
    }

    #[test]
    fn accessibility_counts_as_refused_once_the_window_has_asked() {
        assert_eq!(Permissions::accessibility(true, false), Access::Granted);
        assert_eq!(Permissions::accessibility(true, true), Access::Granted);
        assert_eq!(Permissions::accessibility(false, false), Access::NotAsked);
        assert_eq!(Permissions::accessibility(false, true), Access::Denied);
    }

    #[test]
    fn the_checks_are_asked_and_accessibility_only_on_macos() {
        let asked = Rc::new(Cell::new(0));
        let counting = asked.clone();
        let checks = PermissionChecks {
            microphone: Box::new(|| Access::Denied),
            accessibility: Box::new(move || {
                counting.set(counting.get() + 1);
                false
            }),
        };
        assert_eq!(
            checks.read(Platform::MacOs, true),
            Permissions {
                microphone: Access::Denied,
                accessibility: Access::Denied
            }
        );
        assert_eq!(asked.get(), 1);
        assert_eq!(
            checks.read(Platform::Windows, false).accessibility,
            Access::Granted
        );
        assert_eq!(asked.get(), 1, "Windows has no Accessibility to check");
    }
}
