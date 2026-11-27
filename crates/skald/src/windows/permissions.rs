//! The permissions the first-run window shows, which Windows does not ask for: the microphone is open to
//! desktop programs unless the user has turned that off in Windows' privacy settings, and the key and the
//! paste need no grant. Both read as granted, and there is no prompt and no settings pane.

use skald_audio::MicrophoneAccess;
use skald_firstrun::{Access, Permission, PermissionChecks};

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
