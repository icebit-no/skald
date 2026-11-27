//! Windows: key state and the paste keystroke through the Win32 keyboard functions, the clipboard with
//! every format it holds, and the integrity level of the window in front, which decides whether it takes
//! Skald's keystrokes.

pub(crate) mod access;
mod clipboard;
pub(crate) mod desktop;
mod elevation;
pub(crate) mod keyboard;
