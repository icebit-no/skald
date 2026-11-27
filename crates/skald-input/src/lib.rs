//! The hold-to-talk key, the hands-free chord and insertion: arming, cancelling and the watchdog, and
//! pasting the text where the cursor is.
//!
//! [`HoldListener`] reports [`HoldEvent`]s for the hold key and for ⌃⌘X, which starts and stops a
//! hands-free take on macOS; [`Inserter`] pastes a take's text and puts the previous clipboard back. The
//! rules of a hold live in [`HoldMachine`], free of any platform. macOS is supported. On Linux the
//! desktop's key binding reports the hold key through [`press_hold_key`], and Skald inserts no text there.
//! Windows brings its own key state, permission watch, hands-free chord and clipboard.
//!
//! Known limit: ⌘V is sent as the key in V's place on a QWERTY keyboard, which the Norwegian layout shares.
//! On a layout that moves V, such as Dvorak, it is another shortcut.

mod hold;
// Their crate-private parts serve the macOS listener and inserter only.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
mod hotkey;
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
mod insert;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod listener;
mod listening;
#[cfg(target_os = "macos")]
mod macos;

pub use hold::{ARM_AFTER, HoldEvent, HoldMachine, KeySample, WATCHDOG_EVERY};
pub use hotkey::HoldKey;
#[cfg(target_os = "macos")]
pub use insert::Inserter;
pub use insert::{InsertError, InsertOptions, Inserted};
#[cfg(target_os = "linux")]
pub use linux::access::{
    accessibility_granted, open_accessibility_settings, request_accessibility,
};
#[cfg(target_os = "linux")]
pub use linux::insert::Inserter;
#[cfg(target_os = "linux")]
pub use linux::listener::{HoldListener, press_hold_key};
#[cfg(target_os = "macos")]
pub use listener::HoldListener;
pub use listening::{ListenerEvent, PressRefused, Unavailable};
#[cfg(target_os = "macos")]
pub use macos::access::{
    accessibility_granted, open_accessibility_settings, request_accessibility,
};

/// Presses or releases the hold key for a key binding of the desktop. Here Skald hears the key itself, so
/// a binding has nothing to add.
#[cfg(not(target_os = "linux"))]
pub fn press_hold_key(_down: bool) -> Result<(), PressRefused> {
    Err(PressRefused::KeyHeard)
}
