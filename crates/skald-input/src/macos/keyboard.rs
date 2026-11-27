//! What the keyboard hardware holds down, and the paste keystroke.

use objc2_core_graphics::{
    CGEvent, CGEventFlags, CGEventSource, CGEventSourceStateID, CGEventTapLocation, CGKeyCode,
};

use crate::hold::KeySample;
use crate::hotkey::HoldKey;

const RIGHT_COMMAND: CGKeyCode = 0x36;
const LEFT_COMMAND: CGKeyCode = 0x37;
const RIGHT_CONTROL: CGKeyCode = 0x3E;
const LEFT_CONTROL: CGKeyCode = 0x3B;
/// The V key's position on a QWERTY keyboard, the Norwegian layout included.
const V: CGKeyCode = 0x09;

/// Set beside the Command flag by a real left ⌘ press; some apps look at it.
const LEFT_COMMAND_DEVICE_FLAG: u64 = 0x08;

/// The keyboard hardware's own state, which no missed or synthetic event can disturb.
const HARDWARE: CGEventSourceStateID = CGEventSourceStateID::HIDSystemState;

pub(crate) fn sample(hold_key: HoldKey) -> KeySample {
    let (hold, twin, other_groups) = match hold_key {
        HoldKey::RightCommand => (
            RIGHT_COMMAND,
            LEFT_COMMAND,
            CGEventFlags::MaskShift | CGEventFlags::MaskControl | CGEventFlags::MaskAlternate,
        ),
        HoldKey::RightControl => (
            RIGHT_CONTROL,
            LEFT_CONTROL,
            CGEventFlags::MaskShift | CGEventFlags::MaskCommand | CGEventFlags::MaskAlternate,
        ),
    };
    KeySample {
        hold_key_down: CGEventSource::key_state(HARDWARE, hold),
        other_modifiers_down: CGEventSource::flags_state(HARDWARE).intersects(other_groups)
            || CGEventSource::key_state(HARDWARE, twin),
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
