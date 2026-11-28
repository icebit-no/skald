//! What the keyboard hardware holds down, and the paste keystroke.

use handy_keys::Modifiers;
use objc2_core_graphics::{
    CGEvent, CGEventFlags, CGEventSource, CGEventSourceStateID, CGEventTapLocation, CGKeyCode,
};

use crate::hold::KeySample;
use crate::hotkey::HoldKey;

/// The V key's position on a QWERTY keyboard, the Norwegian layout included.
const V: CGKeyCode = 0x09;

/// Set beside the Command flag by a real left ⌘ press; some apps look at it.
const LEFT_COMMAND_DEVICE_FLAG: u64 = 0x08;

/// The keyboard hardware's own state, which no missed or synthetic event can disturb.
const HARDWARE: CGEventSourceStateID = CGEventSourceStateID::HIDSystemState;

/// The hold key and the other modifiers now. `held` is the modifiers by side as the key events last had
/// them: handy-keys names each from its event's key code, 54 for right ⌘ and 55 for left. The hardware's
/// state per key cannot tell the sides apart on every keyboard; an Apple Magic Keyboard over Bluetooth
/// has right ⌘ held read as left ⌘ held and right ⌘ up. So the side comes from the events, and the
/// hardware's modifier flags, one per kind and the same for both sides, say whether that kind is down at
/// all, which also ends a take whose release the events missed.
pub(crate) fn sample(hold_key: HoldKey, held: Modifiers) -> KeySample {
    sample_from(hold_key, held, CGEventSource::flags_state(HARDWARE))
}

fn sample_from(hold_key: HoldKey, held: Modifiers, flags: CGEventFlags) -> KeySample {
    let (kind, other_kinds) = match hold_key {
        HoldKey::RightCommand => (
            CGEventFlags::MaskCommand,
            CGEventFlags::MaskShift | CGEventFlags::MaskControl | CGEventFlags::MaskAlternate,
        ),
        HoldKey::RightControl => (
            CGEventFlags::MaskControl,
            CGEventFlags::MaskShift | CGEventFlags::MaskCommand | CGEventFlags::MaskAlternate,
        ),
    };
    let kind_down = flags.contains(kind);
    let twin_down = kind_down && held.contains(hold_key.twin());
    KeySample {
        // The kind is down and it is not the left key alone: the events may have the hold key's own
        // press the wrong way round after one was missed, but not which key the press came from.
        hold_key_down: kind_down && (held.contains(hold_key.modifier()) || !twin_down),
        other_modifiers_down: flags.intersects(other_kinds) || twin_down,
    }
}

/// Whether Shift, Control, Option or Command is down, on either side.
pub(crate) fn modifiers_down() -> bool {
    CGEventSource::flags_state(HARDWARE).intersects(
        CGEventFlags::MaskShift
            | CGEventFlags::MaskControl
            | CGEventFlags::MaskAlternate
            | CGEventFlags::MaskCommand,
    )
}

/// Sends ⌘V to the focused app: V down and up carrying the Command flag, with no ⌘ key events of its own,
/// so the system's record of which modifiers are down is never touched.
pub(crate) fn send_paste() -> bool {
    let source = CGEventSource::new(CGEventSourceStateID::CombinedSessionState);
    // Posting can otherwise hold back the user's own keystrokes for a quarter of a second.
    CGEventSource::set_local_events_suppression_interval(source.as_deref(), 0.0);
    let flags =
        CGEventFlags::MaskCommand | CGEventFlags::from_bits_retain(LEFT_COMMAND_DEVICE_FLAG);
    let (Some(down), Some(up)) = (
        CGEvent::new_keyboard_event(source.as_deref(), V, true),
        CGEvent::new_keyboard_event(source.as_deref(), V, false),
    ) else {
        return false;
    };
    for event in [&down, &up] {
        CGEvent::set_flags(Some(event), flags);
        CGEvent::post(CGEventTapLocation::HIDEventTap, Some(event));
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The flags an Apple Magic Keyboard over Bluetooth sends, as a probe read them from the events:
    /// the Command flag, the side's device bit and the non-coalesced bit.
    const RIGHT_COMMAND_DOWN: u64 = 0x0010_0110;
    const LEFT_COMMAND_DOWN: u64 = 0x0010_0108;
    const NOTHING_DOWN: u64 = 0x0000_0100;
    const RIGHT_CONTROL_DOWN: u64 = 0x0004_2100;
    const LEFT_CONTROL_DOWN: u64 = 0x0004_0101;
    const SHIFT: u64 = 0x0002_0000;

    fn sample(hold_key: HoldKey, held: Modifiers, flags: u64) -> KeySample {
        sample_from(hold_key, held, CGEventFlags::from_bits_retain(flags))
    }

    const ALONE: KeySample = KeySample {
        hold_key_down: true,
        other_modifiers_down: false,
    };
    const UP: KeySample = KeySample {
        hold_key_down: false,
        other_modifiers_down: false,
    };

    #[test]
    fn right_command_held_is_the_hold_key_whatever_the_hardware_says_per_key() {
        let right = HoldKey::RightCommand;
        assert_eq!(
            sample(right, Modifiers::CMD_RIGHT, RIGHT_COMMAND_DOWN),
            ALONE
        );
        assert_eq!(sample(right, Modifiers::empty(), NOTHING_DOWN), UP);
    }

    #[test]
    fn left_command_held_is_another_modifier_not_the_hold_key() {
        assert_eq!(
            sample(
                HoldKey::RightCommand,
                Modifiers::CMD_LEFT,
                LEFT_COMMAND_DOWN
            ),
            KeySample {
                hold_key_down: false,
                other_modifiers_down: true,
            }
        );
        assert_eq!(
            sample(
                HoldKey::RightCommand,
                Modifiers::CMD,
                RIGHT_COMMAND_DOWN | LEFT_COMMAND_DOWN
            ),
            KeySample {
                hold_key_down: true,
                other_modifiers_down: true,
            },
            "both sides held"
        );
    }

    #[test]
    fn a_release_the_events_missed_is_seen_in_the_hardware_flags() {
        assert_eq!(
            sample(HoldKey::RightCommand, Modifiers::CMD_RIGHT, NOTHING_DOWN),
            UP
        );
        assert_eq!(
            sample(HoldKey::RightCommand, Modifiers::CMD_LEFT, NOTHING_DOWN),
            UP,
            "a left key the events think is down cannot spoil a hold once no ⌘ is down"
        );
    }

    #[test]
    fn a_press_the_events_have_the_wrong_way_round_still_counts() {
        // handy-keys toggles a modifier at each event, so after a missed release it reads the next
        // press as the key going up; the key code still says right ⌘.
        assert_eq!(
            sample(
                HoldKey::RightCommand,
                Modifiers::empty(),
                RIGHT_COMMAND_DOWN
            ),
            ALONE
        );
    }

    #[test]
    fn right_control_is_told_from_left_control_the_same_way() {
        let control = HoldKey::RightControl;
        assert_eq!(
            sample(control, Modifiers::CTRL_RIGHT, RIGHT_CONTROL_DOWN),
            ALONE
        );
        assert_eq!(
            sample(control, Modifiers::CTRL_LEFT, LEFT_CONTROL_DOWN),
            KeySample {
                hold_key_down: false,
                other_modifiers_down: true,
            }
        );
        assert_eq!(sample(control, Modifiers::empty(), NOTHING_DOWN), UP);
    }

    #[test]
    fn shift_or_option_with_the_hold_key_is_another_modifier() {
        assert_eq!(
            sample(
                HoldKey::RightCommand,
                Modifiers::CMD_RIGHT | Modifiers::SHIFT_LEFT,
                RIGHT_COMMAND_DOWN | SHIFT
            ),
            KeySample {
                hold_key_down: true,
                other_modifiers_down: true,
            }
        );
        assert_eq!(
            sample(
                HoldKey::RightControl,
                Modifiers::CTRL_RIGHT,
                RIGHT_CONTROL_DOWN | RIGHT_COMMAND_DOWN
            ),
            KeySample {
                hold_key_down: true,
                other_modifiers_down: true,
            },
            "⌘ beside right Control"
        );
    }
}
