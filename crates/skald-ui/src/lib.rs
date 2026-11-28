//! What Skald's windows share: the identity's colours in light and dark mode, a text field's colours in
//! them, gpui-kit set up once, the name the system knows the windows by, and bringing a window to the
//! front.
//!
//! The overlay has themes of its own (D-24) and takes neither.

mod palette;

use gpui_kit::base::input::InputEditorStyle;
use gpui_kit::{App, Global, Window};

pub use palette::{Palette, with_alpha};

/// The name Skald's windows give the system, the bundle identifier on macOS (D-34); a Wayland compositor
/// matches its window rules against it.
pub const APP_ID: &str = "app.icebit.skald";

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

/// Brings the window to the front as the key window and makes Skald the active app. Skald has no Dock
/// icon and is not in the app switcher, so a window left behind another app's is hard to find again. On
/// macOS the window is also put above the app in front by itself, so it shows even when the system does not
/// let Skald become the active app.
pub fn bring_to_front(window: &Window, cx: &App) {
    #[cfg(target_os = "macos")]
    macos::order_front(window);
    cx.activate(true);
    window.activate_window();
}

#[cfg(target_os = "macos")]
mod macos {
    use gpui_kit::Window;
    use objc2_app_kit::NSView;
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    pub(super) fn order_front(window: &Window) {
        // GPUI's own `window_handle` names the window in GPUI; this one is the platform's.
        let Ok(handle) = HasWindowHandle::window_handle(window) else {
            return;
        };
        let RawWindowHandle::AppKit(appkit) = handle.as_raw() else {
            return;
        };
        // SAFETY: GPUI's AppKit handle points at the live content view of this window, on the main
        // thread, for as long as the window is borrowed.
        let view: &NSView = unsafe { appkit.ns_view.cast::<NSView>().as_ref() };
        if let Some(ns_window) = view.window() {
            ns_window.orderFrontRegardless();
        }
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
