//! What Skald's windows share: the identity's colours in light and dark mode, a text field's colours in
//! them, and gpui-kit set up once.
//!
//! The overlay has themes of its own (D-24) and takes neither.

mod palette;

use gpui_kit::base::input::InputEditorStyle;
use gpui_kit::{App, Global};

pub use palette::{Palette, with_alpha};

struct Initialized;

impl Global for Initialized {}

/// Sets gpui-kit up the first time a window asks: its text fields' keys for the whole app, and the
/// system's reduced motion for its animations. Later calls do nothing.
pub fn init(cx: &mut App) {
    if !cx.has_global::<Initialized>() {
        gpui_kit::init(cx);
        cx.set_global(Initialized);
    }
}

/// A text field's own colours in the palette: its text, placeholder, selection and caret.
pub fn input_style(palette: &Palette) -> InputEditorStyle {
    InputEditorStyle {
        foreground: palette.text,
        muted_foreground: palette.faint,
        selection: palette.selection,
        caret: palette.gold,
        ..InputEditorStyle::default()
    }
}
