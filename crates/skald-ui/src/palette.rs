//! The windows' colours: the identity's in dark mode (docs/design.md, Identity), and the same roles on light
//! ground in light mode, so a window follows the system as it switches.

use gpui_kit::{Hsla, Rgba, WindowAppearance};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Palette {
    pub dark: bool,
    pub ground: Hsla,
    pub surface: Hsla,
    /// Fields, buttons and the track of a progress bar.
    pub raised: Hsla,
    pub line: Hsla,
    pub text: Hsla,
    pub quiet: Hsla,
    /// What cannot be chosen or done now.
    pub faint: Hsla,
    /// Buttons, what is chosen, highlights.
    pub gold: Hsla,
    pub on_gold: Hsla,
    pub link: Hsla,
    /// What is done, as a tick or a word on the ground.
    pub done: Hsla,
    /// A warning or an error.
    pub ochre: Hsla,
    pub selection: Hsla,
}

impl Palette {
    pub fn of(appearance: WindowAppearance) -> Self {
        match appearance {
            WindowAppearance::Dark | WindowAppearance::VibrantDark => Self::dark(),
            WindowAppearance::Light | WindowAppearance::VibrantLight => Self::light(),
        }
    }

    fn dark() -> Self {
        Self {
            dark: true,
            ground: color(0x12100d),  // soot
            surface: color(0x1b1813), // hearth
            raised: color(0x25211b),  // iron
            line: color(0x3a3329),    // seam
            text: color(0xede4d0),    // bone
            quiet: color(0xab9f88),   // ash
            faint: color(0x6e6555),
            gold: color(0xd9a94e),
            on_gold: color(0x12100d),
            link: color(0xf0c674),
            done: color(0xd9a94e),
            // The lighter ochre stands out on dark ground.
            ochre: color(0xe2664e),
            selection: with_alpha(color(0xd9a94e), 0.35),
        }
    }

    /// The identity names no light ground; these are its bone and soot turned around, with the darker
    /// ochre, which reads on light ground.
    fn light() -> Self {
        Self {
            dark: false,
            ground: color(0xf3eee3),
            surface: color(0xfaf7f0),
            raised: color(0xffffff),
            line: color(0xd9cfbb),
            text: color(0x12100d),
            quiet: color(0x655b49),
            faint: color(0xa89e8b),
            gold: color(0xd9a94e),
            on_gold: color(0x12100d),
            // Gold is too light to read as text on light ground: links take soot, and what is done a
            // darker gold.
            link: color(0x12100d),
            done: color(0x9a6f1f),
            ochre: color(0xc2412d),
            selection: with_alpha(color(0xd9a94e), 0.4),
        }
    }
}

fn color(rgb: u32) -> Hsla {
    Rgba {
        r: ((rgb >> 16) & 0xff) as f32 / 255.0,
        g: ((rgb >> 8) & 0xff) as f32 / 255.0,
        b: (rgb & 0xff) as f32 / 255.0,
        a: 1.0,
    }
    .into()
}

pub fn with_alpha(mut color: Hsla, alpha: f32) -> Hsla {
    color.a = alpha;
    color
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dark_mode_takes_the_identitys_colours() {
        let dark = Palette::of(WindowAppearance::Dark);
        assert!(dark.dark);
        assert_eq!(dark.ground, color(0x12100d));
        assert_eq!(dark.text, color(0xede4d0));
        assert_eq!(Palette::of(WindowAppearance::VibrantDark), dark);
    }

    #[test]
    fn light_mode_turns_ground_and_text_around() {
        let light = Palette::of(WindowAppearance::Light);
        assert!(!light.dark);
        assert_eq!(light.text, color(0x12100d));
        assert_eq!(light.gold, Palette::of(WindowAppearance::Dark).gold);
        assert_eq!(with_alpha(light.gold, 0.5).a, 0.5);
    }
}
