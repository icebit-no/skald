//! The key held to talk (D-61): a modifier alone, by side, fn, or a function key. The configuration names
//! it in words, `right-command` or `f13`, and Skald names it to the user in bokmål as the keyboard of each
//! system labels it, «høyre ⌘» on a Mac and «høyre Ctrl» on a PC.

use serde::{Deserialize, Serialize};

/// The key held to talk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "String", into = "String")]
pub enum HoldKey {
    RightCommand,
    RightControl,
    RightShift,
    RightOption,
    LeftCommand,
    LeftControl,
    LeftShift,
    LeftOption,
    /// fn, the Globe key on newer Macs.
    Fn,
    /// F1 to F19, by number.
    Function(u8),
    /// A key this version does not know, written by a newer Skald, kept as written (D-19).
    Unknown(String),
}

/// The function keys a hold key can be.
pub const FUNCTION_KEYS: std::ops::RangeInclusive<u8> = 1..=19;

/// Every name the configuration takes for a hold key, as the file writes it.
pub const NAMES: [&str; 28] = [
    "right-command",
    "right-control",
    "right-shift",
    "right-option",
    "left-command",
    "left-control",
    "left-shift",
    "left-option",
    "fn",
    "f1",
    "f2",
    "f3",
    "f4",
    "f5",
    "f6",
    "f7",
    "f8",
    "f9",
    "f10",
    "f11",
    "f12",
    "f13",
    "f14",
    "f15",
    "f16",
    "f17",
    "f18",
    "f19",
];

/// Whose keyboard the key is named for: a Mac labels the keys ⌘, ⌥ and ⇧, a PC Windows or Super, Alt and
/// Shift.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Keyboard {
    Mac,
    Windows,
    Linux,
}

impl Keyboard {
    pub const CURRENT: Keyboard = if cfg!(target_os = "macos") {
        Keyboard::Mac
    } else if cfg!(windows) {
        Keyboard::Windows
    } else {
        Keyboard::Linux
    };
}

impl Default for HoldKey {
    fn default() -> Self {
        Self::default_on(Keyboard::CURRENT)
    }
}

impl HoldKey {
    /// Right ⌘ on a Mac; right Ctrl on Windows and Linux, where right Alt is AltGr.
    pub fn default_on(keyboard: Keyboard) -> Self {
        if keyboard == Keyboard::Mac {
            Self::RightCommand
        } else {
            Self::RightControl
        }
    }

    /// The key as a sentence names it on this system's keyboard: «høyre ⌘», «venstre Alt», «F13». A key
    /// this version does not know is named as the system's own, which Skald listens for in its place.
    pub fn name(&self) -> String {
        self.name_on(Keyboard::CURRENT)
    }

    pub fn name_on(&self, keyboard: Keyboard) -> String {
        let mac = keyboard == Keyboard::Mac;
        let side =
            |right: bool, key: &str| format!("{} {key}", if right { "høyre" } else { "venstre" });
        let command = match keyboard {
            Keyboard::Mac => "⌘",
            Keyboard::Windows => "Windows-tast",
            Keyboard::Linux => "Super",
        };
        match self {
            Self::RightCommand => side(true, command),
            Self::LeftCommand => side(false, command),
            Self::RightControl => side(true, if mac { "Control" } else { "Ctrl" }),
            Self::LeftControl => side(false, if mac { "Control" } else { "Ctrl" }),
            Self::RightShift => side(true, if mac { "⇧" } else { "Shift" }),
            Self::LeftShift => side(false, if mac { "⇧" } else { "Shift" }),
            // A PC keyboard labels right Alt «Alt Gr», which it is on a Norwegian layout.
            Self::RightOption if mac => side(true, "⌥"),
            Self::RightOption => "Alt Gr".to_owned(),
            Self::LeftOption => side(false, if mac { "⌥" } else { "Alt" }),
            Self::Fn => "fn".to_owned(),
            Self::Function(number) => format!("F{number}"),
            Self::Unknown(_) => Self::default_on(keyboard).name_on(keyboard),
        }
    }

    /// The name at the start of a line, as a setting shows it: «Høyre ⌘», «F13»; fn keeps its small letters,
    /// as the key does.
    pub fn label(&self) -> String {
        capitalized(&self.name())
    }

    /// What the user should know before holding this key, or nothing: a key Norwegian text needs, one
    /// shortcuts use, fn with what the Globe key does besides, or one this system does not let Skald hear.
    pub fn warning(&self) -> Option<String> {
        self.warning_on(Keyboard::CURRENT)
    }

    pub fn warning_on(&self, keyboard: Keyboard) -> Option<String> {
        let label = capitalized(&self.name_on(keyboard));
        match self {
            Self::RightOption => Some(format!(
                "{label} trengs til @, [ ], {{ }} og | på norsk tastatur."
            )),
            Self::LeftCommand | Self::LeftControl | Self::LeftShift => Some(format!(
                "{label} brukes når du skriver og i snarveier. Holder du den litt før neste tast, \
                 starter et opptak som avbrytes når tasten kommer."
            )),
            // Let go alone, the Globe key also does what macOS's keyboard settings give it.
            Self::Fn if keyboard == Keyboard::Mac => Some(GLOBE.to_owned()),
            Self::Fn if keyboard == Keyboard::Windows => Some(format!(
                "Windows lar ikke Skald høre fn-tasten, så Skald lytter etter {} i stedet.",
                Self::default_on(keyboard).name_on(keyboard)
            )),
            _ => None,
        }
    }

    /// Whether the key is one this version knows.
    pub fn is_known(&self) -> bool {
        !matches!(self, Self::Unknown(_))
    }
}

/// What the Globe key does besides, when fn is the hold key on a Mac.
const GLOBE: &str = "🌐 gjør også det som står under Systeminnstillinger › Tastatur › «Trykk på 🌐-tasten \
     for å». Sett den til «Gjør ingenting» hvis du bruker 🌐 til Skald.";

fn capitalized(name: &str) -> String {
    if name == "fn" {
        return name.to_owned();
    }
    let mut chars = name.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

impl From<String> for HoldKey {
    fn from(name: String) -> Self {
        match name.as_str() {
            "right-command" => Self::RightCommand,
            "right-control" => Self::RightControl,
            "right-shift" => Self::RightShift,
            "right-option" => Self::RightOption,
            "left-command" => Self::LeftCommand,
            "left-control" => Self::LeftControl,
            "left-shift" => Self::LeftShift,
            "left-option" => Self::LeftOption,
            "fn" => Self::Fn,
            _ => match name
                .strip_prefix('f')
                .and_then(|number| number.parse::<u8>().ok())
            {
                // «f05» is not how the file writes F5, so it stays as written.
                Some(number) if FUNCTION_KEYS.contains(&number) && name == format!("f{number}") => {
                    Self::Function(number)
                }
                _ => Self::Unknown(name),
            },
        }
    }
}

impl From<HoldKey> for String {
    fn from(key: HoldKey) -> Self {
        match key {
            HoldKey::RightCommand => "right-command".to_owned(),
            HoldKey::RightControl => "right-control".to_owned(),
            HoldKey::RightShift => "right-shift".to_owned(),
            HoldKey::RightOption => "right-option".to_owned(),
            HoldKey::LeftCommand => "left-command".to_owned(),
            HoldKey::LeftControl => "left-control".to_owned(),
            HoldKey::LeftShift => "left-shift".to_owned(),
            HoldKey::LeftOption => "left-option".to_owned(),
            HoldKey::Fn => "fn".to_owned(),
            HoldKey::Function(number) => format!("f{number}"),
            HoldKey::Unknown(name) => name,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_name_reads_as_a_known_key_and_writes_back_the_same() {
        for name in NAMES {
            let key = HoldKey::from(name.to_owned());
            assert!(key.is_known(), "{name}");
            assert_eq!(String::from(key), name);
        }
        assert_eq!(HoldKey::from("f13".to_owned()), HoldKey::Function(13));
        assert_eq!(HoldKey::from("fn".to_owned()), HoldKey::Fn);
    }

    #[test]
    fn a_name_this_version_does_not_know_is_kept_as_written() {
        for name in ["f20", "f0", "f05", "F5", "right-hyper", "", "f"] {
            assert_eq!(
                HoldKey::from(name.to_owned()),
                HoldKey::Unknown(name.to_owned()),
                "{name}"
            );
            assert_eq!(String::from(HoldKey::Unknown(name.to_owned())), name);
        }
    }

    #[test]
    fn the_keys_are_named_as_each_keyboard_labels_them() {
        use Keyboard::{Linux, Mac, Windows};
        let named = |key: HoldKey, keyboard| key.name_on(keyboard);
        assert_eq!(named(HoldKey::RightCommand, Mac), "høyre ⌘");
        assert_eq!(named(HoldKey::RightCommand, Windows), "høyre Windows-tast");
        assert_eq!(named(HoldKey::LeftCommand, Linux), "venstre Super");
        assert_eq!(named(HoldKey::RightControl, Mac), "høyre Control");
        assert_eq!(named(HoldKey::RightControl, Windows), "høyre Ctrl");
        assert_eq!(named(HoldKey::LeftShift, Mac), "venstre ⇧");
        assert_eq!(named(HoldKey::RightShift, Linux), "høyre Shift");
        assert_eq!(named(HoldKey::RightOption, Mac), "høyre ⌥");
        assert_eq!(named(HoldKey::RightOption, Windows), "Alt Gr");
        assert_eq!(named(HoldKey::LeftOption, Windows), "venstre Alt");
        assert_eq!(named(HoldKey::Fn, Mac), "fn");
        assert_eq!(named(HoldKey::Function(13), Linux), "F13");
        assert_eq!(
            named(HoldKey::Unknown("right-hyper".into()), Mac),
            HoldKey::default_on(Mac).name_on(Mac),
            "the key listened for in its place"
        );
    }

    #[test]
    fn a_label_starts_with_a_capital_but_fn_keeps_its_small_letters() {
        assert_eq!(capitalized("høyre ⌘"), "Høyre ⌘");
        assert_eq!(capitalized("fn"), "fn");
        assert_eq!(capitalized("F5"), "F5");
    }

    #[test]
    fn keys_norwegian_text_or_shortcuts_need_are_warned_of_and_others_are_not() {
        use Keyboard::{Mac, Windows};
        assert_eq!(
            HoldKey::RightOption.warning_on(Mac).as_deref(),
            Some("Høyre ⌥ trengs til @, [ ], { } og | på norsk tastatur.")
        );
        assert_eq!(
            HoldKey::RightOption.warning_on(Windows).as_deref(),
            Some("Alt Gr trengs til @, [ ], { } og | på norsk tastatur.")
        );
        for key in [
            HoldKey::LeftCommand,
            HoldKey::LeftControl,
            HoldKey::LeftShift,
        ] {
            let warning = key.warning_on(Mac).unwrap();
            assert!(warning.contains("snarveier"), "{warning}");
        }
        assert!(
            HoldKey::LeftShift
                .warning_on(Mac)
                .unwrap()
                .starts_with("Venstre ⇧ brukes")
        );
        assert!(
            HoldKey::Fn
                .warning_on(Windows)
                .unwrap()
                .contains("høyre Ctrl")
        );
        assert_eq!(
            HoldKey::Fn.warning_on(Mac).as_deref(),
            Some(
                "🌐 gjør også det som står under Systeminnstillinger › Tastatur › «Trykk på 🌐-tasten \
                 for å». Sett den til «Gjør ingenting» hvis du bruker 🌐 til Skald."
            )
        );
        assert_eq!(HoldKey::Fn.warning_on(Keyboard::Linux), None);
        for key in [
            HoldKey::RightCommand,
            HoldKey::RightControl,
            HoldKey::RightShift,
            HoldKey::LeftOption,
            HoldKey::Function(13),
        ] {
            assert_eq!(key.warning_on(Mac), None, "{key:?}");
        }
    }
}
