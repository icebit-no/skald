//! The hold-to-talk key and insertion: arming, cancelling and the watchdog, and pasting the text where the
//! cursor is.
//!
//! [`HoldListener`] reports [`HoldEvent`]s for the hold key; [`Inserter`] pastes a take's text and puts
//! the previous clipboard back. The rules of a hold live in [`HoldMachine`], free of any platform. macOS
//! is supported; Windows and Linux bring their own key state, permission watch and clipboard.
//!
//! Known limit: ⌘V is sent as the key in V's place on a QWERTY keyboard, which the Norwegian layout shares.
//! On a layout that moves V, such as Dvorak, it is another shortcut.

mod hold;
// Their crate-private parts serve the macOS listener and inserter only.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
mod hotkey;
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
mod insert;
#[cfg(target_os = "macos")]
mod listener;
#[cfg(target_os = "macos")]
mod macos;

pub use hold::{ARM_AFTER, HoldEvent, HoldMachine, KeySample, WATCHDOG_EVERY};
pub use hotkey::HoldKey;
#[cfg(target_os = "macos")]
pub use insert::Inserter;
pub use insert::{InsertError, InsertOptions, Inserted};
#[cfg(target_os = "macos")]
pub use listener::{HoldListener, ListenerEvent, Unavailable};
#[cfg(target_os = "macos")]
pub use macos::access::open_accessibility_settings;
