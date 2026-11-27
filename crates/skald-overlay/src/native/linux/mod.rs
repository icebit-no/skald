//! The overlay on Linux. On Wayland it is a layer-shell surface on the overlay layer, anchored to the bottom
//! of the output the compositor chooses, the one with the focus; it takes no keyboard and lets every click
//! through. On X11 it is GPUI's pop-up window, which takes no focus. The accent colour and reduced motion
//! come from the desktop's Settings portal (`appearance`); the system's material is not drawn here.

mod appearance;

use gpui_kit::layer_shell::{Anchor, KeyboardInteractivity, Layer, LayerShellOptions};
use gpui_kit::{Rgba, Window, WindowKind, px};

use super::PillFrame;

/// The name compositors see the surface by, as in Hyprland's `layerrule`.
const NAMESPACE: &str = "skald";

pub(crate) fn let_clicks_through(window: &Window) {
    window.set_input_region(Some(&[]));
}

pub(crate) fn active_display() -> Option<u64> {
    None
}

pub(crate) fn accent_color() -> Option<Rgba> {
    appearance::current().accent
}

pub(crate) fn prefers_reduced_motion() -> bool {
    appearance::current().reduced_motion
}

pub(crate) fn shape_material(_: &Window, _: PillFrame) {}

/// The overlay's kind of window: on Wayland a layer surface with `bottom` between the window's lower edge
/// and the bottom of the output's area that other surfaces leave free, on X11 a pop-up.
pub(crate) fn window_kind(bottom: f32) -> WindowKind {
    // GPUI takes Wayland when the session names a Wayland display.
    if std::env::var_os("WAYLAND_DISPLAY").is_none() {
        return WindowKind::PopUp;
    }
    WindowKind::LayerShell(LayerShellOptions {
        namespace: NAMESPACE.into(),
        layer: Layer::Overlay,
        anchor: Anchor::BOTTOM,
        margin: Some((px(0.0), px(0.0), px(bottom), px(0.0))),
        keyboard_interactivity: KeyboardInteractivity::None,
        ..LayerShellOptions::default()
    })
}
