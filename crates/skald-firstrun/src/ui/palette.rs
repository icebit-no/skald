//! The window's colours: the identity's in dark mode (docs/design.md, Identity), and the same roles on
//! light ground in light mode, so the window follows the system as it switches. The values are the
//! settings window's, so Skald's two windows look alike.

use gpui_kit::{Hsla, Rgba, WindowAppearance};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Palette {
    pub dark: bool,
    pub ground: Hsla,
    pub surface: Hsla,
    /// Buttons and the progress bar's track.
    pub raised: Hsla,
    pub line: Hsla,
    pub text: Hsla,
    pub quiet: Hsla,
    /// What cannot be done yet.
    pub faint: Hsla,
    /// Buttons, the current step, what is done.
    pub gold: Hsla,
    pub on_gold: Hsla,
    /// A warning or an error.
    pub ochre: Hsla,
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
            // The lighter ochre stands out on dark ground.
            ochre: color(0xe2664e),
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
            ochre: color(0xc2412d),
        }
    }

    /// What is done, as a tick or a word on the ground: the identity's gold on dark ground, a darker gold
    /// on light ground, where the identity's does not read as text.
    pub fn done(&self) -> Hsla {
        if self.dark {
            self.gold
        } else {
            color(0x9a6f1f)
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

pub(crate) fn with_alpha(mut color: Hsla, alpha: f32) -> Hsla {
    color.a = alpha;
    color
}
