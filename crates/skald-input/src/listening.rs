//! What a hold listener reports, whichever way it learns of the hold key.

use std::fmt;

use crate::hold::{HoldEvent, KeySample};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ListenerEvent {
    /// The hold key is listened for.
    Ready,
    /// The hold key is not listened for, and why. The listener keeps trying and reports
    /// [`ListenerEvent::Ready`] once it can listen again.
    Unavailable(Unavailable),
    Hold(HoldEvent),
    /// What the listener saw on the way to a hold, so a log can show where a press that started no take
    /// was lost.
    Seen(Seen),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Seen {
    /// The first key event since the listener started listening: the key's or modifier's name, and
    /// whether it went down.
    FirstKey { key: String, down: bool },
    /// An event said the hold key went down or up; `sample` is what the keyboard hardware said then,
    /// which decides it.
    HoldKey { down: bool, sample: KeySample },
    /// The system's record of this process's event taps, a moment after the listener started listening,
    /// and whether the system lets this process listen to the keyboard.
    Taps {
        taps: Vec<Tap>,
        keyboard_allowed: bool,
    },
}

/// One event tap as the system records it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tap {
    pub enabled: bool,
    /// Only listens, rather than being able to hold an event back.
    pub listen_only: bool,
    /// The longest an event has waited on the tap, in microseconds.
    pub max_latency_us: u32,
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
    /// Skald listens to the keyboard itself on this system, so a binding has nothing to add.
    KeyHeard,
    /// No hold listener runs.
    NotListening,
}

impl fmt::Display for PressRefused {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            PressRefused::KeyHeard => "Skald listens to the keyboard itself on this system",
            PressRefused::NotListening => "no hold listener runs",
        })
    }
}

impl std::error::Error for PressRefused {}
