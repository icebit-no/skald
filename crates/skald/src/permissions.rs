//! The microphone and Accessibility as the first-run window needs them: read without a prompt, asked for
//! with the system's prompt, and the System Settings pane where a refusal is undone.

use std::process::Command;

use skald_audio::MicrophoneAccess;
use skald_firstrun::{Access, Permission, PermissionChecks};

const MICROPHONE_PANE: &str =
    "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone";

/// The system's own checks, which never prompt.
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

/// Shows the system's prompt for the permission. `answered` is called, on any thread, once the user has
/// answered the microphone's prompt; Accessibility's prompt sends the user to System Settings, and the
/// window reads it again when it comes back to the front.
pub(crate) fn request(permission: Permission, answered: impl Fn() + Send + Sync + 'static) {
    match permission {
        Permission::Microphone => microphone::request(answered),
        Permission::Accessibility => {
            skald_input::request_accessibility();
        }
    }
}

/// Opens the permission's pane in System Settings.
pub(crate) fn open_settings(permission: Permission) {
    match permission {
        Permission::Microphone => {
            let _ = Command::new("open").arg(MICROPHONE_PANE).status();
        }
        Permission::Accessibility => {
            let _ = skald_input::open_accessibility_settings();
        }
    }
}

/// AVFoundation's request for the microphone, through the Objective-C runtime.
mod microphone {
    use block2::RcBlock;
    use objc2::msg_send;
    use objc2::runtime::{AnyClass, Bool};
    use objc2_foundation::NSString;

    #[link(name = "AVFoundation", kind = "framework")]
    unsafe extern "C" {
        static AVMediaTypeAudio: Option<&'static NSString>;
    }

    pub(super) fn request(answered: impl Fn() + Send + Sync + 'static) {
        let Some(class) = AnyClass::get(c"AVCaptureDevice") else {
            return;
        };
        // SAFETY: a constant AVFoundation exports, read once the class lookup has shown it is loaded.
        let Some(media_type) = (unsafe { AVMediaTypeAudio }) else {
            return;
        };
        let handler = RcBlock::new(move |_granted: Bool| answered());
        // SAFETY: the class method takes a media type and a block taking BOOL, which AVFoundation copies
        // and calls once on a queue of its own.
        unsafe {
            let _: () = msg_send![
                class,
                requestAccessForMediaType: media_type,
                completionHandler: &*handler
            ];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_microphone_reads_as_the_window_names_it() {
        assert_eq!(access(MicrophoneAccess::Granted), Access::Granted);
        assert_eq!(access(MicrophoneAccess::NotAsked), Access::NotAsked);
        assert_eq!(access(MicrophoneAccess::Denied), Access::Denied);
        assert_eq!(access(MicrophoneAccess::Restricted), Access::Restricted);
    }

    #[test]
    fn the_checks_ask_the_system_without_a_prompt() {
        let checks = checks();
        assert_eq!(
            (checks.microphone)(),
            access(skald_audio::microphone_access())
        );
        assert_eq!(
            (checks.accessibility)(),
            skald_input::accessibility_granted()
        );
    }
}
