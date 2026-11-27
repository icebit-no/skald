//! The hold-to-talk key and how handy-keys' key events read for it.

use handy_keys::{Key, KeyEvent, Modifiers};

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

/// What a key event means for a hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum KeyChange {
    /// The hold key went down or up. Which one is read from the operating system, because the press flag
    /// handy-keys reports for a modifier can be inverted after a missed event.
    HoldKey,
    OtherKeyPressed,
    Ignored,
}

pub(crate) fn classify(event: &KeyEvent, hold_key: HoldKey) -> KeyChange {
    if event.changed_modifier == Some(hold_key.modifier()) {
        return KeyChange::HoldKey;
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

        assert_eq!(classify(&down, HoldKey::RightCommand), KeyChange::HoldKey);
        assert_eq!(classify(&up, HoldKey::RightCommand), KeyChange::HoldKey);
    }

    #[test]
    fn left_command_is_another_key() {
        let down = modifier_event(Modifiers::CMD_LEFT, Modifiers::CMD, true);
        let up = modifier_event(Modifiers::CMD_LEFT, Modifiers::CMD_RIGHT, false);

        assert_eq!(
            classify(&down, HoldKey::RightCommand),
            KeyChange::OtherKeyPressed
        );
        assert_eq!(classify(&up, HoldKey::RightCommand), KeyChange::Ignored);
    }

    #[test]
    fn a_letter_with_the_hold_key_down_is_another_key() {
        let c_down = key_event(Key::C, Modifiers::CMD_RIGHT, true);
        let c_up = key_event(Key::C, Modifiers::CMD_RIGHT, false);

        assert_eq!(
            classify(&c_down, HoldKey::RightCommand),
            KeyChange::OtherKeyPressed
        );
        assert_eq!(classify(&c_up, HoldKey::RightCommand), KeyChange::Ignored);
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
                classify(&down, HoldKey::RightCommand),
                KeyChange::OtherKeyPressed,
                "{changed}"
            );
        }
    }

    #[test]
    fn a_mouse_button_with_the_hold_key_down_is_another_key() {
        let click = key_event(Key::MouseLeft, Modifiers::CMD_RIGHT, true);

        assert_eq!(
            classify(&click, HoldKey::RightCommand),
            KeyChange::OtherKeyPressed
        );
    }

    #[test]
    fn caps_lock_counts_as_pressed_either_way() {
        let on = key_event(Key::CapsLock, Modifiers::CMD_RIGHT, true);
        let off = key_event(Key::CapsLock, Modifiers::CMD_RIGHT, false);

        assert_eq!(
            classify(&on, HoldKey::RightCommand),
            KeyChange::OtherKeyPressed
        );
        assert_eq!(
            classify(&off, HoldKey::RightCommand),
            KeyChange::OtherKeyPressed
        );
    }

    #[test]
    fn right_control_as_hold_key_makes_right_command_another_key() {
        let ctrl = modifier_event(Modifiers::CTRL_RIGHT, Modifiers::CTRL_RIGHT, true);
        let cmd = modifier_event(Modifiers::CMD_RIGHT, Modifiers::CMD_RIGHT, true);

        assert_eq!(classify(&ctrl, HoldKey::RightControl), KeyChange::HoldKey);
        assert_eq!(
            classify(&cmd, HoldKey::RightControl),
            KeyChange::OtherKeyPressed
        );
    }

    #[test]
    fn the_platform_default_is_the_settled_key() {
        #[cfg(target_os = "macos")]
        assert_eq!(HoldKey::PLATFORM_DEFAULT, HoldKey::RightCommand);
        #[cfg(not(target_os = "macos"))]
        assert_eq!(HoldKey::PLATFORM_DEFAULT, HoldKey::RightControl);
    }
}
