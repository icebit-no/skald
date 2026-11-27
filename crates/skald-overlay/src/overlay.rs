//! The overlay the app drives: one call per thing that happens in a take, and the window that shows it.
//!
//! The window is a PopUp: on macOS a non-activating panel above other windows, so it never takes focus.
//! It opens when the pill appears and closes once it has faded out, on the active screen each time. It
//! lets clicks through and draws at the display's rate while it shows, focused or not.

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui_kit::{
    App, AppContext, Bounds, Context, DisplayId, Entity, IntoElement, ParentElement, Pixels,
    Render, RenderImage, Rgba, Styled, Window, WindowAppearance, WindowBackgroundAppearance,
    WindowBounds, WindowHandle, WindowKind, WindowOptions, canvas, div, point, px, size,
};
use skald_core::config::OverlayTheme;

use crate::draw::{self, Face};
use crate::geometry::{ABOVE_BOTTOM, H, SHADOW_ROOM, W};
use crate::native::{self, PillFrame};
use crate::paint::Painter;
use crate::pill::{Motion, Pill};
use crate::texture;
use crate::theme::{Style, System, Theme};

/// The pill near the bottom of the active screen.
pub struct Overlay {
    state: Entity<State>,
    window: Option<WindowHandle<PillView>>,
}

struct State {
    pill: Pill,
    theme: Theme,
    /// Motion asked for by the caller; otherwise the system's setting, read as the pill appears.
    motion_override: Option<Motion>,
    motion: Motion,
    accent: Option<Rgba>,
    clock: Instant,
    frames: u64,
    faces: HashMap<(texture::Face, u32), Arc<RenderImage>>,
}

impl State {
    fn now(&self) -> Duration {
        self.clock.elapsed()
    }

    /// The plate's face for the theme at a scale, rendered the first time it is needed.
    fn face(&mut self, theme: Theme, scale: f32) -> Option<Face> {
        let face = match theme {
            Theme::Runepinne => texture::Face::Oak,
            Theme::SmiddJern => texture::Face::Iron,
            Theme::Kjedelig => return None,
        };
        let key = (face, (scale * 100.0).round() as u32);
        let image = self
            .faces
            .entry(key)
            .or_insert_with(|| render_face(face, scale));
        Some(Face {
            image: image.clone(),
        })
    }
}

fn render_face(face: texture::Face, scale: f32) -> Arc<RenderImage> {
    let picture = texture::render(face, scale);
    // GPUI's images hold BGRA in the buffer `image` calls RGBA.
    let buffer = image::RgbaImage::from_raw(picture.width, picture.height, picture.bgra)
        .expect("the picture's buffer matches its size");
    Arc::new(RenderImage::new(vec![image::Frame::new(buffer)]))
}

/// The Viking themes' type, loaded into GPUI.
fn load_fonts(cx: &mut App) {
    let fonts = vec![
        Cow::Borrowed(ALEGREYA_SANS_REGULAR),
        Cow::Borrowed(ALEGREYA_SANS_MEDIUM),
    ];
    if let Err(error) = cx.text_system().add_fonts(fonts) {
        eprintln!("skald-overlay: Alegreya Sans did not load, the system font stands in: {error}");
    }
}

const ALEGREYA_SANS_REGULAR: &[u8] = include_bytes!("../assets/fonts/AlegreyaSans-Regular.ttf");
const ALEGREYA_SANS_MEDIUM: &[u8] = include_bytes!("../assets/fonts/AlegreyaSans-Medium.ttf");

impl Overlay {
    /// Loads the Viking themes' type into GPUI and starts hidden.
    pub fn new(theme: &OverlayTheme, cx: &mut App) -> Self {
        load_fonts(cx);
        let state = cx.new(|_| State {
            pill: Pill::default(),
            theme: Theme::from(theme),
            motion_override: None,
            motion: Motion::Full,
            accent: None,
            clock: Instant::now(),
            frames: 0,
            faces: HashMap::new(),
        });
        Self {
            state,
            window: None,
        }
    }

    pub fn set_theme(&mut self, theme: &OverlayTheme, cx: &mut App) {
        let theme = Theme::from(theme);
        self.state.update(cx, |state, cx| {
            state.theme = theme;
            cx.notify();
        });
        if let Some(handle) = self.window {
            let _ = handle.update(cx, |_, window, _| dress(window, theme));
        }
    }

    /// Motion regardless of the system's setting, or `None` to follow it.
    pub fn set_motion(&mut self, motion: Option<Motion>, cx: &mut App) {
        self.state
            .update(cx, |state, _| state.motion_override = motion);
    }

    /// A take started: Opptak.
    pub fn record(&mut self, cx: &mut App) {
        self.drive(cx, |pill, now| pill.record(false, now));
    }

    /// A hands-free take started: Håndfritt.
    pub fn record_hands_free(&mut self, cx: &mut App) {
        self.drive(cx, |pill, now| pill.record(true, now));
    }

    /// One level reading of the take, the RMS of 60 ms of audio in dBFS.
    pub fn level(&mut self, dbfs: f32, cx: &mut App) {
        self.drive(cx, |pill, now| pill.level(dbfs, now));
    }

    /// The key was released, or the hands-free take stopped: Rister, until the text or nothing comes.
    pub fn release(&mut self, cx: &mut App) {
        self.drive(cx, |pill, now| pill.release(now));
    }

    /// The text arrived and was inserted: the chisel runs out, then Satt inn.
    pub fn inserted(&mut self, text: &str, cx: &mut App) {
        self.drive(cx, |pill, now| pill.inserted(text, now));
    }

    /// The take ended without text: Hørte ingenting, naming the microphone it listened on.
    pub fn heard_nothing(&mut self, microphone: &str, cx: &mut App) {
        self.drive(cx, |pill, now| pill.heard_nothing(microphone, now));
    }

    /// The text could not be inserted: «Ikke satt inn · ligger i Siste tekster».
    pub fn not_inserted(&mut self, cx: &mut App) {
        self.drive(cx, |pill, now| pill.not_inserted(now));
    }

    /// The window in front runs as administrator and refused the text: «Kan ikke sette inn i et
    /// administratorvindu · ligger i Siste tekster».
    pub fn administrator_window(&mut self, cx: &mut App) {
        self.drive(cx, |pill, now| pill.administrator_window(now));
    }

    /// The model failed on the take: «Transkriberingen feilet».
    pub fn transcription_failed(&mut self, cx: &mut App) {
        self.drive(cx, |pill, now| pill.transcription_failed(now));
    }

    /// A take armed with no microphone connected: «Ingen mikrofon er koblet til».
    pub fn no_microphone(&mut self, cx: &mut App) {
        self.drive(cx, |pill, now| pill.no_microphone(now));
    }

    /// The active microphone changed: Mikrofon byttet, once, after the take on screen if there is one.
    pub fn microphone_changed(&mut self, microphone: &str, cx: &mut App) {
        self.drive(cx, |pill, now| pill.microphone_changed(microphone, now));
    }

    /// The take was cancelled: the pill fades out.
    pub fn cancel(&mut self, cx: &mut App) {
        self.drive(cx, |pill, now| pill.cancel(now));
    }

    /// How many frames the pill has drawn, to measure its frame rate.
    pub fn frames_drawn(&self, cx: &App) -> u64 {
        self.state.read(cx).frames
    }

    fn drive(&mut self, cx: &mut App, change: impl FnOnce(&mut Pill, Duration)) {
        let showing = self.state.update(cx, |state, cx| {
            let now = state.now();
            change(&mut state.pill, now);
            cx.notify();
            !state.pill.is_hidden()
        });
        if showing {
            self.open(cx);
        }
    }

    fn open(&mut self, cx: &mut App) {
        if let Some(handle) = self.window
            && handle.update(cx, |_, _, _| ()).is_ok()
        {
            return;
        }
        let theme = self.state.update(cx, |state, _| {
            state.motion = state
                .motion_override
                .unwrap_or(if native::prefers_reduced_motion() {
                    Motion::Reduced
                } else {
                    Motion::Full
                });
            state.accent = native::accent_color();
            state.theme
        });
        let (display_id, bounds) = placement(cx);
        let kind = native::window_kind(ABOVE_BOTTOM - SHADOW_ROOM);
        // A layer surface goes where the compositor puts it.
        let display_id = display_id.filter(|_| kind == WindowKind::PopUp);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: None,
            focus: false,
            show: true,
            kind,
            is_movable: false,
            is_resizable: false,
            is_minimizable: false,
            display_id,
            window_background: background(theme),
            // Without focus GPUI draws near 30 frames a second; the pill never has focus.
            inactive_frame_interval: None,
            ..WindowOptions::default()
        };
        let state = self.state.clone();
        let opened = cx.open_window(options, move |window, cx| {
            native::let_clicks_through(window);
            dress(window, theme);
            cx.new(|_| PillView { state })
        });
        match opened {
            Ok(handle) => self.window = Some(handle),
            Err(error) => eprintln!("skald-overlay: the overlay window did not open: {error}"),
        }
    }
}

/// Where the window goes: centred near the bottom of the active screen's usable area.
fn placement(cx: &App) -> (Option<DisplayId>, Bounds<Pixels>) {
    let window = size(px(W + 2.0 * SHADOW_ROOM), px(H + 2.0 * SHADOW_ROOM));
    let display = native::active_display()
        .and_then(|id| cx.find_display(DisplayId::new(id)))
        .or_else(|| cx.primary_display());
    let Some(display) = display else {
        return (None, Bounds::centered(None, window, cx));
    };
    let usable = display.visible_bounds();
    let origin = point(
        usable.origin.x + (usable.size.width - window.width) / 2.0,
        usable.origin.y + usable.size.height - px(ABOVE_BOTTOM + H + SHADOW_ROOM),
    );
    (
        Some(display.id()),
        Bounds {
            origin,
            size: window,
        },
    )
}

fn background(theme: Theme) -> WindowBackgroundAppearance {
    match theme {
        Theme::Kjedelig => WindowBackgroundAppearance::Blurred,
        Theme::Runepinne | Theme::SmiddJern => WindowBackgroundAppearance::Transparent,
    }
}

/// The window's background for the theme: clear, or the system's material in the pill's shape.
fn dress(window: &mut Window, theme: Theme) {
    window.set_background_appearance(background(theme));
    if theme == Theme::Kjedelig {
        native::shape_material(
            window,
            PillFrame {
                x: f64::from(SHADOW_ROOM),
                y: f64::from(SHADOW_ROOM),
                width: f64::from(W),
                height: f64::from(H),
                radius: f64::from(H / 2.0),
            },
        );
    }
}

struct PillView {
    state: Entity<State>,
}

impl Render for PillView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let scale = window.scale_factor();
        let frame = self.state.update(cx, |state, _| {
            let now = state.now();
            state.pill.advance(now);
            if state.pill.is_hidden() {
                return None;
            }
            state.frames += 1;
            let theme = state.theme;
            Some((
                state.pill.clone(),
                now,
                state.motion,
                state.accent,
                theme,
                state.face(theme, scale),
            ))
        });
        let Some((pill, now, motion, accent, theme, face)) = frame else {
            window.remove_window();
            return div().into_any_element();
        };
        window.request_animation_frame();
        let dark = matches!(
            window.appearance(),
            WindowAppearance::Dark | WindowAppearance::VibrantDark
        );
        let style = Style::of(theme, System { dark, accent });
        let opacity = pill.scene(now, motion).opacity;
        let paint = move |bounds: Bounds<Pixels>, (), window: &mut Window, cx: &mut App| {
            let origin = bounds.origin + point(px(SHADOW_ROOM), px(SHADOW_ROOM));
            let mut painter = Painter::new(window, cx, origin);
            let scene = pill.scene(now, motion);
            draw::plate(&mut painter, &style, face.as_ref());
            draw::states(&mut painter, &scene, &style, theme.carves_runes());
        };
        div()
            .size_full()
            .opacity(opacity)
            .child(canvas(|_, _, _| (), paint).size_full())
            .into_any_element()
    }
}
