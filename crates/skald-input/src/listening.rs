//! What a hold listener reports, whichever way it learns of the hold key.

use std::fmt;

use crate::hold::HoldEvent;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ListenerEvent {
    /// The hold key is listened for.
    Ready,
    /// The hold key is not listened for, and why. The listener keeps trying and reports
    /// [`ListenerEvent::Ready`] once it can listen again.
    Unavailable(Unavailable),
    Hold(HoldEvent),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unavailable {
    /// macOS Accessibility is not granted, or was revoked.
    NoAccessibility,
    /// The key listener could not start or stopped, for the reason given.
    ListenerFailed(String),
}

impl fmt::Display for Unavailable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Unavailable::NoAccessibility => f.write_str("Accessibility is not granted"),
            Unavailable::ListenerFailed(reason) => write!(f, "the key listener failed: {reason}"),
        }
    }
}

/// Why a press from a key binding of the desktop reached no listener.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PressRefused {
    /// Skald hears the hold key itself on this system, so a binding has nothing to add.
    KeyHeard,
    /// No hold listener runs.
    NotListening,
}

impl fmt::Display for PressRefused {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            PressRefused::KeyHeard => "Skald hears the hold key itself on this system",
            PressRefused::NotListening => "no hold listener runs",
        })
    }
}

impl std::error::Error for PressRefused {}
