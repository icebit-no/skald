//! The item's icon as pixels, for a platform that takes a bitmap: ᚨ in one colour on a transparent square,
//! with a dot while a take records or is transcribed. The strokes are the menu bar icon's
//! (`assets/icon/render.py`): on an 18-unit square, 2 units wide, the stave with square ends, the twigs
//! starting butt on its centre line and ending square. They are drawn anew at each size, so the rune is
//! sharp at every scale of the display.

const GRID: f32 = 18.0;
const STROKE: f32 = 2.0;

/// Samples per pixel along each axis; the share of them inside a stroke is the pixel's coverage.
const SAMPLES: u32 = 4;

/// The dot sits in the free corner below the twigs, clear of the rune.
const DOT_CENTRE: (f32, f32) = (14.5, 14.5);
const DOT_RADIUS: f32 = 2.5;

struct Stroke {
    from: (f32, f32),
    to: (f32, f32),
    starts_on_stroke: bool,
}

const ANSUZ: [Stroke; 3] = [
    Stroke {
        from: (7.0, 3.0),
        to: (7.0, 15.0),
        starts_on_stroke: false,
    },
    Stroke {
        from: (7.0, 3.75),
        to: (11.5, 7.25),
        starts_on_stroke: true,
    },
    Stroke {
        from: (7.0, 7.75),
        to: (11.5, 11.25),
        starts_on_stroke: true,
    },
];

impl Stroke {
    fn covers(&self, x: f32, y: f32) -> bool {
        let (dx, dy) = (self.to.0 - self.from.0, self.to.1 - self.from.1);
        let length = (dx * dx + dy * dy).sqrt();
        let (ux, uy) = (dx / length, dy / length);
        let (px, py) = (x - self.from.0, y - self.from.1);
        let along = px * ux + py * uy;
        let across = (px * -uy + py * ux).abs();
        let start = if self.starts_on_stroke {
            0.0
        } else {
            -STROKE / 2.0
        };
        across <= STROKE / 2.0 && along >= start && along <= length + STROKE / 2.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Rgb(pub u8, pub u8, pub u8);

/// `size` × `size` pixels, the top row first, each blue, green, red and alpha, the colour not multiplied
/// by the alpha. The dot is drawn over the rune where they would meet.
pub(crate) fn pixels(size: u32, rune: Rgb, dot: Option<Rgb>) -> Vec<[u8; 4]> {
    let scale = GRID / size as f32;
    let samples = (SAMPLES * SAMPLES) as f32;
    let mut out = Vec::with_capacity((size * size) as usize);
    for row in 0..size {
        for column in 0..size {
            let (mut on_rune, mut on_dot) = (0u32, 0u32);
            for sy in 0..SAMPLES {
                for sx in 0..SAMPLES {
                    let x = (column as f32 + (sx as f32 + 0.5) / SAMPLES as f32) * scale;
                    let y = (row as f32 + (sy as f32 + 0.5) / SAMPLES as f32) * scale;
                    let (cx, cy) = (x - DOT_CENTRE.0, y - DOT_CENTRE.1);
                    if dot.is_some() && cx * cx + cy * cy <= DOT_RADIUS * DOT_RADIUS {
                        on_dot += 1;
                    } else if ANSUZ.iter().any(|stroke| stroke.covers(x, y)) {
                        on_rune += 1;
                    }
                }
            }
            let colour = match dot {
                Some(dot) if on_dot >= on_rune && on_dot > 0 => dot,
                _ => rune,
            };
            let alpha = ((on_rune + on_dot) as f32 / samples * 255.0).round() as u8;
            out.push([colour.2, colour.1, colour.0, alpha]);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const WHITE: Rgb = Rgb(255, 255, 255);
    const RED: Rgb = Rgb(226, 102, 78);

    fn at(pixels: &[[u8; 4]], size: u32, column: u32, row: u32) -> [u8; 4] {
        pixels[(row * size + column) as usize]
    }

    #[test]
    fn at_eighteen_pixels_the_stave_is_two_whole_columns_with_square_ends() {
        let pixels = pixels(18, WHITE, None);
        for row in 2..16 {
            assert_eq!(at(&pixels, 18, 6, row)[3], 255, "row {row}");
            assert_eq!(at(&pixels, 18, 7, row)[3], 255, "row {row}");
        }
        assert_eq!(at(&pixels, 18, 6, 1)[3], 0, "above the stave");
        assert_eq!(at(&pixels, 18, 6, 16)[3], 0, "below the stave");
        assert_eq!(at(&pixels, 18, 5, 8)[3], 0, "left of the stave");
    }

    #[test]
    fn the_twigs_end_short_of_the_right_edge_and_nothing_pokes_out_left_of_the_stave() {
        let pixels = pixels(36, WHITE, None);
        for row in 0..36 {
            assert_eq!(
                at(&pixels, 36, 11, row)[3],
                0,
                "row {row}, left of the stave"
            );
            for column in 27..36 {
                assert_eq!(at(&pixels, 36, column, row)[3], 0, "{column}, {row}");
            }
        }
        // The upper twig crosses this pixel on its way down to the right.
        assert!(at(&pixels, 36, 18, 10)[3] > 200);
    }

    #[test]
    fn the_dot_comes_only_when_asked_and_stays_clear_of_the_rune() {
        let plain = pixels(32, WHITE, None);
        let dotted = pixels(32, WHITE, Some(RED));
        let centre = (DOT_CENTRE.0 / GRID * 32.0) as u32;
        assert_eq!(at(&plain, 32, centre, centre)[3], 0);
        assert_eq!(at(&dotted, 32, centre, centre), [78, 102, 226, 255]);
        for (a, b) in plain.iter().zip(&dotted) {
            if a[3] > 0 {
                assert_eq!(a, b, "the rune is the same with the dot");
            }
        }
    }

    #[test]
    fn the_colour_is_the_one_asked_for_and_the_corners_are_clear() {
        for size in [16, 20, 24, 32] {
            let pixels = pixels(size, Rgb(0, 0, 0), None);
            assert_eq!(pixels.len(), (size * size) as usize);
            assert!(pixels.iter().all(|p| p[..3] == [0, 0, 0]));
            assert_eq!(at(&pixels, size, 0, 0)[3], 0);
            assert_eq!(at(&pixels, size, size - 1, 0)[3], 0);
            assert!(pixels.iter().filter(|p| p[3] > 0).count() > (size * 2) as usize);
            assert!(pixels.iter().any(|p| p[3] == 255), "{size}");
        }
    }
}
