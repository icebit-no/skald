//! How each theme draws the same states: its type, colours and the way it makes a mark (docs/design.md,
//! Overlay; the drawings of round 2). Runepinne and Smidd jern look the same in light and dark mode;
//! Kjedelig follows the system's appearance and accent colour.

use gpui_kit::{FontWeight, Rgba};
use skald_core::config::OverlayTheme;

use crate::geometry::{H, MIDDLE};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Theme {
    #[default]
    Runepinne,
    SmiddJern,
    Kjedelig,
}

impl From<&OverlayTheme> for Theme {
    fn from(theme: &OverlayTheme) -> Self {
        match theme {
            OverlayTheme::Runepinne | OverlayTheme::Unknown(_) => Theme::Runepinne,
            OverlayTheme::SmiddJern => Theme::SmiddJern,
            OverlayTheme::Kjedelig => Theme::Kjedelig,
        }
    }
}

/// What Kjedelig takes from the system.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct System {
    pub dark: bool,
    /// The accent colour the user chose, when the platform tells it.
    pub accent: Option<Rgba>,
}

pub(crate) struct Style {
    pub family: &'static str,
    pub size: f32,
    pub strong: FontWeight,
    /// The text's baseline, from the pill's top.
    pub baseline: f32,
    /// The time's digits each stand in a slot this wide, so the time never jitters.
    pub digit: f32,
    pub colon: f32,
    /// From the pill's ends to what it shows.
    pub pad: f32,
    pub text: Rgba,
    pub quiet_text: Rgba,
    pub text_shadow: Option<Rgba>,
    pub bar: Ink,
    /// A bar the chisel has passed before the text has arrived: a fresh cut.
    pub passed: Ink,
    pub rune: Ink,
    pub tick: Tick,
    pub word_divider: Rgba,
    pub ember: Ember,
    pub chisel: Chisel,
    /// The chisel line's top and bottom.
    pub chisel_span: (f32, f32),
    pub plate: Plate,
}

/// How a mark is laid down.
pub(crate) enum Ink {
    /// In layers, lit from the top left: a cut with its wall, or gold beaten into iron.
    Layered(Vec<InkLayer>),
    /// One flat colour, the bar a rounded bar.
    Flat(Rgba),
}

pub(crate) struct InkLayer {
    pub color: Rgba,
    pub width_factor: f32,
    pub width_added: f32,
    pub dx: f32,
    pub dy: f32,
}

pub(crate) enum Tick {
    /// Square ends and mitred, in the theme's ink.
    Cut(Ink),
    /// Round ends, flat.
    Round(Rgba),
}

pub(crate) enum Ember {
    /// A glowing coal, on a burnt ring where it sits in wood.
    Glowing { burnt_ring: Option<Rgba> },
    /// A flat dot in the system's red.
    Flat { red: Rgba },
}

pub(crate) enum Chisel {
    /// A bright line with a glow on either side.
    Glow,
    /// A light blade with a dark edge beside it.
    Knife,
    /// A line in the accent colour.
    Line(Rgba),
}

pub(crate) enum Plate {
    Oak,
    Iron,
    /// The system's material behind a tint, with a light inner rim and a thin edge.
    Glass {
        tint: Rgba,
        edge: Rgba,
        rim: Rgba,
    },
}

pub(crate) fn color(rgb: u32, alpha: f32) -> Rgba {
    Rgba {
        r: ((rgb >> 16) & 0xff) as f32 / 255.0,
        g: ((rgb >> 8) & 0xff) as f32 / 255.0,
        b: (rgb & 0xff) as f32 / 255.0,
        a: alpha,
    }
}

pub(crate) const ALEGREYA_SANS: &str = "Alegreya Sans";
const SYSTEM_FONT: &str = ".SystemUIFont";

impl Style {
    pub fn of(theme: Theme, system: System) -> Style {
        match theme {
            Theme::Runepinne => runepinne(),
            Theme::SmiddJern => smidd_jern(),
            Theme::Kjedelig => kjedelig(system),
        }
    }
}

/// A knife cut: a dark wall up to the left, the light wood of the cut beside it.
fn cut(wood: u32) -> Ink {
    Ink::Layered(vec![
        layer(color(0x100803, 0.85), 1.0, 0.35, -0.4, -0.4),
        layer(color(wood, 1.0), 0.82, 0.0, 0.22, 0.22),
    ])
}

/// Gold beaten into iron: a dark groove, the gold, and a highlight along its upper left.
fn inlay(gold: u32) -> Ink {
    Ink::Layered(vec![
        layer(color(0x000000, 0.62), 1.0, 0.9, 0.0, 0.0),
        layer(color(gold, 1.0), 1.0, 0.0, 0.0, 0.0),
        layer(color(0xfff4cd, 0.5), 0.3, 0.0, -0.45, -0.2),
    ])
}

fn layer(color: Rgba, width_factor: f32, width_added: f32, dx: f32, dy: f32) -> InkLayer {
    InkLayer {
        color,
        width_factor,
        width_added,
        dx,
        dy,
    }
}

fn runepinne() -> Style {
    Style {
        family: ALEGREYA_SANS,
        size: 14.5,
        strong: FontWeight::MEDIUM,
        baseline: MIDDLE + 4.8,
        digit: 7.4,
        colon: 3.6,
        pad: 27.0,
        text: color(0xede4d0, 1.0),
        quiet_text: color(0xc4b697, 1.0),
        text_shadow: Some(color(0x000000, 0.6)),
        bar: cut(0xdcc08a),
        passed: cut(0xf6e1b4),
        rune: cut(0xf6e1b4),
        tick: Tick::Cut(cut(0xf6e1b4)),
        word_divider: color(0xf6e1b4, 1.0),
        ember: Ember::Glowing {
            burnt_ring: Some(color(0x0c0602, 0.72)),
        },
        chisel: Chisel::Knife,
        chisel_span: (2.5, H - 4.5),
        plate: Plate::Oak,
    }
}

fn smidd_jern() -> Style {
    Style {
        family: ALEGREYA_SANS,
        size: 14.5,
        strong: FontWeight::MEDIUM,
        baseline: MIDDLE + 4.8,
        digit: 7.4,
        colon: 3.6,
        pad: 25.0,
        text: color(0xede4d0, 1.0),
        quiet_text: color(0xb5a98f, 1.0),
        text_shadow: Some(color(0x000000, 0.6)),
        bar: inlay(0xd3a349),
        passed: inlay(0xf0c674),
        rune: inlay(0xf0c674),
        tick: Tick::Cut(inlay(0xf0c674)),
        word_divider: color(0xf0c674, 1.0),
        ember: Ember::Glowing { burnt_ring: None },
        chisel: Chisel::Glow,
        chisel_span: (5.0, H - 5.0),
        plate: Plate::Iron,
    }
}

/// macOS: a HUD capsule in the system's material, SF Pro 13 pt, the system's label colours and accent.
/// Windows 11 and Linux get their own looks in M4 and M5.
fn kjedelig(system: System) -> Style {
    let (text, quiet_text, bar, default_accent, red, glass) = if system.dark {
        (
            color(0xffffff, 0.88),
            color(0xffffff, 0.58),
            color(0xffffff, 0.62),
            color(0x0a84ff, 1.0),
            color(0xff453a, 1.0),
            Plate::Glass {
                tint: color(0x222224, 0.68),
                edge: color(0x000000, 0.55),
                rim: color(0xffffff, 0.17),
            },
        )
    } else {
        (
            color(0x000000, 0.85),
            color(0x000000, 0.52),
            color(0x000000, 0.5),
            color(0x007aff, 1.0),
            color(0xff3b30, 1.0),
            Plate::Glass {
                tint: color(0xf6f6f6, 0.72),
                edge: color(0x000000, 0.2),
                rim: color(0xffffff, 0.6),
            },
        )
    };
    let accent = system.accent.unwrap_or(default_accent);
    Style {
        family: SYSTEM_FONT,
        size: 13.0,
        strong: FontWeight::MEDIUM,
        baseline: MIDDLE + 4.6,
        digit: 8.0,
        colon: 3.6,
        pad: 25.0,
        text,
        quiet_text,
        text_shadow: None,
        bar: Ink::Flat(bar),
        passed: Ink::Flat(accent),
        rune: Ink::Flat(accent),
        tick: Tick::Round(accent),
        word_divider: accent,
        ember: Ember::Flat { red },
        chisel: Chisel::Line(accent),
        chisel_span: (10.0, H - 10.0),
        plate: glass,
    }
}

impl Theme {
    /// Kjedelig has no runes: the chisel line turns the bars it passes to the accent colour.
    pub fn carves_runes(self) -> bool {
        self != Theme::Kjedelig
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_configured_theme_is_drawn_and_an_unknown_one_as_the_default() {
        assert_eq!(Theme::from(&OverlayTheme::Runepinne), Theme::Runepinne);
        assert_eq!(Theme::from(&OverlayTheme::SmiddJern), Theme::SmiddJern);
        assert_eq!(Theme::from(&OverlayTheme::Kjedelig), Theme::Kjedelig);
        assert_eq!(
            Theme::from(&OverlayTheme::Unknown("stein".into())),
            Theme::Runepinne
        );
        assert_eq!(Theme::from(&OverlayTheme::default()), Theme::default());
    }

    #[test]
    fn kjedelig_takes_the_system_accent() {
        let accent = color(0xff2d55, 1.0);
        let style = Style::of(
            Theme::Kjedelig,
            System {
                dark: true,
                accent: Some(accent),
            },
        );
        assert!(matches!(style.chisel, Chisel::Line(line) if line == accent));
        assert!(matches!(style.passed, Ink::Flat(passed) if passed == accent));
    }

    #[test]
    fn colours_are_read_from_hex() {
        let c = color(0xd9a94e, 0.5);
        assert_eq!((c.r * 255.0).round(), 217.0);
        assert_eq!((c.g * 255.0).round(), 169.0);
        assert_eq!((c.b * 255.0).round(), 78.0);
        assert_eq!(c.a, 0.5);
    }
}
