//! The faces of the two Viking plates, rendered once per scale on the CPU: dark oak for Runepinne and
//! hammered iron for Smidd jern, with the light, the cut edges and, on the stick, the end bands. GPUI has
//! no noise, no path clipping for images and no radial light, so what is fixed on the plate is drawn here
//! and only what moves is drawn by GPUI.
//!
//! The noise and the recipes follow the drawings (the private artifact of the overlay, round 2).

use crate::geometry::{H, W};

/// Room around the plate inside the picture, for the half of the outline that lies outside it.
pub const MARGIN: f32 = 1.0;

/// A picture in straight-alpha BGRA, the order GPUI's images take.
pub struct Picture {
    pub width: u32,
    pub height: u32,
    pub bgra: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Face {
    Oak,
    Iron,
}

/// The plate's face at `scale` pixels per point, covering the plate and [`MARGIN`] around it.
pub fn render(face: Face, scale: f32) -> Picture {
    let width = ((W + 2.0 * MARGIN) * scale).round() as u32;
    let height = ((H + 2.0 * MARGIN) * scale).round() as u32;
    let mut bgra = Vec::with_capacity((width * height * 4) as usize);
    let pixel = 1.0 / scale;
    for row in 0..height {
        for column in 0..width {
            let x = (column as f32 + 0.5) / scale - MARGIN;
            let y = (row as f32 + 0.5) / scale - MARGIN;
            let color = match face {
                Face::Oak => oak(x, y, pixel),
                Face::Iron => iron(x, y, pixel),
            };
            bgra.extend_from_slice(&color.bgra());
        }
    }
    Picture {
        width,
        height,
        bgra,
    }
}

/// The stick's outline: a rectangle whose corners are cut off with a knife, none quite alike.
pub const STICK: [(f32, f32); 8] = [
    (7.4, 0.0),
    (W - 6.6, 0.3),
    (W, 7.0),
    (W - 0.3, H - 7.6),
    (W - 6.4, H),
    (7.2, H - 0.3),
    (0.0, H - 7.8),
    (0.3, 6.8),
];

fn oak(x: f32, y: f32, pixel: f32) -> Color {
    let gy = y + 1.6 * (x * 0.011 + 1.3).sin() + 3.0 * (fbm(x * 0.004, y * 0.03, 41, 2) - 0.5);
    let ring = fbm(x * 0.01, gy * 0.5, 42, 3);
    let streak = fbm(x * 0.045, gy * 2.4, 43, 2);
    let pore = value_noise(x * 0.3, gy * 3.3, 44);
    let fleck = value_noise(x * 0.8 + gy * 0.25, gy * 0.45, 45);
    let mut tone = 0.8 + 0.5 * (ring - 0.5) + 0.22 * (streak - 0.5);
    if pore > 0.74 {
        tone *= 1.0 - (pore - 0.74) * 1.5;
    }
    if fleck > 0.88 {
        tone *= 1.0 + (fleck - 0.88) * 1.3;
    }
    let mut face = Color::rgb(76.0 * tone, 48.0 * tone, 27.0 * tone);

    // The light from above and the stick's thickness.
    face.over(gradient(
        y / H,
        &[
            (0.0, Color::rgba(255.0, 226.0, 180.0, 0.1)),
            (0.45, Color::rgba(255.0, 226.0, 180.0, 0.0)),
            (1.0, Color::rgba(0.0, 0.0, 0.0, 0.26)),
        ],
    ));
    // Long whittling facets: the knife left the face in flat strips, not one plane.
    let band = |top: f32, height: f32| coverage(rectangle(x, y, 0.0, top, W, height), pixel);
    face.over(Color::rgba(255.0, 230.0, 190.0, 0.04 * band(8.5, 10.5)));
    face.over(Color::rgba(0.0, 0.0, 0.0, 0.055 * band(32.0, 9.5)));
    face.over(Color::rgba(0.0, 0.0, 0.0, 0.26 * band(H - 3.2, 3.2)));
    face.over(Color::rgba(255.0, 222.0, 172.0, 0.1 * band(H - 3.7, 0.5)));
    // Knife facets where the corners are cut off: lit at the top left, in shade at the bottom right.
    let facets: [([(f32, f32); 4], Color); 4] = [
        (
            [(0.0, 6.8), (7.4, 0.0), (13.4, 0.0), (0.0, 12.6)],
            Color::rgba(255.0, 228.0, 184.0, 0.2),
        ),
        (
            [(W - 6.6, 0.0), (W, 7.0), (W, 12.8), (W - 12.4, 0.0)],
            Color::rgba(255.0, 228.0, 184.0, 0.08),
        ),
        (
            [(W, H - 7.6), (W - 6.4, H), (W - 12.2, H), (W, H - 13.6)],
            Color::rgba(0.0, 0.0, 0.0, 0.26),
        ),
        (
            [(7.2, H), (0.0, H - 7.8), (0.0, H - 13.6), (13.0, H)],
            Color::rgba(0.0, 0.0, 0.0, 0.14),
        ),
    ];
    for (corners, mut color) in facets {
        color.a *= coverage(polygon(x, y, &corners), pixel);
        face.over(color);
    }
    bevel(
        &mut face,
        |dx, dy| polygon(x - dx, y - dy, &STICK),
        Color::rgba(255.0, 224.0, 176.0, 0.3),
        Color::rgba(0.0, 0.0, 0.0, 0.5),
        2.2,
        pixel,
    );
    // End bands as on bone and antler combs: cross lines with saltires between. Old cuts, gone dark.
    for (left, right) in [(8.5, 21.5), (W - 21.5, W - 8.5)] {
        let (top, bottom, cells) = (6.0, H - 8.5, 4);
        let cell = (bottom - top) / cells as f32;
        let mut cuts = vec![
            ((left, top), (left, bottom)),
            ((right, top), (right, bottom)),
        ];
        for i in 0..cells {
            let a = top + i as f32 * cell + 1.1;
            let b = top + (i + 1) as f32 * cell - 1.1;
            cuts.push(((left + 2.2, a), (right - 2.2, b)));
            cuts.push(((right - 2.2, a), (left + 2.2, b)));
        }
        for (from, to) in cuts {
            let wall = coverage(segment(x, y, from, to, 1.15), pixel);
            face.over(Color::rgba(14.0, 7.0, 2.0, 0.78 * wall));
            let lit = coverage(segment(x - 0.55, y - 0.55, from, to, 0.55), pixel);
            face.over(Color::rgba(255.0, 220.0, 170.0, 0.22 * lit));
        }
    }
    outlined(face, polygon(x, y, &STICK), pixel)
}

fn iron(x: f32, y: f32, pixel: f32) -> Color {
    let dent = |a: f32, b: f32| {
        let d = (worley(a, b, 10.0, 11) * 1.5).min(1.0);
        d * d
    };
    let shade = dent(x - 0.8, y - 0.8) - dent(x + 0.8, y + 0.8);
    let pit = value_noise(x * 2.1, y * 2.1, 23);
    let grain = hash((x * 3.0).floor() as i32, (y * 3.0).floor() as i32, 5);
    let mut value = 0.12 + (fbm(x * 0.035, y * 0.06, 21, 3) - 0.5) * 0.05 - shade * 0.03
        + (grain - 0.5) * 0.012;
    if pit > 0.82 {
        value *= 1.0 - (pit - 0.82) * 1.7;
    }
    let mut face = Color::rgb(value * 1.05 * 255.0, value * 255.0, value * 0.95 * 255.0);
    face.over(gradient(
        y / H,
        &[
            (0.0, Color::rgba(226.0, 232.0, 240.0, 0.07)),
            (1.0, Color::rgba(0.0, 0.0, 0.0, 0.22)),
        ],
    ));
    bevel(
        &mut face,
        |dx, dy| capsule(x - dx, y - dy),
        Color::rgba(240.0, 238.0, 230.0, 0.2),
        Color::rgba(0.0, 0.0, 0.0, 0.6),
        1.6,
        pixel,
    );
    outlined(face, capsule(x, y), pixel)
}

/// The cut edge: the outline drawn shifted down-right in a light colour and up-left in a dark one, each
/// seen only inside the plate, so the top and left edges catch the light.
fn bevel(
    face: &mut Color,
    distance: impl Fn(f32, f32) -> f32,
    light: Color,
    dark: Color,
    width: f32,
    pixel: f32,
) {
    for (sign, mut color) in [(1.0, light), (-1.0, dark)] {
        let along = distance(sign * width * 0.45, sign * width * 0.5).abs();
        color.a *= coverage(along - width / 2.0, pixel);
        face.over(color);
    }
}

/// The face inside the plate's outline, with a thin dark line on the outline itself.
fn outlined(face: Color, distance: f32, pixel: f32) -> Color {
    let mut picture = face;
    picture.a = coverage(distance, pixel);
    let line = coverage(distance.abs() - 0.375, pixel);
    picture.under(Color::rgba(0.0, 0.0, 0.0, 0.75 * line))
}

/// The share of a pixel inside a shape, from the shape's signed distance in points (negative inside).
fn coverage(distance: f32, pixel: f32) -> f32 {
    (0.5 - distance / pixel).clamp(0.0, 1.0)
}

/// The signed distance to the plate of Smidd jern, a capsule.
fn capsule(x: f32, y: f32) -> f32 {
    let radius = H / 2.0;
    let qx = (x - W / 2.0).abs() - (W / 2.0 - radius);
    let qy = (y - H / 2.0).abs() - (H / 2.0 - radius);
    qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - radius
}

fn rectangle(x: f32, y: f32, left: f32, top: f32, width: f32, height: f32) -> f32 {
    let dx = (x - (left + width / 2.0)).abs() - width / 2.0;
    let dy = (y - (top + height / 2.0)).abs() - height / 2.0;
    dx.max(dy)
}

/// The signed distance to a convex polygon given clockwise on screen (y down).
fn polygon(x: f32, y: f32, corners: &[(f32, f32)]) -> f32 {
    let mut nearest = f32::INFINITY;
    let mut inside = true;
    for (i, &(ax, ay)) in corners.iter().enumerate() {
        let (bx, by) = corners[(i + 1) % corners.len()];
        let (ex, ey) = (bx - ax, by - ay);
        let t = (((x - ax) * ex + (y - ay) * ey) / (ex * ex + ey * ey)).clamp(0.0, 1.0);
        nearest = nearest.min((x - ax - t * ex).hypot(y - ay - t * ey));
        if ex * (y - ay) - ey * (x - ax) < 0.0 {
            inside = false;
        }
    }
    if inside { -nearest } else { nearest }
}

/// The signed distance to a straight cut of `width` with butt ends.
fn segment(x: f32, y: f32, from: (f32, f32), to: (f32, f32), width: f32) -> f32 {
    let (ex, ey) = (to.0 - from.0, to.1 - from.1);
    let length = ex.hypot(ey);
    let along = ((x - from.0) * ex + (y - from.1) * ey) / length;
    let across = ((x - from.0) * ey - (y - from.1) * ex).abs() / length;
    (across - width / 2.0).max((-along).max(along - length))
}

fn gradient(t: f32, stops: &[(f32, Color)]) -> Color {
    let t = t.clamp(0.0, 1.0);
    for pair in stops.windows(2) {
        let ((t0, c0), (t1, c1)) = (pair[0], pair[1]);
        if t <= t1 {
            let k = (t - t0) / (t1 - t0);
            return Color::rgba(
                c0.r + (c1.r - c0.r) * k,
                c0.g + (c1.g - c0.g) * k,
                c0.b + (c1.b - c0.b) * k,
                c0.a + (c1.a - c0.a) * k,
            );
        }
    }
    stops[stops.len() - 1].1
}

/// A colour with channels from 0 to 255 and alpha from 0 to 1.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Color {
    r: f32,
    g: f32,
    b: f32,
    a: f32,
}

impl Color {
    fn rgb(r: f32, g: f32, b: f32) -> Self {
        Self::rgba(r, g, b, 1.0)
    }

    fn rgba(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    /// Lays `top` over this colour.
    fn over(&mut self, top: Color) {
        *self = top.composite(*self);
    }

    /// Puts `bottom` under this colour.
    fn under(self, bottom: Color) -> Color {
        self.composite(bottom)
    }

    fn composite(self, bottom: Color) -> Color {
        let a = self.a + bottom.a * (1.0 - self.a);
        if a <= 0.0 {
            return Color::rgba(0.0, 0.0, 0.0, 0.0);
        }
        let mix = |top: f32, under: f32| (top * self.a + under * bottom.a * (1.0 - self.a)) / a;
        Color::rgba(
            mix(self.r, bottom.r),
            mix(self.g, bottom.g),
            mix(self.b, bottom.b),
            a,
        )
    }

    fn bgra(self) -> [u8; 4] {
        let channel = |value: f32| value.round().clamp(0.0, 255.0) as u8;
        [
            channel(self.b),
            channel(self.g),
            channel(self.r),
            channel(self.a * 255.0),
        ]
    }
}

/// A hash of a lattice point to 0..1.
fn hash(x: i32, y: i32, seed: i32) -> f32 {
    let mut h = x.wrapping_mul(374_761_393)
        ^ y.wrapping_mul(668_265_263)
        ^ seed.wrapping_mul(1_442_695_041);
    h = (h ^ ((h as u32 >> 13) as i32)).wrapping_mul(1_274_126_177);
    (h ^ ((h as u32 >> 16) as i32)) as u32 as f32 / 4_294_967_296.0
}

fn value_noise(x: f32, y: f32, seed: i32) -> f32 {
    let (xi, yi) = (x.floor(), y.floor());
    let (fx, fy) = (x - xi, y - yi);
    let (u, v) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy));
    let (xi, yi) = (xi as i32, yi as i32);
    let a = hash(xi, yi, seed);
    let b = hash(xi + 1, yi, seed);
    let c = hash(xi, yi + 1, seed);
    let d = hash(xi + 1, yi + 1, seed);
    a + (b - a) * u + (c - a) * v + (a - b - c + d) * u * v
}

fn fbm(x: f32, y: f32, seed: i32, octaves: i32) -> f32 {
    let (mut sum, mut amplitude, mut frequency, mut total) = (0.0, 0.5, 1.0, 0.0);
    for octave in 0..octaves {
        sum += amplitude * value_noise(x * frequency, y * frequency, seed + octave * 17);
        total += amplitude;
        amplitude *= 0.5;
        frequency *= 2.0;
    }
    sum / total
}

/// The distance to the nearest of a jittered grid of points, in cells.
fn worley(x: f32, y: f32, cell: f32, seed: i32) -> f32 {
    let (gx, gy) = ((x / cell).floor() as i32, (y / cell).floor() as i32);
    let mut nearest = f32::INFINITY;
    for dy in -1..=1 {
        for dx in -1..=1 {
            let (cx, cy) = (gx + dx, gy + dy);
            let px = (cx as f32 + hash(cx, cy, seed)) * cell;
            let py = (cy as f32 + hash(cx, cy, seed + 1)) * cell;
            nearest = nearest.min((x - px).hypot(y - py));
        }
    }
    nearest / cell
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alpha(picture: &Picture, x: f32, y: f32, scale: f32) -> u8 {
        let column = ((x + MARGIN) * scale) as u32;
        let row = ((y + MARGIN) * scale) as u32;
        picture.bgra[((row * picture.width + column) * 4 + 3) as usize]
    }

    #[test]
    fn the_picture_covers_the_plate_and_its_margin() {
        let picture = render(Face::Oak, 2.0);
        assert_eq!((picture.width, picture.height), (864, 120));
        assert_eq!(picture.bgra.len(), 864 * 120 * 4);
    }

    #[test]
    fn the_face_is_opaque_inside_and_clear_outside() {
        for face in [Face::Oak, Face::Iron] {
            let picture = render(face, 1.0);
            assert_eq!(alpha(&picture, W / 2.0, H / 2.0, 1.0), 255, "{face:?}");
            assert_eq!(alpha(&picture, -0.9, -0.9, 1.0), 0, "{face:?}");
            assert_eq!(alpha(&picture, 1.0, 1.0, 1.0), 0, "{face:?}: a cut corner");
        }
    }

    #[test]
    fn the_same_scale_gives_the_same_picture() {
        assert_eq!(render(Face::Iron, 0.5).bgra, render(Face::Iron, 0.5).bgra);
    }

    #[test]
    fn the_noise_stays_in_range() {
        for i in 0..1000 {
            let (x, y) = (i as f32 * 0.731 - 200.0, i as f32 * 0.317 - 50.0);
            assert!((0.0..1.0).contains(&hash(i, -i, 7)));
            assert!((0.0..=1.0).contains(&value_noise(x, y, 3)));
            assert!((0.0..=1.0).contains(&fbm(x, y, 3, 3)));
        }
    }

    #[test]
    fn distances_are_negative_inside_and_positive_outside() {
        assert!(capsule(W / 2.0, H / 2.0) < 0.0);
        assert!((capsule(W / 2.0, -2.0) - 2.0).abs() < 1e-4);
        assert!(capsule(1.0, 1.0) > 0.0, "the capsule's corner is round");
        assert!(polygon(W / 2.0, H / 2.0, &STICK) < 0.0);
        assert!(polygon(0.5, 0.5, &STICK) > 0.0, "the stick's corner is cut");
        assert!((segment(5.0, 1.0, (0.0, 0.0), (10.0, 0.0), 1.0) - 0.5).abs() < 1e-6);
        assert!(
            segment(-1.0, 0.0, (0.0, 0.0), (10.0, 0.0), 1.0) > 0.0,
            "butt ends"
        );
    }

    #[test]
    fn colours_composite_as_a_canvas_does() {
        let mut white = Color::rgb(255.0, 255.0, 255.0);
        white.over(Color::rgba(0.0, 0.0, 0.0, 0.5));
        assert_eq!(white.bgra(), [128, 128, 128, 255]);
        let clear = Color::rgba(0.0, 0.0, 0.0, 0.0).under(Color::rgba(10.0, 20.0, 30.0, 0.5));
        assert_eq!(clear.bgra(), [30, 20, 10, 128]);
    }
}
