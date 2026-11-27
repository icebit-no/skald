//! What the app tells the menu bar item, and what the item asks of the app.

use skald_core::config::{HoldKey, Language};

/// Everything the item and its menu show. The app builds it from its own state and hands it to
/// [`Tray::update`](crate::Tray::update) whenever any part changes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TrayModel {
    pub activity: Activity,
    /// The key the app listens for, which the menu's hint names.
    pub hold_key: HoldKey,
    pub language: Language,
    /// In the order the «Mikrofon» submenu lists them.
    pub microphones: Vec<Microphone>,
    /// The id of the microphone a take uses now.
    pub in_use: Option<String>,
    /// The id of the microphone chosen in the menu, put ahead of the user's order until the pin is
    /// removed (D-20).
    pub pinned: Option<String>,
    /// Newest first.
    pub latest_texts: Vec<String>,
}

/// What a take is doing, shown as a dot beside the rune.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Activity {
    #[default]
    Idle,
    /// Held or hands-free: a red dot.
    Recording,
    /// A gold dot.
    Transcribing,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Microphone {
    /// The app's key for it, handed back in [`Action::PinMicrophone`].
    pub id: String,
    /// The system's name for it, which the menu shows.
    pub name: String,
    /// A microphone that is not connected is listed greyed, so a pinned one that is unplugged still
    /// shows its pin.
    pub connected: bool,
}

/// What an entry of the menu asks of the app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Put this microphone ahead of the order, by its [`Microphone::id`].
    PinMicrophone(String),
    /// «Bruk den første som er koblet til»: the order applies again.
    UnpinMicrophone,
    /// One of «Siste tekster», whole, for the clipboard.
    CopyText(String),
    SetLanguage(Language),
    /// «Ordskatt …»
    OpenVocabulary,
    /// «Innstillinger …»
    OpenSettings,
    Quit,
}
