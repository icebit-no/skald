//! The states drawn in a theme: the plate, then each state on screen at its alpha (docs/design.md,
//! Overlay). The layout follows the drawings of round 2, in the pill's own points.

use std::f32::consts::{FRAC_PI_2, PI, TAU};
use std::sync::Arc;
use std::time::Duration;

use gpui_kit::{FontWeight, RenderImage, Rgba};

use crate::bars::{LIVE_BARS, LiveBar};
use crate::futhark::Mark;
use crate::geometry::{
    BAR_WIDTH, CARVED_PITCH, H, LIVE_PITCH, MIDDLE, RUNE_HEIGHT, RUNE_STROKE, RUNE_WIDTH, W,
};
use crate::paint::{Painter, Type};
use crate::pill::{Look, Scene};
use crate::text;
use crate::texture::MARGIN;
use crate::theme::{Chisel, Ember, Ink, Plate, Style, Tick, color};

/// Where things stand along the pill for a style.
struct Layout {
    pad: f32,
    dot: f32,
    time: f32,
    bars: f32,
    right: f32,
}

impl Layout {
    fn of(style: &Style) -> Self {
        let pad = style.pad;
        Self {
            pad,
            dot: pad + 4.5,
            time: pad + 15.0,
            bars: pad + 15.0 + 3.0 * style.digit + style.colon + 8.2,
            right: W - pad,
        }
    }

    /// A recording bar's left edge, `from_right` pitches left of where bars enter.
    fn live_bar(&self, from_right: f32) -> f32 {
        self.bars + (LIVE_BARS as f32 - 1.0) * LIVE_PITCH - from_right * LIVE_PITCH
    }

    /// A carved slot's left edge.
    fn slot(&self, slot: usize) -> f32 {
        self.pad + slot as f32 * CARVED_PITCH
    }
}

/// The plate's face, rendered for the window's scale.
pub(crate) struct Face {
    pub image: Arc<RenderImage>,
}

pub(crate) fn plate(painter: &mut Painter, style: &Style, face: Option<&Face>) {
    let whole = (0.0, 0.0, W, H);
    match style.plate {
        Plate::Oak | Plate::Iron => {
            let radius = if matches!(style.plate, Plate::Oak) {
                7.0
            } else {
                H / 2.0
            };
            painter.shadows(
                whole,
                radius,
                &[(15.0, 6.0, color(0, 0.4)), (2.5, 1.0, color(0, 0.5))],
            );
            if let Some(face) = face {
                let area = (-MARGIN, -MARGIN, W + 2.0 * MARGIN, H + 2.0 * MARGIN);
                painter.image(area, face.image.clone());
            }
            if matches!(style.plate, Plate::Iron) {
                serpents(painter);
                rivets(painter);
            }
        }
        Plate::Glass { tint, edge, rim } => {
            painter.shadows(whole, H / 2.0, &[(22.0, 8.0, color(0, 0.26))]);
            painter.framed(whole, H / 2.0, tint, (1.2, rim));
            let around = (-0.5, -0.5, W + 1.0, H + 1.0);
            painter.framed(around, H / 2.0 + 0.5, color(0, 0.0), (1.0, edge));
        }
    }
}

pub(crate) fn states(painter: &mut Painter, scene: &Scene, style: &Style, runes: bool) {
    let layout = Layout::of(style);
    for layer in &scene.layers {
        painter.alpha = layer.alpha;
        match &layer.look {
            Look::Recording {
                hands_free,
                elapsed,
                pulse,
                bars,
            } => {
                head(painter, style, &layout, *elapsed, *hands_free, *pulse, 1.0);
                let end = bars_end(painter, style, &layout, *hands_free);
                live_bars(painter, style, &layout, bars, end);
                hint(painter, style, &layout, *hands_free, 1.0);
            }
            Look::Carving {
                hands_free,
                elapsed,
                head: head_alpha,
                label,
                fold,
                from,
                to,
                chisel,
                runes: carved,
            } => {
                let carving = Carving {
                    fold: *fold,
                    from,
                    to,
                    chisel: *chisel,
                    runes: carved.filter(|_| runes),
                };
                head(
                    painter,
                    style,
                    &layout,
                    *elapsed,
                    *hands_free,
                    0.5,
                    *head_alpha,
                );
                carve(painter, style, &layout, &carving);
                hint(painter, style, &layout, *hands_free, 1.0 - label);
                rister(painter, style, &layout, *label);
            }
            Look::Inserted { text } => inserted(painter, style, &layout, text),
            look => {
                if let Some(words) = Message::of(look) {
                    message(painter, style, &layout, &words);
                }
            }
        }
    }
}

/// A state that is words alone (docs/design.md, Overlay): the first part strong, the rest quiet.
#[derive(Debug, PartialEq)]
struct Message<'a> {
    first: &'a str,
    /// The first part stays whole and the rest is cut to fit; otherwise the first part, a microphone's
    /// name, is cut.
    first_fixed: bool,
    rest: String,
}

impl<'a> Message<'a> {
    fn of(look: &Look<'a>) -> Option<Self> {
        let fixed = |first, rest: &str| {
            Some(Self {
                first,
                first_fixed: true,
                rest: rest.to_owned(),
            })
        };
        match look {
            Look::HeardNothing { microphone } => {
                fixed("Hørte ingenting", &format!(" · lyttet på {microphone}"))
            }
            Look::MicrophoneChanged { microphone } => Some(Self {
                first: microphone,
                first_fixed: false,
                rest: " · koblet til, brukes nå".to_owned(),
            }),
            Look::NotInserted => fixed("Ikke satt inn", " · ligger i Siste tekster"),
            Look::TranscriptionFailed => fixed("Transkriberingen feilet", ""),
            Look::NoMicrophone => fixed("Ingen mikrofon er koblet til", ""),
            Look::Recording { .. } | Look::Carving { .. } | Look::Inserted { .. } => None,
        }
    }
}

fn faded(painter: &mut Painter, alpha: f32, draw: impl FnOnce(&mut Painter)) {
    if alpha <= 0.0 {
        return;
    }
    let before = painter.alpha;
    painter.alpha *= alpha;
    draw(painter);
    painter.alpha = before;
}

fn look(style: &Style, weight: FontWeight) -> Type {
    Type {
        family: style.family,
        size: style.size,
        weight,
        figures: false,
    }
}

/// Text with the theme's shadow under it.
fn words(
    painter: &mut Painter,
    style: &Style,
    text: &str,
    x: f32,
    color: Rgba,
    weight: FontWeight,
) -> f32 {
    words_in(painter, style, text, x, color, look(style, weight))
}

fn words_in(
    painter: &mut Painter,
    style: &Style,
    text: &str,
    x: f32,
    color: Rgba,
    look: Type,
) -> f32 {
    if let Some(shadow) = style.text_shadow {
        painter.text(text, look, x, style.baseline + 0.8, shadow);
    }
    painter.text(text, look, x, style.baseline, color)
}

/// The dot, or the padlock in a hands-free take, and the elapsed time.
fn head(
    painter: &mut Painter,
    style: &Style,
    layout: &Layout,
    elapsed: Duration,
    hands_free: bool,
    pulse: f32,
    alpha: f32,
) {
    faded(painter, alpha, |painter| {
        if hands_free {
            padlock(painter, style, layout.dot, MIDDLE - 0.6);
        } else {
            ember(painter, style, layout.dot, MIDDLE, pulse);
        }
        let figures = Type {
            figures: true,
            ..look(style, style.strong)
        };
        let mut x = layout.time;
        for digit in text::elapsed(elapsed).chars() {
            let slot = if digit == ':' {
                style.colon
            } else {
                style.digit
            };
            let glyph = digit.to_string();
            let width = painter.measure(&glyph, figures);
            words_in(
                painter,
                style,
                &glyph,
                x + (slot - width) / 2.0,
                style.text,
                figures,
            );
            x += slot;
        }
    });
}

fn ember(painter: &mut Painter, style: &Style, x: f32, y: f32, pulse: f32) {
    match &style.ember {
        Ember::Flat { red } => {
            let mut halo = *red;
            halo.a *= 0.1 + 0.2 * pulse;
            painter.circle(x, y, 8.5, halo);
            painter.circle(x, y, 4.5, *red);
        }
        Ember::Glowing { burnt_ring } => {
            if let Some(ring) = burnt_ring {
                painter.circle(x, y, 6.4, *ring);
            }
            // GPUI has no radial gradient: the glow is rings of falling strength.
            let strength = 0.2 + 0.34 * pulse;
            const RINGS: usize = 8;
            for ring in 0..RINGS {
                let radius = 12.0 * (RINGS - ring) as f32 / RINGS as f32;
                painter.circle(x, y, radius, color(0xe2664e, strength / RINGS as f32 * 1.6));
            }
            painter.circle(x, y, 4.4, color(0xc2412d, 1.0));
            painter.circle(x - 0.3, y - 0.3, 3.6, color(0xe2664e, 1.0));
            let hot = 1.4 + 1.4 * pulse;
            painter.circle(x - 0.8, y - 0.8, hot + 0.6, color(0xffc9a1, 0.45));
            painter.circle(x - 0.8, y - 0.8, hot, color(0xffc9a1, 0.9));
        }
    }
}

fn padlock(painter: &mut Painter, style: &Style, x: f32, y: f32) {
    let shackle: Vec<(f32, f32)> = [(x - 2.9, y - 0.6)]
        .into_iter()
        .chain(arc((x, y - 3.0), 2.9, PI, TAU, 10))
        .chain([(x + 2.9, y - 0.6)])
        .collect();
    let (body_radius, metal) = match &style.ember {
        Ember::Flat { red } => (2.0, *red),
        Ember::Glowing { burnt_ring } => {
            if burnt_ring.is_some() {
                painter.circle(x, y + 0.6, 8.0, color(0x0c0602, 0.5));
            }
            let shadow = color(0, 0.55);
            painter.stroke(&[&shackle], 2.0, (-0.4, -0.4), shadow);
            painter.rounded(x - 4.7 - 0.4, y - 1.1 - 0.4, 9.4, 7.3, 1.4, shadow);
            (1.4, color(0xe2664e, 1.0))
        }
    };
    painter.stroke(&[&shackle], 1.6, (0.0, 0.0), metal);
    painter.rounded(x - 4.7, y - 1.1, 9.4, 7.3, body_radius, metal);
    if matches!(style.ember, Ember::Glowing { .. }) {
        painter.circle(x, y + 2.5, 1.0, color(0x140a06, 0.8));
    }
}

/// Points along a circle from `start` to `end`, angles clockwise on screen from the right.
fn arc(
    centre: (f32, f32),
    radius: f32,
    start: f32,
    end: f32,
    steps: usize,
) -> impl Iterator<Item = (f32, f32)> {
    (0..=steps).map(move |step| {
        let angle = start + (end - start) * step as f32 / steps as f32;
        (
            centre.0 + radius * angle.cos(),
            centre.1 + radius * angle.sin(),
        )
    })
}

fn bar(painter: &mut Painter, ink: &Ink, x: f32, height: f32) {
    let y = MIDDLE - height / 2.0;
    match ink {
        Ink::Flat(color) => painter.rounded(x, y, BAR_WIDTH, height, 1.5, *color),
        Ink::Layered(layers) => {
            for layer in layers {
                let width = BAR_WIDTH * layer.width_factor + layer.width_added;
                painter.rect(
                    x + (BAR_WIDTH - width) / 2.0 + layer.dx,
                    y - layer.width_added / 2.0 + layer.dy,
                    width,
                    height + layer.width_added,
                    layer.color,
                );
            }
        }
    }
}

/// The room kept between the recording bars and a hint named in words.
const HINT_GAP: f32 = 8.0;

/// Where the recording bars end so the hint beside them stays clear. macOS's drawn keys fit beside every
/// bar; a hint named in words can reach further, and the oldest bars give way to it.
fn bars_end(painter: &mut Painter, style: &Style, layout: &Layout, hands_free: bool) -> f32 {
    if cfg!(target_os = "macos") {
        return f32::INFINITY;
    }
    worded_hint_left(painter, style, layout, hands_free) - HINT_GAP
}

fn live_bars(painter: &mut Painter, style: &Style, layout: &Layout, bars: &[LiveBar], end: f32) {
    let left = layout.bars - 1.0;
    let right = (layout.live_bar(0.0) + BAR_WIDTH + 1.0).min(end);
    painter.clip_across(left, right, |painter| {
        for live in bars {
            bar(
                painter,
                &style.bar,
                layout.live_bar(live.from_right),
                live.height,
            );
        }
    });
}

struct Carving<'a> {
    fold: f32,
    from: &'a [LiveBar],
    to: &'a [f32],
    chisel: f32,
    runes: Option<&'a [Mark]>,
}

/// The bars folding into the take, and behind the chisel the runes, or the bars it has passed.
fn carve(painter: &mut Painter, style: &Style, layout: &Layout, carving: &Carving) {
    let edge = layout.pad - 2.0 + carving.chisel * (LIVE_BARS as f32 * CARVED_PITCH + 2.0);
    let folded = |slot: usize| {
        let from = carving.from[slot];
        let height = lerp(from.height, carving.to[slot], carving.fold);
        let x = lerp(
            layout.live_bar(from.from_right),
            layout.slot(slot) + 2.5,
            carving.fold,
        );
        (x, height)
    };
    painter.clip_across(edge, W, |painter| {
        for slot in 0..carving.from.len().min(carving.to.len()) {
            let (x, height) = folded(slot);
            bar(painter, &style.bar, x, height);
        }
    });
    if carving.chisel <= 0.0 {
        return;
    }
    painter.clip_across(0.0, edge, |painter| match carving.runes {
        Some(runes) => {
            for (slot, mark) in runes.iter().enumerate() {
                rune(
                    painter,
                    style,
                    *mark,
                    layout.slot(slot) + 0.5,
                    MIDDLE - RUNE_HEIGHT / 2.0,
                );
            }
        }
        None => {
            for slot in 0..carving.from.len().min(carving.to.len()) {
                let (x, height) = folded(slot);
                bar(painter, &style.passed, x, height);
            }
        }
    });
    if carving.chisel < 1.0 {
        chisel(painter, style, edge);
    }
}

fn rune(painter: &mut Painter, style: &Style, mark: Mark, x: f32, y: f32) {
    let rune = match mark {
        Mark::Rune(rune) => rune,
        Mark::WordDivider => {
            let (cx, cy, reach) = (
                x + RUNE_WIDTH / 2.0,
                y + RUNE_HEIGHT / 2.0,
                1.1 * 2f32.sqrt(),
            );
            let diamond = [
                (cx, cy - reach),
                (cx + reach, cy),
                (cx, cy + reach),
                (cx - reach, cy),
            ];
            painter.fill(&[&diamond], style.word_divider);
            return;
        }
    };
    let strokes: Vec<Vec<(f32, f32)>> = rune
        .strokes()
        .iter()
        .map(|stroke| {
            stroke
                .outline(RUNE_WIDTH, RUNE_HEIGHT, RUNE_STROKE)
                .into_iter()
                .map(|(sx, sy)| (x + sx, y + sy))
                .collect()
        })
        .collect();
    let lines: Vec<&[(f32, f32)]> = strokes.iter().map(Vec::as_slice).collect();
    inked(painter, &style.rune, &lines, RUNE_STROKE);
}

fn inked(painter: &mut Painter, ink: &Ink, lines: &[&[(f32, f32)]], width: f32) {
    match ink {
        Ink::Flat(color) => painter.stroke(lines, width, (0.0, 0.0), *color),
        Ink::Layered(layers) => {
            for layer in layers {
                let layer_width = width * layer.width_factor + layer.width_added;
                painter.stroke(lines, layer_width, (layer.dx, layer.dy), layer.color);
            }
        }
    }
}

fn chisel(painter: &mut Painter, style: &Style, x: f32) {
    let (top, bottom) = style.chisel_span;
    let height = bottom - top;
    match style.chisel {
        Chisel::Glow => {
            let glow = color(0xf0c674, 0.34);
            let clear = color(0xf0c674, 0.0);
            painter.shade_across(x - 9.0, top, 9.0, height, clear, glow);
            painter.shade_across(x, top, 9.0, height, glow, clear);
            painter.rect(x - 0.8, top, 1.6, height, color(0xffe2a0, 1.0));
        }
        Chisel::Knife => {
            painter.rect(x + 0.7, top, 1.3, height, color(0, 0.45));
            painter.rect(x - 0.7, top, 1.4, height, color(0xfff0c6, 1.0));
        }
        Chisel::Line(accent) => painter.rounded(x - 1.0, top, 2.0, height, 1.0, accent),
    }
}

/// The hands-free chord where its keys are named in words.
const HANDS_FREE_KEYS: &str = if cfg!(windows) {
    "Ctrl+Win+X"
} else {
    "Ctrl+Super+X"
};

/// «slipp ⌘», or «⌃⌘X stopper» in a hands-free take, at the right end. Windows and Linux name their keys
/// in words: «slipp Ctrl», and «Ctrl+Win+X stopper» or «Ctrl+Super+X stopper».
fn hint(painter: &mut Painter, style: &Style, layout: &Layout, hands_free: bool, alpha: f32) {
    faded(painter, alpha, |painter| {
        if cfg!(target_os = "macos") {
            drawn_hint(painter, style, layout, hands_free);
        } else {
            worded_hint(painter, style, layout, hands_free);
        }
    });
}

fn worded_hint_left(
    painter: &mut Painter,
    style: &Style,
    layout: &Layout,
    hands_free: bool,
) -> f32 {
    let (quiet, keys) = if hands_free {
        ("stopper", HANDS_FREE_KEYS)
    } else {
        ("slipp", "Ctrl")
    };
    let quiet = painter.measure(quiet, look(style, FontWeight::NORMAL));
    let keys = painter.measure(keys, look(style, style.strong));
    layout.right - quiet - 5.0 - keys
}

fn worded_hint(painter: &mut Painter, style: &Style, layout: &Layout, hands_free: bool) {
    let right = layout.right;
    if hands_free {
        let stopper = painter.measure("stopper", look(style, FontWeight::NORMAL));
        let keys = painter.measure(HANDS_FREE_KEYS, look(style, style.strong));
        let x = right - stopper;
        words(
            painter,
            style,
            "stopper",
            x,
            style.quiet_text,
            FontWeight::NORMAL,
        );
        words(
            painter,
            style,
            HANDS_FREE_KEYS,
            x - 5.0 - keys,
            style.text,
            style.strong,
        );
    } else {
        let key = painter.measure("Ctrl", look(style, style.strong));
        let slipp = painter.measure("slipp", look(style, FontWeight::NORMAL));
        words(
            painter,
            style,
            "Ctrl",
            right - key,
            style.text,
            style.strong,
        );
        words(
            painter,
            style,
            "slipp",
            right - key - 5.0 - slipp,
            style.quiet_text,
            FontWeight::NORMAL,
        );
    }
}

/// The keys drawn as macOS draws them.
fn drawn_hint(painter: &mut Painter, style: &Style, layout: &Layout, hands_free: bool) {
    let right = layout.right;
    if !hands_free {
        command_key(painter, style, right - 5.6);
        let width = painter.measure("slipp", look(style, FontWeight::NORMAL));
        words(
            painter,
            style,
            "slipp",
            right - 16.5 - width,
            style.quiet_text,
            FontWeight::NORMAL,
        );
        return;
    }
    let stopper = painter.measure("stopper", look(style, FontWeight::NORMAL));
    let key = painter.measure("X", look(style, style.strong));
    let mut x = right - stopper;
    words(
        painter,
        style,
        "stopper",
        x,
        style.quiet_text,
        FontWeight::NORMAL,
    );
    x -= 5.0 + key;
    words(painter, style, "X", x, style.text, style.strong);
    x -= 2.5 + 11.2;
    command_key(painter, style, x + 5.6);
    x -= 2.0 + 7.0;
    control_key(painter, style, x + 3.5);
}

fn rister(painter: &mut Painter, style: &Style, layout: &Layout, alpha: f32) {
    faded(painter, alpha, |painter| {
        let width = painter.measure("rister …", look(style, FontWeight::NORMAL));
        words(
            painter,
            style,
            "rister …",
            layout.right - width,
            style.quiet_text,
            FontWeight::NORMAL,
        );
    });
}

/// ⌘, drawn: the font may not have it.
fn command_key(painter: &mut Painter, style: &Style, x: f32) {
    let (q, r) = (1.95, 1.75);
    let o = q + r;
    let at = |points: Vec<(f32, f32)>| -> Vec<(f32, f32)> {
        points
            .into_iter()
            .map(|(px, py)| (x + px, MIDDLE + py))
            .collect()
    };
    let lines = [
        at(vec![(-o, -q), (o, -q)]),
        at(vec![(-o, q), (o, q)]),
        at(vec![(-q, -o), (-q, o)]),
        at(vec![(q, -o), (q, o)]),
        at(arc((-o, -o), r, FRAC_PI_2, TAU, 9).collect()),
        at(arc((o, -o), r, PI, PI * 2.5, 9).collect()),
        at(arc((o, o), r, PI * 1.5, PI * 3.0, 9).collect()),
        at(arc((-o, o), r, 0.0, PI * 1.5, 9).collect()),
    ];
    key_glyph(painter, style, &lines, 1.15);
}

/// ⌃, drawn.
fn control_key(painter: &mut Painter, style: &Style, x: f32) {
    let y = MIDDLE - 2.6;
    let caret = vec![(x - 3.1, y + 1.6), (x, y - 1.9), (x + 3.1, y + 1.6)];
    key_glyph(painter, style, &[caret], 1.3);
}

fn key_glyph(painter: &mut Painter, style: &Style, lines: &[Vec<(f32, f32)>], width: f32) {
    let lines: Vec<&[(f32, f32)]> = lines.iter().map(Vec::as_slice).collect();
    if let Some(shadow) = style.text_shadow {
        painter.stroke(&lines, width, (0.0, 0.8), shadow);
    }
    painter.stroke(&lines, width, (0.0, 0.0), style.text);
}

/// Satt inn: a tick and the start of the text, in plain type.
fn inserted(painter: &mut Painter, style: &Style, layout: &Layout, text: &str) {
    let tick = [(0.0, 5.4), (4.1, 9.4), (11.4, 0.6)];
    match &style.tick {
        Tick::Round(accent) => {
            let (x, y) = (layout.pad + 1.0, MIDDLE - 5.0);
            let points: Vec<(f32, f32)> = tick.iter().map(|&(tx, ty)| (x + tx, y + ty)).collect();
            painter.stroke(&[&points], 2.0, (0.0, 0.0), *accent);
            for &(ex, ey) in [points[0], points[2]].iter() {
                painter.circle(ex, ey, 1.0, *accent);
            }
        }
        Tick::Cut(ink) => {
            let (x, y) = (layout.pad, MIDDLE - 5.0);
            let mut points: Vec<(f32, f32)> =
                tick.iter().map(|&(tx, ty)| (x + tx, y + ty)).collect();
            square_ends(&mut points, 2.1);
            inked(painter, ink, &[&points], 2.1);
        }
    }
    let strong = look(style, style.strong);
    let room = layout.right - layout.pad - 21.0;
    let shown = text::start_of(text, room, |candidate| painter.measure(candidate, strong));
    words(
        painter,
        style,
        &shown,
        layout.pad + 21.0,
        style.text,
        style.strong,
    );
}

/// Carries both ends of a polyline half the width further, so butt ends read as square.
fn square_ends(points: &mut [(f32, f32)], width: f32) {
    let last = points.len() - 1;
    for (end, inner) in [(0, 1), (last, last - 1)] {
        let (ex, ey) = points[end];
        let (ix, iy) = points[inner];
        let length = (ex - ix).hypot(ey - iy);
        let reach = width / 2.0 / length;
        points[end] = (ex + (ex - ix) * reach, ey + (ey - iy) * reach);
    }
}

/// A message centred on the pill: its first part in the strong weight, the rest quieter. A first part
/// that is a name is cut to fit.
fn message(painter: &mut Painter, style: &Style, layout: &Layout, message: &Message) {
    let strong = look(style, style.strong);
    let quiet = look(style, FontWeight::NORMAL);
    let room = W - 2.0 * layout.pad;
    let mut rest = message.rest.clone();
    let mut first = message.first.to_owned();
    let rest_width = painter.measure(&rest, quiet);
    if message.first_fixed {
        let first_width = painter.measure(&first, strong);
        if first_width + rest_width > room {
            // The fixed words stay; the microphone's name in the rest is cut.
            rest = text::cut(&rest, room - first_width, |candidate| {
                painter.measure(candidate, quiet)
            });
        }
    } else {
        first = text::cut(&first, room - rest_width, |candidate| {
            painter.measure(candidate, strong)
        });
    }
    let first_width = painter.measure(&first, strong);
    let rest_width = painter.measure(&rest, quiet);
    let x = (W - first_width - rest_width) / 2.0;
    words(painter, style, &first, x, style.text, style.strong);
    words(
        painter,
        style,
        &rest,
        x + first_width,
        style.quiet_text,
        FontWeight::NORMAL,
    );
}

fn lerp(from: f32, to: f32, t: f32) -> f32 {
    from + (to - from) * t
}

/// Smidd jern's gold edge: two Urnes-style serpents, each along one long edge, bending round one end and
/// biting the other's tail. The second is the first turned half a turn.
fn serpents(painter: &mut Painter) {
    let gold_top = color(0xf0c674, 1.0);
    let gold_bottom = color(0xc69641, 1.0);
    let outline = color(0, 0.7);
    let beasts = [serpent(false), serpent(true)];
    for beast in &beasts {
        for part in [&beast.body, &beast.head, &beast.lappet] {
            let mut closed = part.clone();
            closed.push(part[0]);
            painter.stroke(&[&closed], 0.9, (0.0, 0.0), outline);
        }
    }
    for beast in &beasts {
        for part in [&beast.body, &beast.head, &beast.lappet] {
            painter.fill_shaded(part, gold_top, gold_bottom);
        }
        painter.fill(&[&beast.eye], color(0x17130f, 1.0));
    }
}

struct Serpent {
    body: Vec<(f32, f32)>,
    head: Vec<(f32, f32)>,
    lappet: Vec<(f32, f32)>,
    eye: Vec<(f32, f32)>,
}

fn serpent(turned: bool) -> Serpent {
    let place = |x: f32, y: f32| if turned { (W - x, H - y) } else { (x, y) };
    let (inset, radius, tip, k) = (4.0, H / 2.0, W - 46.0, 1.15);
    let (mut left, mut right) = (Vec::new(), Vec::new());
    let mut rib = |x: f32, y: f32, nx: f32, ny: f32, width: f32| {
        left.push(place(x + nx * width / 2.0, y + ny * width / 2.0));
        right.push(place(x - nx * width / 2.0, y - ny * width / 2.0));
    };
    // The tail and the back, along the top edge.
    let mut x = tip;
    while x > radius {
        let s = tip - x;
        let width = if s < 34.0 {
            lerp(0.5, 1.6, smooth(s / 34.0))
        } else {
            1.6
        };
        let lift = if s < 20.0 {
            (1.0 - s / 20.0).powi(2) * 3.0
        } else {
            0.0
        };
        rib(x, inset + width / 2.0 + lift, 0.0, -1.0, width);
        x -= 3.0;
    }
    // The neck, thickening round the end.
    for i in 0..=40 {
        let a = i as f32 / 40.0;
        let angle = -FRAC_PI_2 - a * PI;
        let width = lerp(1.6, 5.0 * k, smooth(a));
        let r = radius - inset - width / 2.0;
        rib(
            radius + r * angle.cos(),
            radius + r * angle.sin(),
            angle.cos(),
            angle.sin(),
            width,
        );
    }
    let mut body = left;
    body.extend(right.into_iter().rev());

    // The head in profile: u along the snout, v up from the jaw's line.
    let at = |u: f32, v: f32| place(radius + u * k, H - inset - v * k);
    let mut head = Outline::new(at(-0.5, 0.0));
    head.line(at(8.0, 0.0));
    head.quad(at(13.0, 0.1), at(17.6, 1.5));
    head.line(at(17.2, 2.3));
    head.quad(at(13.0, 2.1), at(9.6, 3.2));
    head.quad(at(14.5, 3.9), at(20.6, 3.6));
    head.cubic(at(23.3, 3.8), at(24.8, 6.2), at(23.3, 8.2));
    head.cubic(at(22.3, 9.6), at(20.2, 9.4), at(19.8, 7.9));
    head.cubic(at(21.2, 8.2), at(22.2, 7.2), at(21.4, 6.2));
    head.cubic(at(20.6, 5.4), at(19.0, 5.6), at(17.0, 6.2));
    head.quad(at(12.5, 7.4), at(9.0, 9.2));
    head.cubic(at(6.5, 10.3), at(2.6, 9.8), at(0.6, 7.4));
    head.line(at(-0.5, 5.0));

    // The neck lappet, a tapering tendril from the back of the skull.
    let tendril: [(f32, f32, f32); 6] = [
        (2.6, 9.3, 1.5),
        (0.4, 10.2, 1.4),
        (-2.6, 10.3, 1.2),
        (-5.2, 9.6, 1.0),
        (-6.5, 8.2, 0.8),
        (-5.5, 7.0, 0.5),
    ];
    let (mut upper, mut lower) = (Vec::new(), Vec::new());
    for (i, &(u, v, width)) in tendril.iter().enumerate() {
        let next = tendril[(i + 1).min(tendril.len() - 1)];
        let previous = tendril[i.saturating_sub(1)];
        let (du, dv) = (next.0 - previous.0, next.1 - previous.1);
        let length = du.hypot(dv);
        upper.push(at(
            u - dv / length * width / 2.0,
            v + du / length * width / 2.0,
        ));
        lower.push(at(
            u + dv / length * width / 2.0,
            v - du / length * width / 2.0,
        ));
    }
    let mut lappet = upper;
    lappet.extend(lower.into_iter().rev());

    let mut eye = Outline::new(at(3.1, 6.1));
    eye.quad(at(6.4, 10.0), at(11.8, 6.6));
    eye.quad(at(7.0, 3.6), at(3.1, 6.1));

    Serpent {
        body,
        head: head.points,
        lappet,
        eye: eye.points,
    }
}

/// A closed outline with its curves flattened into short lines.
struct Outline {
    points: Vec<(f32, f32)>,
}

const CURVE_STEPS: usize = 8;

impl Outline {
    fn new(start: (f32, f32)) -> Self {
        Self {
            points: vec![start],
        }
    }

    fn last(&self) -> (f32, f32) {
        *self.points.last().expect("an outline starts with a point")
    }

    fn line(&mut self, to: (f32, f32)) {
        self.points.push(to);
    }

    fn quad(&mut self, control: (f32, f32), to: (f32, f32)) {
        let from = self.last();
        for step in 1..=CURVE_STEPS {
            let t = step as f32 / CURVE_STEPS as f32;
            let u = 1.0 - t;
            self.points.push((
                u * u * from.0 + 2.0 * u * t * control.0 + t * t * to.0,
                u * u * from.1 + 2.0 * u * t * control.1 + t * t * to.1,
            ));
        }
    }

    fn cubic(&mut self, first: (f32, f32), second: (f32, f32), to: (f32, f32)) {
        let from = self.last();
        for step in 1..=CURVE_STEPS {
            let t = step as f32 / CURVE_STEPS as f32;
            let u = 1.0 - t;
            let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
            self.points.push((
                a * from.0 + b * first.0 + c * second.0 + d * to.0,
                a * from.1 + b * first.1 + c * second.1 + d * to.1,
            ));
        }
    }
}

fn smooth(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

/// A domed rivet at each end of the iron plate.
fn rivets(painter: &mut Painter) {
    for x in [14.5, W - 14.5] {
        painter.circle(x + 0.4, MIDDLE + 0.7, 4.4, color(0, 0.65));
        painter.circle(x, MIDDLE, 3.6, color(0xb98a39, 1.0));
        painter.circle(x - 0.5, MIDDLE - 0.6, 2.7, color(0xe6ba60, 1.0));
        painter.circle(x - 1.2, MIDDLE - 1.3, 1.1, color(0xfff7db, 0.92));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_recording_bars_end_where_the_carved_row_would_not_reach() {
        let style = crate::theme::Style::of(crate::theme::Theme::Runepinne, Default::default());
        let layout = Layout::of(&style);
        let last_live = layout.live_bar(0.0) + BAR_WIDTH;
        assert!(
            last_live < layout.right - 40.0,
            "room is left for «slipp ⌘»"
        );
        let last_slot = layout.slot(LIVE_BARS - 1) + RUNE_WIDTH;
        assert!(
            last_slot < layout.right - 50.0,
            "room is left for «rister …»"
        );
    }

    #[test]
    fn a_serpent_stays_on_the_plate() {
        for turned in [false, true] {
            let serpent = serpent(turned);
            for &(x, y) in serpent
                .body
                .iter()
                .chain(&serpent.head)
                .chain(&serpent.lappet)
            {
                assert!(
                    (0.0..=W).contains(&x) && (0.0..=H).contains(&y),
                    "({x}, {y})"
                );
            }
        }
    }

    #[test]
    fn the_second_serpent_is_the_first_turned() {
        let (first, second) = (serpent(false), serpent(true));
        for (a, b) in first.head.iter().zip(&second.head) {
            assert!((a.0 + b.0 - W).abs() < 1e-3 && (a.1 + b.1 - H).abs() < 1e-3);
        }
    }

    #[test]
    fn each_message_reads_as_design_md_gives_it() {
        let read = |look: Look| {
            let message = Message::of(&look).expect("words alone");
            (
                format!("{}{}", message.first, message.rest),
                message.first_fixed,
            )
        };
        assert_eq!(
            read(Look::HeardNothing {
                microphone: "Shure MV7"
            }),
            ("Hørte ingenting · lyttet på Shure MV7".to_owned(), true)
        );
        assert_eq!(
            read(Look::MicrophoneChanged {
                microphone: "Shure MV7"
            }),
            ("Shure MV7 · koblet til, brukes nå".to_owned(), false)
        );
        assert_eq!(
            read(Look::NotInserted),
            ("Ikke satt inn · ligger i Siste tekster".to_owned(), true)
        );
        assert_eq!(
            read(Look::TranscriptionFailed),
            ("Transkriberingen feilet".to_owned(), true)
        );
        assert_eq!(
            read(Look::NoMicrophone),
            ("Ingen mikrofon er koblet til".to_owned(), true)
        );
        assert_eq!(Message::of(&Look::Inserted { text: "Hei." }), None);
    }

    #[test]
    fn every_message_fits_the_pill_in_every_theme() {
        // Measured with Core Text, these words take at most 0.46 em a letter in Alegreya Sans Medium and
        // SF Pro (204 pt of the 376 pt there is); 0.62 leaves room for a wider system font.
        for theme in [
            crate::theme::Theme::Runepinne,
            crate::theme::Theme::SmiddJern,
            crate::theme::Theme::Kjedelig,
        ] {
            let style = crate::theme::Style::of(theme, Default::default());
            let room = W - 2.0 * style.pad;
            for look in [
                Look::NotInserted,
                Look::TranscriptionFailed,
                Look::NoMicrophone,
            ] {
                let message = Message::of(&look).unwrap();
                let letters = (message.first.chars().count() + message.rest.chars().count()) as f32;
                assert!(letters * style.size * 0.62 < room, "{look:?} in {theme:?}");
            }
        }
    }

    #[test]
    fn square_ends_reach_half_the_width() {
        let mut points = [(0.0, 0.0), (10.0, 0.0)];
        square_ends(&mut points, 2.0);
        assert_eq!(points, [(-1.0, 0.0), (11.0, 0.0)]);
    }
}
