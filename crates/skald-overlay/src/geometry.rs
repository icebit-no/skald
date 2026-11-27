//! The pill's measures in points, the same in every theme (docs/design.md, Overlay).

pub const W: f32 = 430.0;
pub const H: f32 = 58.0;
/// The line the bars, the runes and the dot are centred on.
pub const MIDDLE: f32 = H / 2.0;

/// Room around the pill inside its window, for the shadow.
pub const SHADOW_ROOM: f32 = 24.0;

/// The gap between the pill and the bottom of the screen's usable area, above the Dock.
pub const ABOVE_BOTTOM: f32 = 32.0;

pub const BAR_WIDTH: f32 = 3.0;
/// From one recording bar to the next.
pub const LIVE_PITCH: f32 = 6.0;
/// From one bar to the next once they have folded into the take: room for one rune each (D-24).
pub const CARVED_PITCH: f32 = 8.0;

pub const RUNE_WIDTH: f32 = 7.0;
pub const RUNE_HEIGHT: f32 = 17.0;
pub const RUNE_STROKE: f32 = 1.7;
