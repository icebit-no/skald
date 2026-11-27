//! What the keyboard holds down, and the paste keystroke.

use std::mem::size_of;

use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, MAPVK_VK_TO_VSC,
    MapVirtualKeyW, SendInput, VIRTUAL_KEY, VK_LCONTROL, VK_LMENU, VK_LSHIFT, VK_LWIN, VK_RCONTROL,
    VK_RMENU, VK_RSHIFT, VK_RWIN, VK_V,
};

use crate::hold::KeySample;
use crate::hotkey::HoldKey;

const SHIFT: [VIRTUAL_KEY; 2] = [VK_LSHIFT, VK_RSHIFT];
const CONTROL: [VIRTUAL_KEY; 2] = [VK_LCONTROL, VK_RCONTROL];
const ALT: [VIRTUAL_KEY; 2] = [VK_LMENU, VK_RMENU];
const WINDOWS: [VIRTUAL_KEY; 2] = [VK_LWIN, VK_RWIN];

/// The keyboard's own state, as the system keeps it whatever the hook was told. AltGr holds left Ctrl
/// down beside right Alt, so it counts as another modifier while right Ctrl is held.
pub(crate) fn sample(hold_key: HoldKey) -> KeySample {
    let (hold, twin, others) = match hold_key {
        HoldKey::RightControl => (VK_RCONTROL, VK_LCONTROL, [SHIFT, ALT, WINDOWS]),
        // The Windows key stands where ⌘ does.
        HoldKey::RightCommand => (VK_RWIN, VK_LWIN, [SHIFT, CONTROL, ALT]),
    };
    KeySample {
        hold_key_down: is_down(hold),
        other_modifiers_down: is_down(twin) || others.iter().flatten().any(|key| is_down(*key)),
    }
}

/// Whether Shift, Ctrl, Alt or the Windows key is down, on either side.
pub(crate) fn modifiers_down() -> bool {
    [SHIFT, CONTROL, ALT, WINDOWS]
        .iter()
        .flatten()
        .any(|key| is_down(*key))
}

fn is_down(key: VIRTUAL_KEY) -> bool {
    // Safety: takes any key code and only reads the key state.
    let state = unsafe { GetAsyncKeyState(i32::from(key)) };
    // The high bit says the key is down now; the low bit is about earlier presses.
    state < 0
}

/// Sends Ctrl+V to the focused window: left Ctrl down, V down and up, left Ctrl up, in one call so no
/// other input comes between them. Apps read the shortcut by its virtual key, so it is Ctrl+V on every
/// layout.
///
/// The system drops the keystrokes without saying so when the window runs with more rights than Skald,
/// so a true answer means only that they were sent.
pub(crate) fn send_paste() -> bool {
    let inputs = [
        key(VK_LCONTROL, false),
        key(VK_V, false),
        key(VK_V, true),
        key(VK_LCONTROL, true),
    ];
    // Safety: a valid array of keyboard inputs, with its length and the size of one input.
    let sent = unsafe {
        SendInput(
            inputs.len() as u32,
            inputs.as_ptr(),
            size_of::<INPUT>() as i32,
        )
    };
    sent as usize == inputs.len()
}

fn key(key: VIRTUAL_KEY, up: bool) -> INPUT {
    // Safety: maps a key code to its scan code; nothing is read or written through pointers.
    let scan = unsafe { MapVirtualKeyW(u32::from(key), MAPVK_VK_TO_VSC) };
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: key,
                wScan: scan as u16,
                dwFlags: if up { KEYEVENTF_KEYUP } else { 0 },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}
