//! The dictated text in the Younger Futhark, the row Rister carves (docs/design.md, Overlay), and each
//! rune's strokes as the two Viking themes cut them.

/// A rune of the Younger Futhark, long-branch forms, named as the rune poems name them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rune {
    Fe,
    Ur,
    Oss,
    Reid,
    Kaun,
    Hagall,
    Naud,
    Isa,
    Ar,
    Sol,
    Tyr,
    Bjarkan,
    Madr,
    Logr,
}

/// One place in the carved row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mark {
    Rune(Rune),
    /// ᛫, between two words.
    WordDivider,
}

/// The text as runes, a word divider between words. What has no rune (digits, punctuation, letters
/// outside the table) is left out, and a word left with no rune leaves no divider either.
pub fn transliterate(text: &str) -> Vec<Mark> {
    let mut marks = Vec::new();
    for word in text.split_whitespace() {
        let before_word = marks.len();
        if before_word > 0 {
            marks.push(Mark::WordDivider);
        }
        let before_runes = marks.len();
        marks.extend(
            word.chars()
                .flat_map(char::to_lowercase)
                .filter_map(rune_for)
                .map(Mark::Rune),
        );
        if marks.len() == before_runes {
            marks.truncate(before_word);
        }
    }
    marks
}

/// design.md's table. A letter with an accent is carved as the letter without it, so a name like
/// «Zoë» or «kafé» keeps all its letters.
fn rune_for(letter: char) -> Option<Rune> {
    use Rune::*;
    Some(match letter {
        'a' | 'æ' | 'á' | 'à' | 'â' | 'ä' | 'ã' => Ar,
        'b' | 'p' => Bjarkan,
        'c' | 'g' | 'k' | 'q' | 'ç' => Kaun,
        'd' | 't' => Tyr,
        'e' | 'i' | 'j' | 'é' | 'è' | 'ê' | 'ë' | 'í' | 'ì' | 'î' | 'ï' => Isa,
        'f' => Fe,
        'h' => Hagall,
        'l' => Logr,
        'm' => Madr,
        'n' | 'ñ' => Naud,
        'o' | 'u' | 'v' | 'w' | 'y' | 'ø' | 'ó' | 'ò' | 'ô' | 'ö' | 'õ' | 'ú' | 'ù' | 'û' | 'ü' => {
            Ur
        }
        'r' => Reid,
        's' | 'x' | 'z' | 'ß' => Sol,
        'å' => Oss,
        _ => return None,
    })
}

impl Rune {
    #[cfg(test)]
    pub fn char(self) -> char {
        match self {
            Rune::Fe => 'ᚠ',
            Rune::Ur => 'ᚢ',
            Rune::Oss => 'ᚬ',
            Rune::Reid => 'ᚱ',
            Rune::Kaun => 'ᚴ',
            Rune::Hagall => 'ᚼ',
            Rune::Naud => 'ᚾ',
            Rune::Isa => 'ᛁ',
            Rune::Ar => 'ᛅ',
            Rune::Sol => 'ᛋ',
            Rune::Tyr => 'ᛏ',
            Rune::Bjarkan => 'ᛒ',
            Rune::Madr => 'ᛘ',
            Rune::Logr => 'ᛚ',
        }
    }

    /// The rune as strokes in a unit box, x to the right and y down.
    pub fn strokes(self) -> &'static [Stroke] {
        use End::{Free as F, Meets as M};
        // A struct literal, not a function call, so each table is a constant with a static lifetime.
        macro_rules! s {
            ($start:expr, $end:expr, $points:expr) => {
                Stroke {
                    start: $start,
                    end: $end,
                    points: $points,
                }
            };
        }
        match self {
            Rune::Fe => &[
                s!(F, F, &[(0.24, 1.0), (0.24, 0.0)]),
                s!(M, F, &[(0.24, 0.58), (0.86, 0.27)]),
                s!(M, F, &[(0.24, 0.31), (0.74, 0.06)]),
            ],
            Rune::Ur => &[
                s!(F, F, &[(0.2, 1.0), (0.2, 0.0)]),
                s!(M, F, &[(0.2, 0.05), (0.8, 0.47), (0.8, 1.0)]),
            ],
            Rune::Oss => &[
                s!(F, F, &[(0.5, 1.0), (0.5, 0.0)]),
                s!(F, F, &[(0.14, 0.17), (0.86, 0.45)]),
                s!(F, F, &[(0.14, 0.42), (0.86, 0.7)]),
            ],
            Rune::Reid => &[
                s!(F, F, &[(0.22, 1.0), (0.22, 0.0)]),
                s!(M, M, &[(0.22, 0.05), (0.78, 0.26), (0.22, 0.5)]),
                s!(M, F, &[(0.22, 0.5), (0.82, 1.0)]),
            ],
            Rune::Kaun => &[
                s!(F, F, &[(0.26, 1.0), (0.26, 0.0)]),
                s!(M, F, &[(0.26, 0.58), (0.82, 0.15)]),
            ],
            Rune::Hagall => &[
                s!(F, F, &[(0.5, 1.0), (0.5, 0.0)]),
                s!(F, F, &[(0.12, 0.3), (0.88, 0.7)]),
                s!(F, F, &[(0.12, 0.7), (0.88, 0.3)]),
            ],
            Rune::Naud => &[
                s!(F, F, &[(0.5, 1.0), (0.5, 0.0)]),
                s!(F, F, &[(0.15, 0.34), (0.85, 0.62)]),
            ],
            Rune::Isa => &[s!(F, F, &[(0.5, 1.0), (0.5, 0.0)])],
            Rune::Ar => &[
                s!(F, F, &[(0.5, 1.0), (0.5, 0.0)]),
                s!(F, F, &[(0.15, 0.62), (0.85, 0.34)]),
            ],
            Rune::Sol => &[s!(
                F,
                F,
                &[(0.22, 0.0), (0.22, 0.6), (0.78, 0.32), (0.78, 1.0)]
            )],
            Rune::Tyr => &[
                s!(F, M, &[(0.5, 1.0), (0.5, 0.1)]),
                s!(F, F, &[(0.1, 0.46), (0.5, 0.1), (0.9, 0.46)]),
            ],
            Rune::Bjarkan => &[
                s!(F, F, &[(0.22, 1.0), (0.22, 0.0)]),
                s!(M, M, &[(0.22, 0.05), (0.8, 0.27), (0.22, 0.5)]),
                s!(M, M, &[(0.22, 0.5), (0.8, 0.73), (0.22, 0.95)]),
            ],
            Rune::Madr => &[
                s!(F, F, &[(0.5, 1.0), (0.5, 0.0)]),
                s!(M, F, &[(0.5, 0.54), (0.1, 0.12)]),
                s!(M, F, &[(0.5, 0.54), (0.9, 0.12)]),
            ],
            Rune::Logr => &[
                s!(F, F, &[(0.26, 1.0), (0.26, 0.0)]),
                s!(M, F, &[(0.26, 0.05), (0.82, 0.42)]),
            ],
        }
    }
}

/// One cut of a rune: a polyline with miter joins.
#[derive(Debug)]
pub struct Stroke {
    pub start: End,
    pub end: End,
    pub points: &'static [(f32, f32)],
}

/// How a stroke's end is cut (CLAUDE.md, the stroke rule).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum End {
    /// Cut square: the stroke goes on half its width past the point.
    Free,
    /// Meets another stroke and stops butt on that stroke's centre line, at the point.
    Meets,
}

impl Stroke {
    /// The polyline scaled into a box of `width` × `height`, with each free end carried half the stroke
    /// width past its point, so a butt cap there reads as a square end.
    pub fn outline(&self, width: f32, height: f32, stroke_width: f32) -> Vec<(f32, f32)> {
        let mut points: Vec<(f32, f32)> = self
            .points
            .iter()
            .map(|&(x, y)| (x * width, y * height))
            .collect();
        let extend = |points: &mut Vec<(f32, f32)>, end: usize, inner: usize| {
            let (ex, ey) = points[end];
            let (ix, iy) = points[inner];
            let length = (ex - ix).hypot(ey - iy).max(f32::EPSILON);
            let reach = stroke_width / 2.0 / length;
            points[end] = (ex + (ex - ix) * reach, ey + (ey - iy) * reach);
        };
        let last = points.len() - 1;
        if self.start == End::Free {
            extend(&mut points, 0, 1);
        }
        if self.end == End::Free {
            extend(&mut points, last, last - 1);
        }
        points
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn runes(text: &str) -> String {
        transliterate(text)
            .into_iter()
            .map(|mark| match mark {
                Mark::Rune(rune) => rune.char(),
                Mark::WordDivider => '᛫',
            })
            .collect()
    }

    const ALL: [Rune; 14] = [
        Rune::Fe,
        Rune::Ur,
        Rune::Oss,
        Rune::Reid,
        Rune::Kaun,
        Rune::Hagall,
        Rune::Naud,
        Rune::Isa,
        Rune::Ar,
        Rune::Sol,
        Rune::Tyr,
        Rune::Bjarkan,
        Rune::Madr,
        Rune::Logr,
    ];

    #[test]
    fn every_letter_of_the_table_has_its_rune() {
        let table = [
            ("a", "ᛅ"),
            ("b", "ᛒ"),
            ("p", "ᛒ"),
            ("c", "ᚴ"),
            ("g", "ᚴ"),
            ("k", "ᚴ"),
            ("q", "ᚴ"),
            ("d", "ᛏ"),
            ("t", "ᛏ"),
            ("e", "ᛁ"),
            ("i", "ᛁ"),
            ("j", "ᛁ"),
            ("f", "ᚠ"),
            ("h", "ᚼ"),
            ("l", "ᛚ"),
            ("m", "ᛘ"),
            ("n", "ᚾ"),
            ("o", "ᚢ"),
            ("u", "ᚢ"),
            ("v", "ᚢ"),
            ("w", "ᚢ"),
            ("y", "ᚢ"),
            ("ø", "ᚢ"),
            ("r", "ᚱ"),
            ("s", "ᛋ"),
            ("x", "ᛋ"),
            ("z", "ᛋ"),
            ("æ", "ᛅ"),
            ("å", "ᚬ"),
        ];
        for (letter, rune) in table {
            assert_eq!(runes(letter), rune, "{letter}");
            assert_eq!(
                runes(&letter.to_uppercase()),
                rune,
                "{letter} in upper case"
            );
        }
    }

    #[test]
    fn words_are_divided_and_punctuation_is_left_out() {
        assert_eq!(runes("Kan du sende meg"), "ᚴᛅᚾ᛫ᛏᚢ᛫ᛋᛁᚾᛏᛁ᛫ᛘᛁᚴ");
        assert_eq!(runes("i går, så"), "ᛁ᛫ᚴᚬᚱ᛫ᛋᚬ");
        assert_eq!(runes("lunsj."), "ᛚᚢᚾᛋᛁ");
    }

    #[test]
    fn a_word_with_no_rune_leaves_no_divider() {
        assert_eq!(runes("Møte kl. 14 – i dag"), "ᛘᚢᛏᛁ᛫ᚴᛚ᛫ᛁ᛫ᛏᛅᚴ");
        assert_eq!(runes("2026 — 42"), "");
        assert_eq!(runes("   "), "");
        assert_eq!(runes("— hei"), "ᚼᛁᛁ");
    }

    #[test]
    fn a_hyphen_does_not_divide_a_word() {
        assert_eq!(runes("pull-requesten"), "ᛒᚢᛚᛚᚱᛁᚴᚢᛁᛋᛏᛁᚾ");
    }

    #[test]
    fn line_breaks_and_tabs_divide_words_like_spaces() {
        assert_eq!(runes("hei\nder\tdu"), "ᚼᛁᛁ᛫ᛏᛁᚱ᛫ᛏᚢ");
    }

    #[test]
    fn accented_letters_are_carved_as_their_letter() {
        assert_eq!(runes("kafé"), runes("kafe"));
        assert_eq!(runes("Zoë Müller"), runes("Zoe Muller"));
    }

    #[test]
    fn every_point_lies_in_the_unit_box() {
        for rune in ALL {
            for stroke in rune.strokes() {
                assert!(stroke.points.len() >= 2, "{rune:?}");
                for &(x, y) in stroke.points {
                    assert!(
                        (0.0..=1.0).contains(&x) && (0.0..=1.0).contains(&y),
                        "{rune:?}"
                    );
                }
            }
        }
    }

    fn distance_to_segment(p: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let t = (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
        (p.0 - a.0 - t * dx).hypot(p.1 - a.1 - t * dy)
    }

    /// The stroke rule: an end that meets another stroke ends on that stroke's centre line.
    #[test]
    fn an_end_that_meets_lies_on_another_strokes_centre_line() {
        for rune in ALL {
            let strokes = rune.strokes();
            for (i, stroke) in strokes.iter().enumerate() {
                let ends = [
                    (stroke.start, stroke.points[0]),
                    (stroke.end, *stroke.points.last().unwrap()),
                ];
                for (end, point) in ends {
                    if end != End::Meets {
                        continue;
                    }
                    let nearest = strokes
                        .iter()
                        .enumerate()
                        .filter(|&(j, _)| j != i)
                        .flat_map(|(_, other)| other.points.windows(2))
                        .map(|segment| distance_to_segment(point, segment[0], segment[1]))
                        .fold(f32::INFINITY, f32::min);
                    assert!(
                        nearest < 1e-4,
                        "{rune:?}: an end at {point:?} meets nothing"
                    );
                }
            }
        }
    }

    #[test]
    fn a_free_end_reaches_half_the_stroke_width_past_its_point() {
        let stem = &Rune::Isa.strokes()[0];
        assert_eq!(stem.outline(7.0, 17.0, 2.0), vec![(3.5, 18.0), (3.5, -1.0)]);
        let branch = &Rune::Kaun.strokes()[1];
        let outline = branch.outline(1.0, 1.0, 0.1);
        assert_eq!(
            outline[0],
            (0.26, 0.58),
            "the end that meets stays on the stem"
        );
        let (x, y) = outline[1];
        let reach = (x - 0.82).hypot(y - 0.15);
        assert!((reach - 0.05).abs() < 1e-5, "{reach}");
    }
}
