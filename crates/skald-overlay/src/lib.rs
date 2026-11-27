//! The overlay: the pill near the bottom of the active screen that shows a take as it happens, in the
//! theme chosen under «Overleggets utseende» (docs/design.md, Overlay). The app drives it through
//! [`Overlay`], one call per thing that happens in a take.

mod bars;
mod chisel;
mod draw;
mod futhark;
mod geometry;
mod native;
mod overlay;
mod paint;
mod pill;
mod text;
mod texture;
mod theme;

pub use overlay::Overlay;
pub use pill::Motion;
