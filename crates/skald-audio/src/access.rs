//! Whether this program may record from a microphone.
//!
//! macOS asks the user once per app. A program the user refused still opens every microphone, but gets
//! digital silence from it rather than an error, so a take cannot tell by itself; this asks the system.
//! Asking never shows the system's prompt. Windows and Linux have no such check here and report
//! [`MicrophoneAccess::Granted`].

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MicrophoneAccess {
    Granted,
    /// The user has not answered yet; the system asks when a microphone is first opened, and that take
    /// records silence while the question is open.
    NotAsked,
    /// The user refused, in the system's prompt or later in its settings.
    Denied,
    /// A policy on the machine forbids it, and the user cannot change that.
    Restricted,
}

pub fn microphone_access() -> MicrophoneAccess {
    #[cfg(target_os = "macos")]
    return macos::status().map_or(MicrophoneAccess::NotAsked, from_av_status);
    #[cfg(not(target_os = "macos"))]
    return MicrophoneAccess::Granted;
}

/// AVFoundation's `AVAuthorizationStatus`. A value this version does not know leaves the answer to the
/// system when the microphone opens.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn from_av_status(status: isize) -> MicrophoneAccess {
    match status {
        1 => MicrophoneAccess::Restricted,
        2 => MicrophoneAccess::Denied,
        3 => MicrophoneAccess::Granted,
        _ => MicrophoneAccess::NotAsked,
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use objc2::msg_send;
    use objc2::runtime::AnyClass;
    use objc2_foundation::NSString;

    #[link(name = "AVFoundation", kind = "framework")]
    unsafe extern "C" {
        static AVMediaTypeAudio: Option<&'static NSString>;
    }

    /// `[AVCaptureDevice authorizationStatusForMediaType:AVMediaTypeAudio]`, or `None` when AVFoundation
    /// does not answer.
    pub(super) fn status() -> Option<isize> {
        let class = AnyClass::get(c"AVCaptureDevice")?;
        // SAFETY: a constant AVFoundation exports, read once the framework is loaded, which the class
        // lookup above has shown.
        let media_type = unsafe { AVMediaTypeAudio }?;
        // SAFETY: the class method takes a media type string and returns an NSInteger; it only reads the
        // permission store and never prompts.
        let status: isize =
            unsafe { msg_send![class, authorizationStatusForMediaType: media_type] };
        Some(status)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_status_avfoundation_gives_has_its_answer() {
        assert_eq!(from_av_status(0), MicrophoneAccess::NotAsked);
        assert_eq!(from_av_status(1), MicrophoneAccess::Restricted);
        assert_eq!(from_av_status(2), MicrophoneAccess::Denied);
        assert_eq!(from_av_status(3), MicrophoneAccess::Granted);
        assert_eq!(from_av_status(7), MicrophoneAccess::NotAsked);
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn the_system_answers_without_a_prompt() {
        assert!(macos::status().is_some_and(|status| (0..=3).contains(&status)));
    }
}
