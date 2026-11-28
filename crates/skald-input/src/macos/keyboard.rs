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

/// The modifier kinds that count as another key held with the hold key. fn does not: a laptop needs it
/// for a function key, and the system sets its flag for the arrow keys too.
const KINDS: CGEventFlags = CGEventFlags::MaskShift
    .union(CGEventFlags::MaskControl)
    .union(CGEventFlags::MaskAlternate)
    .union(CGEventFlags::MaskCommand);

/// The hold key and the other modifiers now. `held` is the modifiers by side as the key events last had
/// them: handy-keys names each from its event's key code, 54 for right ⌘ and 55 for left. The hardware's
/// state per key cannot tell the sides apart on every keyboard; an Apple Magic Keyboard over Bluetooth
/// has right ⌘ held read as left ⌘ held and right ⌘ up. So the side comes from the events, and the
/// hardware's modifier flags, one per kind and the same for both sides, say whether that kind is down at
/// all, which also ends a take whose release the events missed. A function key has no flag, so
/// `key_down`, its own events' word, decides it, and a release they miss ends the take at the cap.
pub(crate) fn sample(hold_key: HoldKey, held: Modifiers, key_down: bool) -> KeySample {
    sample_from(
        hold_key,
        held,
        key_down,
        CGEventSource::flags_state(HARDWARE),
    )
}

fn sample_from(
    hold_key: HoldKey,
    held: Modifiers,
    key_down: bool,
    flags: CGEventFlags,
) -> KeySample {
    let Some((hold, kind)) = hold_key.modifier().zip(kind(hold_key)) else {
        return KeySample {
            hold_key_down: key_down,
            other_modifiers_down: flags.intersects(KINDS),
        };
    };
    let kind_down = flags.contains(kind);
    let twin_down = kind_down && hold_key.twin().is_some_and(|twin| held.contains(twin));
    KeySample {
        // The kind is down and it is not the twin alone: the events may have the hold key's own press
        // the wrong way round after one was missed, but not which key the press came from.
        hold_key_down: kind_down && (held.contains(hold) || !twin_down),
        other_modifiers_down: flags.intersects(KINDS.difference(kind)) || twin_down,
    }
}

/// The flag the system sets while a modifier of the hold key's kind is down, either side.
fn kind(hold_key: HoldKey) -> Option<CGEventFlags> {
    match hold_key {
        HoldKey::RightCommand | HoldKey::LeftCommand => Some(CGEventFlags::MaskCommand),
        HoldKey::RightControl | HoldKey::LeftControl => Some(CGEventFlags::MaskControl),
        HoldKey::RightShift | HoldKey::LeftShift => Some(CGEventFlags::MaskShift),
        HoldKey::RightOption | HoldKey::LeftOption => Some(CGEventFlags::MaskAlternate),
        HoldKey::Fn => Some(CGEventFlags::MaskSecondaryFn),
        HoldKey::Function(_) => None,
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
        sample_from(hold_key, held, false, CGEventFlags::from_bits_retain(flags))
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

    /// The kinds' flags with each side's device bit, as the system sets them.
    const RIGHT_SHIFT_DOWN: u64 = 0x0002_0104;
    const LEFT_SHIFT_DOWN: u64 = 0x0002_0102;
    const RIGHT_OPTION_DOWN: u64 = 0x0008_0140;
    const LEFT_OPTION_DOWN: u64 = 0x0008_0120;
    const FN_DOWN: u64 = 0x0080_0100;

    #[test]
    fn every_modifier_is_told_from_its_twin_by_the_events() {
        let cases = [
            (
                HoldKey::RightShift,
                Modifiers::SHIFT_RIGHT,
                RIGHT_SHIFT_DOWN,
                Modifiers::SHIFT_LEFT,
                LEFT_SHIFT_DOWN,
            ),
            (
                HoldKey::LeftShift,
                Modifiers::SHIFT_LEFT,
                LEFT_SHIFT_DOWN,
                Modifiers::SHIFT_RIGHT,
                RIGHT_SHIFT_DOWN,
            ),
            (
                HoldKey::RightOption,
                Modifiers::OPT_RIGHT,
                RIGHT_OPTION_DOWN,
                Modifiers::OPT_LEFT,
                LEFT_OPTION_DOWN,
            ),
            (
                HoldKey::LeftOption,
                Modifiers::OPT_LEFT,
                LEFT_OPTION_DOWN,
                Modifiers::OPT_RIGHT,
                RIGHT_OPTION_DOWN,
            ),
            (
                HoldKey::LeftCommand,
                Modifiers::CMD_LEFT,
                LEFT_COMMAND_DOWN,
                Modifiers::CMD_RIGHT,
                RIGHT_COMMAND_DOWN,
            ),
            (
                HoldKey::LeftControl,
                Modifiers::CTRL_LEFT,
                LEFT_CONTROL_DOWN,
                Modifiers::CTRL_RIGHT,
                RIGHT_CONTROL_DOWN,
            ),
        ];
        for (key, own, own_flags, twin, twin_flags) in cases {
            assert_eq!(sample(key, own, own_flags), ALONE, "{key:?}");
            assert_eq!(
                sample(key, twin, twin_flags),
                KeySample {
                    hold_key_down: false,
                    other_modifiers_down: true,
                },
                "{key:?} with its twin held"
            );
            assert_eq!(sample(key, Modifiers::empty(), NOTHING_DOWN), UP, "{key:?}");
        }
    }

    #[test]
    fn fn_is_its_own_kind_and_no_other_modifier_for_the_rest() {
        assert_eq!(sample(HoldKey::Fn, Modifiers::FN, FN_DOWN), ALONE);
        assert_eq!(sample(HoldKey::Fn, Modifiers::empty(), NOTHING_DOWN), UP);
        assert_eq!(
            sample(
                HoldKey::Fn,
                Modifiers::FN | Modifiers::SHIFT_LEFT,
                FN_DOWN | SHIFT
            ),
            KeySample {
                hold_key_down: true,
                other_modifiers_down: true,
            }
        );
        assert_eq!(
            sample(
                HoldKey::RightCommand,
                Modifiers::CMD_RIGHT | Modifiers::FN,
                RIGHT_COMMAND_DOWN | FN_DOWN
            ),
            ALONE,
            "fn beside right ⌘, as the arrow keys set it"
        );
    }

    #[test]
    fn a_function_key_is_down_by_its_events_and_fn_beside_it_is_no_other_modifier() {
        let f13 = HoldKey::Function(13);
        let at = |held, key_down, flags| {
            sample_from(f13, held, key_down, CGEventFlags::from_bits_retain(flags))
        };
        assert_eq!(at(Modifiers::empty(), true, NOTHING_DOWN), ALONE);
        assert_eq!(
            at(Modifiers::FN, true, FN_DOWN),
            ALONE,
            "a laptop sends it with fn"
        );
        assert_eq!(at(Modifiers::empty(), false, NOTHING_DOWN), UP);
        assert_eq!(
            at(Modifiers::CMD_RIGHT, true, RIGHT_COMMAND_DOWN),
            KeySample {
                hold_key_down: true,
                other_modifiers_down: true,
            }
        );
    }
}
