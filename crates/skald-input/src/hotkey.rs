//! The hold-to-talk key, the hands-free chord, and how handy-keys' key events read for them.

use handy_keys::{Hotkey, Key, KeyEvent, Modifiers};

/// The key held to talk: a modifier alone, told apart from its twin on the other side, fn, or a function
/// key. handy-keys reads ⌘ as the Windows key on Windows and ⌥ as Alt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HoldKey {
    RightCommand,
    RightControl,
    RightShift,
    RightOption,
    LeftCommand,
    LeftControl,
    LeftShift,
    LeftOption,
    Fn,
    /// F1 to F19, by number.
    Function(u8),
}

/// handy-keys' function keys a hold key can be, F1 first.
const FUNCTION_KEYS: [Key; 19] = [
    Key::F1,
    Key::F2,
    Key::F3,
    Key::F4,
    Key::F5,
    Key::F6,
    Key::F7,
    Key::F8,
    Key::F9,
    Key::F10,
    Key::F11,
    Key::F12,
    Key::F13,
    Key::F14,
    Key::F15,
    Key::F16,
    Key::F17,
    Key::F18,
    Key::F19,
];

/// The modifiers a hold key can be, by side, as handy-keys names them from each event's key code.
const MODIFIERS: [(Modifiers, HoldKey); 9] = [
    (Modifiers::CMD_RIGHT, HoldKey::RightCommand),
    (Modifiers::CTRL_RIGHT, HoldKey::RightControl),
    (Modifiers::SHIFT_RIGHT, HoldKey::RightShift),
    (Modifiers::OPT_RIGHT, HoldKey::RightOption),
    (Modifiers::CMD_LEFT, HoldKey::LeftCommand),
    (Modifiers::CTRL_LEFT, HoldKey::LeftControl),
    (Modifiers::SHIFT_LEFT, HoldKey::LeftShift),
    (Modifiers::OPT_LEFT, HoldKey::LeftOption),
    (Modifiers::FN, HoldKey::Fn),
];

impl HoldKey {
    /// Right ⌘ on macOS; right Ctrl on Windows and Linux, where right Alt is AltGr.
    pub const PLATFORM_DEFAULT: HoldKey = if cfg!(target_os = "macos") {
        HoldKey::RightCommand
    } else {
        HoldKey::RightControl
    };

    /// The modifier, as handy-keys names it by side; none for a function key.
    pub(crate) fn modifier(self) -> Option<Modifiers> {
        MODIFIERS
            .iter()
            .find(|(_, key)| *key == self)
            .map(|(modifier, _)| *modifier)
    }

    /// The same modifier on the other side; none for fn and a function key.
    pub(crate) fn twin(self) -> Option<Modifiers> {
        let twin = match self {
            HoldKey::RightCommand => HoldKey::LeftCommand,
            HoldKey::LeftCommand => HoldKey::RightCommand,
            HoldKey::RightControl => HoldKey::LeftControl,
            HoldKey::LeftControl => HoldKey::RightControl,
            HoldKey::RightShift => HoldKey::LeftShift,
            HoldKey::LeftShift => HoldKey::RightShift,
            HoldKey::RightOption => HoldKey::LeftOption,
            HoldKey::LeftOption => HoldKey::RightOption,
            HoldKey::Fn | HoldKey::Function(_) => return None,
        };
        twin.modifier()
    }

    /// The function key; none for a modifier, or for a number outside F1 to F19.
    pub(crate) fn key(self) -> Option<Key> {
        let HoldKey::Function(number) = self else {
            return None;
        };
        FUNCTION_KEYS
            .get(usize::from(number).checked_sub(1)?)
            .copied()
    }

    /// The hold key a lone modifier is.
    pub(crate) fn of_modifier(modifier: Modifiers) -> Option<HoldKey> {
        MODIFIERS
            .iter()
            .find(|(bit, _)| *bit == modifier)
            .map(|(_, key)| *key)
    }

    /// The hold key a function key is.
    pub(crate) fn of_key(key: Key) -> Option<HoldKey> {
        let index = FUNCTION_KEYS.iter().position(|function| *function == key)?;
        Some(HoldKey::Function(index as u8 + 1))
    }

    /// What handy-keys keeps from the app in front: a function key held, alone or with fn, which laptops
    /// need to send it, since the app would act on it. A modifier alone does nothing in an app.
    pub(crate) fn kept_from_apps(self) -> Vec<Hotkey> {
        self.key()
            .map(|key| {
                [Modifiers::empty(), Modifiers::FN]
                    .map(|modifiers| Hotkey {
                        modifiers,
                        key: Some(key),
                    })
                    .to_vec()
            })
            .unwrap_or_default()
    }
}

/// A key pressed with modifiers, either side of each.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Chord {
    modifiers: Modifiers,
    key: Key,
}

impl Chord {
    /// The same chord for handy-keys, which keeps it from reaching the app in front.
    pub(crate) fn hotkey(self) -> Hotkey {
        Hotkey {
            modifiers: self.modifiers,
            key: Some(self.key),
        }
    }
}

/// ⌃⌘X, which starts and stops a hands-free take (docs/design.md, Recording). handy-keys reads ⌘ as the
/// Windows key on Windows, so there the same chord is Ctrl+Win+X.
const CONTROL_COMMAND_X: Chord = Chord {
    modifiers: Modifiers::CTRL.union(Modifiers::CMD),
    key: Key::X,
};

/// The hands-free chord the listener catches on this platform. On Linux the desktop's own key binding starts
/// and stops a hands-free take instead.
pub(crate) const HANDS_FREE: Option<Chord> = if cfg!(any(target_os = "macos", windows)) {
    Some(CONTROL_COMMAND_X)
} else {
    None
};

/// What a key event means for a hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum KeyChange {
    /// The hold key went down or up. For a modifier, which one is read from the operating system's
    /// modifier flags with the side the events give, because the press flag handy-keys reports for a
    /// modifier can be inverted after a missed event; a function key's event says it.
    HoldKey,
    /// The hands-free chord went down, or its key repeats while held.
    HandsFreePressed,
    /// The hands-free chord's key came up, whatever modifiers are still down.
    HandsFreeReleased,
    OtherKeyPressed,
    Ignored,
}

pub(crate) fn classify(
    event: &KeyEvent,
    hold_key: HoldKey,
    hands_free: Option<Chord>,
) -> KeyChange {
    let is_hold_key = match (hold_key.modifier(), hold_key.key()) {
        (Some(modifier), _) => event.changed_modifier == Some(modifier),
        (None, Some(key)) => event.changed_modifier.is_none() && event.key == Some(key),
        (None, None) => false,
    };
    if is_hold_key {
        return KeyChange::HoldKey;
    }
    if let Some(chord) = hands_free
        && event.changed_modifier.is_none()
        && event.key == Some(chord.key)
    {
        if !event.is_key_down {
            return KeyChange::HandsFreeReleased;
        }
        if chord.modifiers.matches(event.modifiers) {
            return KeyChange::HandsFreePressed;
        }
    }
    // Caps Lock reports the lock's new state rather than the key's, so turning it off arrives as a key-up.
    if event.is_key_down || event.key == Some(Key::CapsLock) {
        KeyChange::OtherKeyPressed
    } else {
        KeyChange::Ignored
    }
}

/// Finding the key the user presses to choose the hold key: a modifier pressed and let go with no other
/// key pressed meanwhile, or a function key as it goes down. fn held beside a key does not count against
/// it, since a laptop needs fn for a function key. Esc gives up.
#[derive(Debug, Default)]
pub(crate) struct Capture {
    /// The modifier down alone so far.
    lone: Option<HoldKey>,
}

/// How a capture ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Captured {
    Key(HoldKey),
    /// Esc was pressed.
    GaveUp,
}

impl Capture {
    pub(crate) fn on_key_event(&mut self, event: &KeyEvent) -> Option<Captured> {
        match (event.changed_modifier, event.key) {
            (Some(modifier), _) if event.is_key_down => {
                let others = event
                    .modifiers
                    .difference(modifier)
                    .difference(Modifiers::FN);
                self.lone = others
                    .is_empty()
                    .then(|| HoldKey::of_modifier(modifier))
                    .flatten();
                None
            }
            (Some(modifier), _) => {
                let lone = self.lone.take()?;
                (HoldKey::of_modifier(modifier) == Some(lone)).then_some(Captured::Key(lone))
            }
            (None, Some(Key::Escape)) if event.is_key_down => Some(Captured::GaveUp),
            (None, Some(key)) if event.is_key_down => {
                self.lone = None;
                HoldKey::of_key(key).map(Captured::Key)
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn modifier_event(changed: Modifiers, held: Modifiers, is_key_down: bool) -> KeyEvent {
        KeyEvent {
            modifiers: held,
            key: None,
            is_key_down,
            changed_modifier: Some(changed),
        }
    }

    fn key_event(key: Key, held: Modifiers, is_key_down: bool) -> KeyEvent {
        KeyEvent {
            modifiers: held,
            key: Some(key),
            is_key_down,
            changed_modifier: None,
        }
    }

    #[test]
    fn right_command_down_and_up_are_hold_key_changes() {
        let down = modifier_event(Modifiers::CMD_RIGHT, Modifiers::CMD_RIGHT, true);
        let up = modifier_event(Modifiers::CMD_RIGHT, Modifiers::empty(), false);

        assert_eq!(
            classify(&down, HoldKey::RightCommand, HANDS_FREE),
            KeyChange::HoldKey
        );
        assert_eq!(
            classify(&up, HoldKey::RightCommand, HANDS_FREE),
            KeyChange::HoldKey
        );
    }

    #[test]
    fn left_command_is_another_key() {
        let down = modifier_event(Modifiers::CMD_LEFT, Modifiers::CMD, true);
        let up = modifier_event(Modifiers::CMD_LEFT, Modifiers::CMD_RIGHT, false);

        assert_eq!(
            classify(&down, HoldKey::RightCommand, HANDS_FREE),
            KeyChange::OtherKeyPressed
        );
        assert_eq!(
            classify(&up, HoldKey::RightCommand, HANDS_FREE),
            KeyChange::Ignored
        );
    }

    #[test]
    fn a_letter_with_the_hold_key_down_is_another_key() {
        let c_down = key_event(Key::C, Modifiers::CMD_RIGHT, true);
        let c_up = key_event(Key::C, Modifiers::CMD_RIGHT, false);

        assert_eq!(
            classify(&c_down, HoldKey::RightCommand, HANDS_FREE),
            KeyChange::OtherKeyPressed
        );
        assert_eq!(
            classify(&c_up, HoldKey::RightCommand, HANDS_FREE),
            KeyChange::Ignored
        );
    }

    #[test]
    fn other_modifiers_and_fn_going_down_are_other_keys() {
        for changed in [
            Modifiers::SHIFT_LEFT,
            Modifiers::SHIFT_RIGHT,
            Modifiers::CTRL_LEFT,
            Modifiers::OPT_RIGHT,
            Modifiers::FN,
        ] {
            let down = modifier_event(changed, changed | Modifiers::CMD_RIGHT, true);
            assert_eq!(
                classify(&down, HoldKey::RightCommand, HANDS_FREE),
                KeyChange::OtherKeyPressed,
                "{changed}"
            );
        }
    }

    #[test]
    fn a_mouse_button_with_the_hold_key_down_is_another_key() {
        let click = key_event(Key::MouseLeft, Modifiers::CMD_RIGHT, true);

        assert_eq!(
            classify(&click, HoldKey::RightCommand, HANDS_FREE),
            KeyChange::OtherKeyPressed
        );
    }

    #[test]
    fn caps_lock_counts_as_pressed_either_way() {
        let on = key_event(Key::CapsLock, Modifiers::CMD_RIGHT, true);
        let off = key_event(Key::CapsLock, Modifiers::CMD_RIGHT, false);

        assert_eq!(
            classify(&on, HoldKey::RightCommand, HANDS_FREE),
            KeyChange::OtherKeyPressed
        );
        assert_eq!(
            classify(&off, HoldKey::RightCommand, HANDS_FREE),
            KeyChange::OtherKeyPressed
        );
    }

    #[test]
    fn right_control_as_hold_key_makes_right_command_another_key() {
        let ctrl = modifier_event(Modifiers::CTRL_RIGHT, Modifiers::CTRL_RIGHT, true);
        let cmd = modifier_event(Modifiers::CMD_RIGHT, Modifiers::CMD_RIGHT, true);

        assert_eq!(
            classify(&ctrl, HoldKey::RightControl, HANDS_FREE),
            KeyChange::HoldKey
        );
        assert_eq!(
            classify(&cmd, HoldKey::RightControl, HANDS_FREE),
            KeyChange::OtherKeyPressed
        );
    }

    #[test]
    fn control_command_x_is_the_hands_free_chord_with_either_side_of_each() {
        let chord = Some(CONTROL_COMMAND_X);
        for held in [
            Modifiers::CTRL_LEFT | Modifiers::CMD_LEFT,
            Modifiers::CTRL_LEFT | Modifiers::CMD_RIGHT,
            Modifiers::CTRL_RIGHT | Modifiers::CMD_RIGHT,
        ] {
            let x = key_event(Key::X, held, true);
            assert_eq!(
                classify(&x, HoldKey::RightCommand, chord),
                KeyChange::HandsFreePressed,
                "{held}"
            );
        }
    }

    #[test]
    fn x_without_exactly_control_and_command_is_another_key() {
        let chord = Some(CONTROL_COMMAND_X);
        for held in [
            Modifiers::empty(),
            Modifiers::CMD_RIGHT,
            Modifiers::CTRL_LEFT,
            Modifiers::CTRL_LEFT | Modifiers::CMD_LEFT | Modifiers::SHIFT_LEFT,
            Modifiers::CTRL_LEFT | Modifiers::CMD_LEFT | Modifiers::OPT_LEFT,
        ] {
            let x = key_event(Key::X, held, true);
            assert_eq!(
                classify(&x, HoldKey::RightCommand, chord),
                KeyChange::OtherKeyPressed,
                "{held}"
            );
        }
        let c = key_event(Key::C, Modifiers::CTRL_LEFT | Modifiers::CMD_LEFT, true);
        assert_eq!(
            classify(&c, HoldKey::RightCommand, chord),
            KeyChange::OtherKeyPressed
        );
    }

    #[test]
    fn x_coming_up_releases_the_chord_whatever_is_still_held() {
        let chord = Some(CONTROL_COMMAND_X);
        for held in [
            Modifiers::empty(),
            Modifiers::CTRL_LEFT | Modifiers::CMD_LEFT,
        ] {
            let up = key_event(Key::X, held, false);
            assert_eq!(
                classify(&up, HoldKey::RightCommand, chord),
                KeyChange::HandsFreeReleased
            );
        }
        let other_up = key_event(Key::C, Modifiers::empty(), false);
        assert_eq!(
            classify(&other_up, HoldKey::RightCommand, chord),
            KeyChange::Ignored
        );
    }

    #[test]
    fn without_a_chord_x_is_only_another_key() {
        let x = key_event(Key::X, Modifiers::CTRL_LEFT | Modifiers::CMD_LEFT, true);
        assert_eq!(
            classify(&x, HoldKey::RightCommand, None),
            KeyChange::OtherKeyPressed
        );
        let up = key_event(Key::X, Modifiers::empty(), false);
        assert_eq!(
            classify(&up, HoldKey::RightCommand, None),
            KeyChange::Ignored
        );
    }

    #[test]
    fn the_hotkey_kept_from_the_app_in_front_matches_the_chord() {
        let hotkey = CONTROL_COMMAND_X.hotkey();
        assert_eq!(hotkey.key, Some(Key::X));
        assert!(
            hotkey
                .modifiers
                .matches(Modifiers::CTRL_RIGHT | Modifiers::CMD_LEFT)
        );
        assert!(!hotkey.modifiers.matches(Modifiers::CMD_LEFT));
        #[cfg(any(target_os = "macos", windows))]
        assert_eq!(HANDS_FREE, Some(CONTROL_COMMAND_X));
    }

    #[test]
    fn the_platform_default_is_the_settled_key() {
        #[cfg(target_os = "macos")]
        assert_eq!(HoldKey::PLATFORM_DEFAULT, HoldKey::RightCommand);
        #[cfg(not(target_os = "macos"))]
        assert_eq!(HoldKey::PLATFORM_DEFAULT, HoldKey::RightControl);
    }

    #[test]
    fn every_hold_key_classifies_its_own_events_and_not_its_twins() {
        let modifiers = [
            (
                HoldKey::RightCommand,
                Modifiers::CMD_RIGHT,
                Some(Modifiers::CMD_LEFT),
            ),
            (
                HoldKey::LeftCommand,
                Modifiers::CMD_LEFT,
                Some(Modifiers::CMD_RIGHT),
            ),
            (
                HoldKey::RightControl,
                Modifiers::CTRL_RIGHT,
                Some(Modifiers::CTRL_LEFT),
            ),
            (
                HoldKey::LeftControl,
                Modifiers::CTRL_LEFT,
                Some(Modifiers::CTRL_RIGHT),
            ),
            (
                HoldKey::RightShift,
                Modifiers::SHIFT_RIGHT,
                Some(Modifiers::SHIFT_LEFT),
            ),
            (
                HoldKey::LeftShift,
                Modifiers::SHIFT_LEFT,
                Some(Modifiers::SHIFT_RIGHT),
            ),
            (
                HoldKey::RightOption,
                Modifiers::OPT_RIGHT,
                Some(Modifiers::OPT_LEFT),
            ),
            (
                HoldKey::LeftOption,
                Modifiers::OPT_LEFT,
                Some(Modifiers::OPT_RIGHT),
            ),
            (HoldKey::Fn, Modifiers::FN, None),
        ];
        for (key, own, twin) in modifiers {
            assert_eq!(key.modifier(), Some(own), "{key:?}");
            assert_eq!(key.twin(), twin, "{key:?}");
            assert_eq!(key.key(), None, "{key:?}");
            assert_eq!(HoldKey::of_modifier(own), Some(key));
            for down in [true, false] {
                let event = modifier_event(own, if down { own } else { Modifiers::empty() }, down);
                assert_eq!(
                    classify(&event, key, HANDS_FREE),
                    KeyChange::HoldKey,
                    "{key:?}"
                );
            }
            if let Some(twin) = twin {
                let event = modifier_event(twin, twin, true);
                assert_eq!(
                    classify(&event, key, HANDS_FREE),
                    KeyChange::OtherKeyPressed,
                    "{key:?}'s twin"
                );
            }
            let f13 = key_event(Key::F13, own, true);
            assert_eq!(classify(&f13, key, HANDS_FREE), KeyChange::OtherKeyPressed);
        }
    }

    #[test]
    fn a_function_key_is_the_hold_key_down_up_and_repeating_and_no_other_is() {
        for number in 1..=19 {
            let key = HoldKey::Function(number);
            let function = key.key().unwrap();
            assert_eq!(function.to_string(), format!("F{number}"));
            assert_eq!(HoldKey::of_key(function), Some(key));
            assert_eq!(key.modifier(), None);
            for (held, down) in [
                (Modifiers::empty(), true),
                (Modifiers::empty(), false),
                (Modifiers::FN, true),
                (Modifiers::FN, false),
            ] {
                let event = key_event(function, held, down);
                assert_eq!(
                    classify(&event, key, HANDS_FREE),
                    KeyChange::HoldKey,
                    "F{number}"
                );
            }
        }
        let f13 = HoldKey::Function(13);
        assert_eq!(
            classify(
                &key_event(Key::F14, Modifiers::empty(), true),
                f13,
                HANDS_FREE
            ),
            KeyChange::OtherKeyPressed
        );
        assert_eq!(
            classify(
                &modifier_event(Modifiers::CMD_RIGHT, Modifiers::CMD_RIGHT, true),
                f13,
                HANDS_FREE
            ),
            KeyChange::OtherKeyPressed,
            "a modifier pressed while it is held"
        );
        assert_eq!(HoldKey::Function(0).key(), None);
        assert_eq!(HoldKey::Function(20).key(), None);
        assert_eq!(HoldKey::of_key(Key::F20), None);
    }

    #[test]
    fn a_function_key_is_kept_from_the_app_in_front_and_a_modifier_is_not() {
        let kept = HoldKey::Function(5).kept_from_apps();
        assert_eq!(kept.len(), 2);
        assert!(kept.iter().all(|hotkey| hotkey.key == Some(Key::F5)));
        assert!(
            kept.iter()
                .any(|hotkey| hotkey.modifiers.matches(Modifiers::FN))
        );
        assert!(
            kept.iter()
                .any(|hotkey| hotkey.modifiers.matches(Modifiers::empty()))
        );
        assert!(HoldKey::RightCommand.kept_from_apps().is_empty());
        assert!(HoldKey::Fn.kept_from_apps().is_empty());
    }

    fn captured(events: &[KeyEvent]) -> Vec<Captured> {
        let mut capture = Capture::default();
        events
            .iter()
            .filter_map(|event| capture.on_key_event(event))
            .collect()
    }

    #[test]
    fn a_modifier_pressed_and_let_go_alone_is_captured_by_side() {
        for (modifier, key) in MODIFIERS {
            let events = [
                modifier_event(modifier, modifier, true),
                modifier_event(modifier, Modifiers::empty(), false),
            ];
            assert_eq!(captured(&events), [Captured::Key(key)], "{key:?}");
        }
        let with_fn = [
            modifier_event(Modifiers::FN, Modifiers::FN, true),
            modifier_event(
                Modifiers::OPT_LEFT,
                Modifiers::FN | Modifiers::OPT_LEFT,
                true,
            ),
            modifier_event(Modifiers::OPT_LEFT, Modifiers::FN, false),
        ];
        assert_eq!(
            captured(&with_fn),
            [Captured::Key(HoldKey::LeftOption)],
            "fn held beside it does not count"
        );
    }

    #[test]
    fn a_shortcut_or_two_modifiers_capture_nothing() {
        let shortcut = [
            modifier_event(Modifiers::CMD_RIGHT, Modifiers::CMD_RIGHT, true),
            key_event(Key::C, Modifiers::CMD_RIGHT, true),
            key_event(Key::C, Modifiers::CMD_RIGHT, false),
            modifier_event(Modifiers::CMD_RIGHT, Modifiers::empty(), false),
        ];
        assert_eq!(captured(&shortcut), []);
        let two = [
            modifier_event(Modifiers::SHIFT_LEFT, Modifiers::SHIFT_LEFT, true),
            modifier_event(
                Modifiers::CTRL_RIGHT,
                Modifiers::SHIFT_LEFT | Modifiers::CTRL_RIGHT,
                true,
            ),
            modifier_event(Modifiers::CTRL_RIGHT, Modifiers::SHIFT_LEFT, false),
            modifier_event(Modifiers::SHIFT_LEFT, Modifiers::empty(), false),
        ];
        assert_eq!(captured(&two), []);
    }

    #[test]
    fn a_function_key_is_captured_as_it_goes_down_and_esc_gives_up() {
        assert_eq!(
            captured(&[key_event(Key::F13, Modifiers::empty(), true)]),
            [Captured::Key(HoldKey::Function(13))]
        );
        assert_eq!(
            captured(&[
                modifier_event(Modifiers::FN, Modifiers::FN, true),
                key_event(Key::F5, Modifiers::FN, true),
            ]),
            [Captured::Key(HoldKey::Function(5))],
            "a laptop's F5 comes with fn"
        );
        assert_eq!(
            captured(&[key_event(Key::Escape, Modifiers::empty(), true)]),
            [Captured::GaveUp]
        );
        assert_eq!(
            captured(&[
                key_event(Key::F20, Modifiers::empty(), true),
                key_event(Key::A, Modifiers::empty(), true)
            ]),
            [],
            "a key that cannot be the hold key"
        );
    }
}
