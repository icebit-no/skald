//! macOS: key state and the paste keystroke through CoreGraphics, the clipboard through NSPasteboard, and
//! the Accessibility permission.

pub(crate) mod access;
pub(crate) mod desktop;
pub(crate) mod keyboard;
mod pasteboard;
