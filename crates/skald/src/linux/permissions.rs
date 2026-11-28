//! The permissions the first-run window shows, which Linux does not ask for: a program may open the
//! microphone, and the hold key comes from the desktop's key binding. Both read as granted, and there is no
//! prompt and no settings pane.

use skald_audio::MicrophoneAccess;
use skald_firstrun::{Access, ListedAs, Permission, PermissionChecks};

pub(crate) fn checks() -> PermissionChecks {
    PermissionChecks {
        microphone: Box::new(|| access(skald_audio::microphone_access())),
        accessibility: Box::new(skald_input::accessibility_granted),
    }
}

pub(crate) fn access(microphone: MicrophoneAccess) -> Access {
    match microphone {
        MicrophoneAccess::Granted => Access::Granted,
        MicrophoneAccess::NotAsked => Access::NotAsked,
        MicrophoneAccess::Denied => Access::Denied,
        MicrophoneAccess::Restricted => Access::Restricted,
    }
}

/// Nothing is asked, so the answer is there at once.
pub(crate) fn request(_permission: Permission, answered: impl Fn() + Send + Sync + 'static) {
    answered();
}

pub(crate) fn open_settings(_permission: Permission) {}

/// Only macOS lists the permissions under the program Skald was started from.
pub(crate) fn listed_as() -> ListedAs {
    ListedAs::Skald
}
