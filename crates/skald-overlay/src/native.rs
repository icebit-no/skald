//! What GPUI leaves to the platform: letting clicks through the window, finding the active screen,
//! reading the accent colour and whether the user asked for reduced motion, and giving the system's
//! material the pill's shape, and on Wayland making the window a layer-shell surface. macOS and Linux have
//! theirs; Windows follows in M4, and until then these do nothing there.

/// The pill's rectangle inside its window, in points from the window's top left, and its corner radius.
#[derive(Clone, Copy, Debug)]
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) struct PillFrame {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub radius: f64,
}

#[cfg(target_os = "macos")]
pub(crate) use macos::*;

#[cfg(target_os = "linux")]
pub(crate) use linux::*;

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub(crate) use elsewhere::*;

/// The overlay's kind of window: a pop-up here. Only a Wayland compositor takes it as a layer surface.
#[cfg(not(target_os = "linux"))]
pub(crate) fn window_kind(_bottom: f32) -> gpui_kit::WindowKind {
    gpui_kit::WindowKind::PopUp
}

#[cfg(target_os = "linux")]
mod linux;

#[cfg(target_os = "macos")]
mod macos {
    use block2::RcBlock;
    use gpui_kit::{Rgba, Window};
    use objc2::MainThreadMarker;
    use objc2::rc::Retained;
    use objc2::runtime::Bool;
    use objc2_app_kit::{
        NSAutoresizingMaskOptions, NSBezierPath, NSColor, NSColorSpace, NSImage, NSScreen, NSView,
        NSVisualEffectView, NSWindow, NSWorkspace,
    };
    use objc2_foundation::{NSNumber, NSPoint, NSRect, NSSize, NSString};
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    use super::PillFrame;

    fn ns_window(window: &Window) -> Option<Retained<NSWindow>> {
        // GPUI's own `window_handle` names the window in GPUI; this one is the platform's.
        let handle = HasWindowHandle::window_handle(window).ok()?;
        let RawWindowHandle::AppKit(appkit) = handle.as_raw() else {
            return None;
        };
        // SAFETY: GPUI's AppKit handle points at the live content view of this window, on the main
        // thread, for as long as the window is borrowed.
        let view: &NSView = unsafe { appkit.ns_view.cast::<NSView>().as_ref() };
        view.window()
    }

    /// Mouse events go to whatever is under the pill, and AppKit draws no shadow of its own: the pill's
    /// shadow is drawn with it.
    pub(crate) fn let_clicks_through(window: &Window) {
        if let Some(window) = ns_window(window) {
            window.setIgnoresMouseEvents(true);
            window.setHasShadow(false);
        }
    }

    /// The screen with the window that has keyboard focus, where the text will land.
    pub(crate) fn active_display() -> Option<u64> {
        let main_thread = MainThreadMarker::new()?;
        let screen = NSScreen::mainScreen(main_thread)?;
        let number = screen
            .deviceDescription()
            .objectForKey(&NSString::from_str("NSScreenNumber"))?
            .downcast::<NSNumber>()
            .ok()?;
        Some(u64::from(number.unsignedIntValue()))
    }

    /// The accent colour chosen in System Settings, in sRGB.
    pub(crate) fn accent_color() -> Option<Rgba> {
        let accent =
            NSColor::controlAccentColor().colorUsingColorSpace(&NSColorSpace::sRGBColorSpace())?;
        Some(Rgba {
            r: accent.redComponent() as f32,
            g: accent.greenComponent() as f32,
            b: accent.blueComponent() as f32,
            a: 1.0,
        })
    }

    pub(crate) fn prefers_reduced_motion() -> bool {
        NSWorkspace::sharedWorkspace().accessibilityDisplayShouldReduceMotion()
    }

    /// The blurred background fills the whole window, a rectangle around the pill and its shadow. This
    /// sizes its visual effect view to the pill and masks it to the pill's rounded shape.
    pub(crate) fn shape_material(window: &Window, pill: PillFrame) {
        let Some(window) = ns_window(window) else {
            return;
        };
        let Some(content) = window.contentView() else {
            return;
        };
        let height = content.frame().size.height;
        for view in content.subviews().iter() {
            let Some(material) = view.downcast_ref::<NSVisualEffectView>() else {
                continue;
            };
            // AppKit's y runs up from the bottom of the window.
            let frame = NSRect::new(
                NSPoint::new(pill.x, height - pill.y - pill.height),
                NSSize::new(pill.width, pill.height),
            );
            material.setAutoresizingMask(NSAutoresizingMaskOptions::empty());
            material.setFrame(frame);
            material.setMaskImage(Some(&rounded_mask(pill)));
        }
    }

    fn rounded_mask(pill: PillFrame) -> Retained<NSImage> {
        let radius = pill.radius;
        let draw = RcBlock::new(move |rect: NSRect| -> Bool {
            NSColor::blackColor().setFill();
            NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(rect, radius, radius).fill();
            Bool::YES
        });
        NSImage::imageWithSize_flipped_drawingHandler(
            NSSize::new(pill.width, pill.height),
            false,
            &draw,
        )
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
mod elsewhere {
    use gpui_kit::{Rgba, Window};

    use super::PillFrame;

    pub(crate) fn let_clicks_through(_: &Window) {}

    pub(crate) fn active_display() -> Option<u64> {
        None
    }

    pub(crate) fn accent_color() -> Option<Rgba> {
        None
    }

    pub(crate) fn prefers_reduced_motion() -> bool {
        false
    }

    pub(crate) fn shape_material(_: &Window, _: PillFrame) {}
}
