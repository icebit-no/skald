//! The hold-to-talk key, the hands-free chord, and how handy-keys' key events read for them.

use handy_keys::{Hotkey, Key, KeyEvent, Modifiers};

/// The key held to talk. Each is a lone modifier, told apart from its twin on the other side.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HoldKey {
    RightCommand,
    RightControl,
}

impl HoldKey {
    /// Right ⌘ on macOS; right Ctrl on Windows and Linux, where right Alt is AltGr.
    pub const PLATFORM_DEFAULT: HoldKey = if cfg!(target_os = "macos") {
        HoldKey::RightCommand
    } else {
        HoldKey::RightControl
    };

    pub(crate) fn modifier(self) -> Modifiers {
        match self {
            HoldKey::RightCommand => Modifiers::CMD_RIGHT,
            HoldKey::RightControl => Modifiers::CTRL_RIGHT,
        }
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

/// The hands-free chord on this platform. Linux chooses its own with that platform, since Ctrl+Alt is AltGr
/// on Norwegian layouts.
pub(crate) const HANDS_FREE: Option<Chord> = if cfg!(any(target_os = "macos", windows)) {
    Some(CONTROL_COMMAND_X)
} else {
    None
};

/// What a key event means for a hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum KeyChange {
    /// The hold key went down or up. Which one is read from the operating system, because the press flag
    /// handy-keys reports for a modifier can be inverted after a missed event.
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
    if event.changed_modifier == Some(hold_key.modifier()) {
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
}
