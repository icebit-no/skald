//! The hold-to-talk key, the hands-free chord and insertion: arming, cancelling and the watchdog, and
//! pasting the text where the cursor is.
//!
//! [`HoldListener`] reports [`HoldEvent`]s for the hold key and for ⌃⌘X, which starts and stops a
//! hands-free take on macOS; [`Inserter`] pastes a take's text and puts the previous clipboard back. The
//! rules of a hold live in [`HoldMachine`], free of any platform. macOS and Windows are supported; Windows
//! has no hands-free chord yet. On Linux the desktop's key binding reports the hold key through
//! [`press_hold_key`], and Skald inserts no text there.
//!
//! Known limit: on macOS, ⌘V is sent as the key in V's place on a QWERTY keyboard, which the Norwegian
//! layout shares. On a layout that moves V, such as Dvorak, it is another shortcut. Windows sends Ctrl+V by
//! its virtual key, which every layout reads as Ctrl+V.

mod hold;
// Their crate-private parts serve the macOS and Windows listener and inserter only.
#[cfg_attr(not(any(target_os = "macos", windows)), allow(dead_code))]
mod hotkey;
#[cfg_attr(not(any(target_os = "macos", windows)), allow(dead_code))]
mod insert;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(any(target_os = "macos", windows))]
mod listener;
mod listening;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod windows;

pub use hold::{ARM_AFTER, HoldEvent, HoldMachine, KeySample, WATCHDOG_EVERY};
pub use hotkey::HoldKey;
#[cfg(any(target_os = "macos", windows))]
pub use insert::Inserter;
pub use insert::{InsertError, InsertOptions, Inserted};
#[cfg(target_os = "linux")]
pub use linux::access::{
    accessibility_granted, open_accessibility_settings, request_accessibility,
};
#[cfg(target_os = "linux")]
pub use linux::insert::Inserter;
#[cfg(target_os = "linux")]
pub use linux::listener::{HoldListener, press_hold_key, toggle_hands_free};
#[cfg(any(target_os = "macos", windows))]
pub use listener::HoldListener;
pub use listening::{ListenerEvent, PressRefused, Unavailable};
#[cfg(target_os = "macos")]
pub use macos::access::{
    accessibility_granted, open_accessibility_settings, request_accessibility,
};

/// Presses or releases the hold key for a key binding of the desktop. Here Skald listens to the keyboard
/// itself, so a binding has nothing to add.
#[cfg(not(target_os = "linux"))]
pub fn press_hold_key(_down: bool) -> Result<(), PressRefused> {
    Err(PressRefused::KeyHeard)
}

/// Presses the hands-free chord for a key binding of the desktop. Here Skald listens to the keyboard
/// itself, so a binding has nothing to add.
#[cfg(not(target_os = "linux"))]
pub fn toggle_hands_free() -> Result<(), PressRefused> {
    Err(PressRefused::KeyHeard)
}
