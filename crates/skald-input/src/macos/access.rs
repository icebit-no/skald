//! The Accessibility permission, which the key listener's event tap and the paste keystroke both need.
//!
//! An enabled event tap left in place after the permission is revoked receives nothing, yet every keyboard
//! and mouse event of the session waits on it, and the Mac stops responding. So the permission is watched
//! while the listener runs, and the tap is taken down the moment it goes. How the change is noticed and
//! checked follows https://github.com/handy-computer/handy-keys/pull/43 (MIT).

use std::ffi::{CStr, c_char, c_int, c_void};
use std::ptr::NonNull;

use objc2_core_foundation::CFMachPort;
use objc2_core_graphics::{
    CGEvent, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement, CGEventTapProxy,
    CGEventType,
};

/// tccd posts this Darwin notification after any privacy permission changes.
const TCC_ACCESS_CHANGED: &CStr = c"com.apple.tcc.access.changed";

unsafe extern "C" {
    fn notify_register_check(name: *const c_char, out_token: *mut c_int) -> u32;
    fn notify_check(token: c_int, check: *mut c_int) -> u32;
    fn notify_cancel(token: c_int) -> u32;
}

/// Says whether a privacy permission has changed since it was last asked. Asking reads shared memory, so
/// it is cheap enough to do on every wake of the listener.
#[derive(Debug)]
pub(crate) struct AccessWatch {
    token: Option<c_int>,
}

impl AccessWatch {
    pub(crate) fn new() -> Self {
        let mut token = 0;
        // Safety: a valid C string and a valid out pointer.
        let registered =
            unsafe { notify_register_check(TCC_ACCESS_CHANGED.as_ptr(), &mut token) } == 0;
        let watch = Self {
            token: registered.then_some(token),
        };
        // The first check after registering always reports a change.
        watch.changed();
        watch
    }

    pub(crate) fn changed(&self) -> bool {
        let Some(token) = self.token else {
            return false;
        };
        let mut check = 0;
        // Safety: a token from notify_register_check and a valid out pointer.
        let status = unsafe { notify_check(token, &mut check) };
        status == 0 && check != 0
    }
}

impl Drop for AccessWatch {
    fn drop(&mut self) {
        if let Some(token) = self.token {
            // Safety: a token from notify_register_check, cancelled once.
            unsafe { notify_cancel(token) };
        }
    }
}

/// Whether the permission is granted, asked fresh. AXIsProcessTrusted() answers from a cache that can
/// keep saying yes after a revoke, so a yes is confirmed by creating an active event tap, which makes the
/// window server check. A no is trusted as it is, so asking never creates a tap without the permission.
pub(crate) fn accessibility_granted() -> bool {
    handy_keys::check_accessibility() && active_tap_allowed()
}

/// Creates and at once destroys an active tap that listens for no real event.
fn active_tap_allowed() -> bool {
    unsafe extern "C-unwind" fn pass(
        _proxy: CGEventTapProxy,
        _event_type: CGEventType,
        event: NonNull<CGEvent>,
        _user_info: *mut c_void,
    ) -> *mut CGEvent {
        event.as_ptr()
    }
    // Safety: the callback returns the event it is given, and needs no user data.
    let tap = unsafe {
        CGEvent::tap_create(
            CGEventTapLocation::SessionEventTap,
            CGEventTapPlacement::TailAppendEventTap,
            CGEventTapOptions::Default,
            1 << CGEventType::Null.0,
            Some(pass),
            std::ptr::null_mut(),
        )
    };
    tap.inspect(|tap| CFMachPort::invalidate(tap)).is_some()
}

/// Opens System Settings at Privacy & Security › Accessibility.
pub fn open_accessibility_settings() -> std::io::Result<()> {
    handy_keys::open_accessibility_settings()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_watch_registers_and_reports_no_change_at_start() {
        let watch = AccessWatch::new();

        assert!(watch.token.is_some());
        assert!(!watch.changed());
    }
}
