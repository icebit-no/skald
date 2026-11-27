//! Word errors and spacing errors between a reference and the model's text, both as the `normalise`
//! module gives their words.
//!
//! **Word errors** are the fewest substitutions, deletions and insertions that turn the reference's words
//! into the text's, the usual word error rate's count.
//!
//! **Spacing errors** are read from that same alignment. Besides the three usual steps it has two more:
//!
//! - a **split**: one reference word against two or more consecutive words of the text that join to it
//!   («arbeidsdag» → «arbeids dag», «pull-requesten» → «pull requesten»);
//! - a **join**: two or more consecutive reference words against one word of the text that they join to
//!   («i dag» → «idag»).
//!
//! Words are joined without their hyphens on both sides, so «pull requesten» joins to «pull-requesten»
//! and «pull request» to «pull-request». A split into k words costs k word errors, as one substitution
//! and k − 1 insertions would; a join of k words costs k, as one substitution and k − 1 deletions. So the
//! two steps never change the count of word errors; of the alignments with the fewest word errors, the
//! one with the most splits and joins is taken. The word errors inside splits and joins are the spacing
//! part of the word error count, and the rest are the other errors.

use std::ops::Range;

/// One step of the alignment, with indices into the reference's and the text's words.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    Match {
        reference: usize,
        text: usize,
    },
    Substitute {
        reference: usize,
        text: usize,
    },
    /// A reference word the text lacks.
    Delete {
        reference: usize,
    },
    /// A text word with no reference word.
    Insert {
        text: usize,
    },
    Split {
        reference: usize,
        text: Range<usize>,
    },
    Join {
        reference: Range<usize>,
        text: usize,
    },
}

impl Step {
    fn word_errors(&self) -> usize {
        match self {
            Self::Match { .. } => 0,
            Self::Substitute { .. } | Self::Delete { .. } | Self::Insert { .. } => 1,
            Self::Split { text, .. } => text.len(),
            Self::Join { reference, .. } => reference.len(),
        }
    }

    fn is_spacing(&self) -> bool {
        matches!(self, Self::Split { .. } | Self::Join { .. })
    }
}

/// A reference and a text, their words and the alignment between them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Score {
    pub reference: Vec<String>,
    pub text: Vec<String>,
    pub steps: Vec<Step>,
}

impl Score {
    pub fn new(reference: &str, text: &str) -> Self {
        let reference = crate::normalise::words(reference);
        let text = crate::normalise::words(text);
        let steps = align(&reference, &text);
        Self {
            reference,
            text,
            steps,
        }
    }

    pub fn word_errors(&self) -> usize {
        self.steps.iter().map(Step::word_errors).sum()
    }

    /// The word errors inside splits and joins.
    pub fn spacing_word_errors(&self) -> usize {
        self.steps
            .iter()
            .filter(|step| step.is_spacing())
            .map(Step::word_errors)
            .sum()
    }

    pub fn splits(&self) -> usize {
        self.steps
            .iter()
            .filter(|step| matches!(step, Step::Split { .. }))
            .count()
    }

    pub fn joins(&self) -> usize {
        self.steps
            .iter()
            .filter(|step| matches!(step, Step::Join { .. }))
            .count()
    }
}

/// A word as splits and joins compare it.
fn spacing_key(word: &str) -> String {
    word.replace('-', "")
}

/// The best alignment so far of a prefix of the reference with a prefix of the text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Cost {
    word_errors: usize,
    spacing: usize,
}

impl Cost {
    fn better_than(self, other: Cost) -> bool {
        self.word_errors < other.word_errors
            || (self.word_errors == other.word_errors && self.spacing > other.spacing)
    }
}

/// How a cell of the table was reached: the last step, by the length of reference and text it covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Last {
    Start,
    Diagonal,
    Delete,
    Insert,
    Split(usize),
    Join(usize),
}

/// The alignment with the fewest word errors and, among those, the most splits and joins.
pub fn align(reference: &[String], text: &[String]) -> Vec<Step> {
    let (n, m) = (reference.len(), text.len());
    let ref_keys: Vec<String> = reference.iter().map(|w| spacing_key(w)).collect();
    let text_keys: Vec<String> = text.iter().map(|w| spacing_key(w)).collect();

    let width = m + 1;
    let mut cost = vec![
        Cost {
            word_errors: usize::MAX,
            spacing: 0,
        };
        (n + 1) * width
    ];
    let mut last = vec![Last::Start; (n + 1) * width];
    cost[0] = Cost {
        word_errors: 0,
        spacing: 0,
    };

    for i in 0..=n {
        for j in 0..=m {
            if i == 0 && j == 0 {
                continue;
            }
            let mut best = Cost {
                word_errors: usize::MAX,
                spacing: 0,
            };
            let mut how = Last::Start;
            let mut consider = |from: Cost, errors: usize, spacing: usize, step: Last| {
                let candidate = Cost {
                    word_errors: from.word_errors + errors,
                    spacing: from.spacing + spacing,
                };
                if candidate.better_than(best) {
                    best = candidate;
                    how = step;
                }
            };
            if i > 0 && j > 0 {
                let errors = usize::from(reference[i - 1] != text[j - 1]);
                consider(cost[(i - 1) * width + j - 1], errors, 0, Last::Diagonal);
            }
            if i > 0 {
                consider(cost[(i - 1) * width + j], 1, 0, Last::Delete);
            }
            if j > 0 {
                consider(cost[i * width + j - 1], 1, 0, Last::Insert);
            }
            if i > 0
                && let Some(k) = joined_from_end(&ref_keys[i - 1], &text_keys[..j])
            {
                consider(cost[(i - 1) * width + j - k], k, 1, Last::Split(k));
            }
            if j > 0
                && let Some(k) = joined_from_end(&text_keys[j - 1], &ref_keys[..i])
            {
                consider(cost[(i - k) * width + j - 1], k, 1, Last::Join(k));
            }
            cost[i * width + j] = best;
            last[i * width + j] = how;
        }
    }

    let mut steps = Vec::with_capacity(n.max(m));
    let (mut i, mut j) = (n, m);
    while i > 0 || j > 0 {
        let step = match last[i * width + j] {
            Last::Diagonal if reference[i - 1] == text[j - 1] => Step::Match {
                reference: i - 1,
                text: j - 1,
            },
            Last::Diagonal => Step::Substitute {
                reference: i - 1,
                text: j - 1,
            },
            Last::Delete => Step::Delete { reference: i - 1 },
            Last::Insert => Step::Insert { text: j - 1 },
            Last::Split(k) => Step::Split {
                reference: i - 1,
                text: j - k..j,
            },
            Last::Join(k) => Step::Join {
                reference: i - k..i,
                text: j - 1,
            },
            Last::Start => unreachable!("every cell but the first is reached by a step"),
        };
        match &step {
            Step::Match { .. } | Step::Substitute { .. } => (i, j) = (i - 1, j - 1),
            Step::Delete { .. } => i -= 1,
            Step::Insert { .. } => j -= 1,
            Step::Split { text, .. } => (i, j) = (i - 1, j - text.len()),
            Step::Join { reference, .. } => (i, j) = (i - reference.len(), j - 1),
        }
        steps.push(step);
    }
    steps.reverse();
    steps
}

/// The number k ≥ 2 for which the last k of `words`, joined, are `whole`, if there is one. Words are
/// never empty, so there is at most one.
fn joined_from_end(whole: &str, words: &[String]) -> Option<usize> {
    let mut rest = whole;
    for (count, word) in words.iter().rev().enumerate().map(|(i, w)| (i + 1, w)) {
        rest = rest
            .strip_suffix(word.as_str())
            .filter(|_| !word.is_empty())?;
        if rest.is_empty() {
            return (count >= 2).then_some(count);
        }
    }
    None
}

/// Counts added up over clips.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Tally {
    pub clips: usize,
    pub reference_words: usize,
    pub word_errors: usize,
    pub spacing_word_errors: usize,
    pub splits: usize,
    pub joins: usize,
}

impl Tally {
    pub fn add(&mut self, score: &Score) {
        self.clips += 1;
        self.reference_words += score.reference.len();
        self.word_errors += score.word_errors();
        self.spacing_word_errors += score.spacing_word_errors();
        self.splits += score.splits();
        self.joins += score.joins();
    }

    /// Word errors per hundred reference words; none without reference words.
    pub fn word_error_rate(&self) -> Option<f64> {
        self.percent(self.word_errors)
    }

    /// The word error rate without the word errors of splits and joins.
    pub fn other_error_rate(&self) -> Option<f64> {
        self.percent(self.word_errors - self.spacing_word_errors)
    }

    fn percent(&self, count: usize) -> Option<f64> {
        (self.reference_words > 0).then(|| 100.0 * count as f64 / self.reference_words as f64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Plain word-level Levenshtein distance, the count the word errors must equal.
    fn levenshtein(reference: &[String], text: &[String]) -> usize {
        let mut row: Vec<usize> = (0..=text.len()).collect();
        for (i, r) in reference.iter().enumerate() {
            let mut previous = row[0];
            row[0] = i + 1;
            for (j, t) in text.iter().enumerate() {
                let current = (row[j + 1] + 1)
                    .min(row[j] + 1)
                    .min(previous + usize::from(r != t));
                previous = row[j + 1];
                row[j + 1] = current;
            }
        }
        row[text.len()]
    }

    fn joined(range: &Range<usize>, words: &[String]) -> String {
        words[range.clone()].join(" ")
    }

    fn splits(score: &Score) -> Vec<(String, String)> {
        score
            .steps
            .iter()
            .filter_map(|step| match step {
                Step::Split { reference, text } => Some((
                    score.reference[*reference].clone(),
                    joined(text, &score.text),
                )),
                _ => None,
            })
            .collect()
    }

    fn joins(score: &Score) -> Vec<(String, String)> {
        score
            .steps
            .iter()
            .filter_map(|step| match step {
                Step::Join { reference, text } => Some((
                    joined(reference, &score.reference),
                    score.text[*text].clone(),
                )),
                _ => None,
            })
            .collect()
    }

    fn pair(a: &str, b: &str) -> (String, String) {
        (a.to_owned(), b.to_owned())
    }

    #[test]
    fn the_same_words_have_no_errors() {
        let score = Score::new("Pull-requesten er merget.", "pull-requesten er merget");
        assert_eq!(score.word_errors(), 0);
        assert_eq!((score.splits(), score.joins()), (0, 0));
        assert!(score.steps.iter().all(|s| matches!(s, Step::Match { .. })));
    }

    #[test]
    fn substitutions_deletions_and_insertions() {
        let score = Score::new("jeg har en blå bil", "jeg hadde en bil i dag");
        // «har» → «hadde», «blå» missing, «i» and «dag» added.
        assert_eq!(score.word_errors(), 4);
        assert_eq!(score.spacing_word_errors(), 0);
    }

    #[test]
    fn a_hyphenated_word_split_at_its_hyphen() {
        let score = Score::new("Pull-requesten er merget.", "Pull requesten er merget.");
        assert_eq!(
            splits(&score),
            vec![pair("pull-requesten", "pull requesten")]
        );
        assert_eq!(score.word_errors(), 2);
        assert_eq!(score.spacing_word_errors(), 2);
        assert!(joins(&score).is_empty());
    }

    #[test]
    fn a_compound_split_in_two() {
        let score = Score::new("En lang arbeidsdag.", "En lang arbeids dag.");
        assert_eq!(splits(&score), vec![pair("arbeidsdag", "arbeids dag")]);
        assert_eq!(score.word_errors(), 2);
        assert_eq!(score.spacing_word_errors(), 2);
    }

    #[test]
    fn a_compound_split_in_three() {
        let score = Score::new(
            "sykehusparkeringsplassen er full",
            "sykehus parkerings plassen er full",
        );
        assert_eq!(
            splits(&score),
            vec![pair(
                "sykehusparkeringsplassen",
                "sykehus parkerings plassen"
            )]
        );
        assert_eq!(score.word_errors(), 3);
    }

    #[test]
    fn words_joined_in_one() {
        let score = Score::new("Vi sees i dag.", "Vi sees idag.");
        assert_eq!(joins(&score), vec![pair("i dag", "idag")]);
        assert_eq!(score.word_errors(), 2);
        assert_eq!(score.spacing_word_errors(), 2);
        assert!(splits(&score).is_empty());
    }

    #[test]
    fn words_joined_with_a_hyphen() {
        let score = Score::new("en pull request", "en pull-request");
        assert_eq!(joins(&score), vec![pair("pull request", "pull-request")]);
    }

    #[test]
    fn a_split_and_a_join_in_one_sentence() {
        let score = Score::new(
            "I dag var en lang arbeidsdag for pull-requesten.",
            "Idag var en lang arbeids dag for pull requesten.",
        );
        assert_eq!(joins(&score), vec![pair("i dag", "idag")]);
        assert_eq!(
            splits(&score),
            vec![
                pair("arbeidsdag", "arbeids dag"),
                pair("pull-requesten", "pull requesten")
            ]
        );
        assert_eq!(score.word_errors(), 6);
        assert_eq!(score.spacing_word_errors(), 6);
        let mut tally = Tally::default();
        tally.add(&score);
        assert_eq!(tally.other_error_rate(), Some(0.0));
    }

    #[test]
    fn a_split_next_to_another_error() {
        let score = Score::new("en lang arbeidsdag", "ei lang arbeids dag");
        assert_eq!(splits(&score), vec![pair("arbeidsdag", "arbeids dag")]);
        assert_eq!(score.word_errors(), 3);
        assert_eq!(score.spacing_word_errors(), 2);
    }

    #[test]
    fn a_split_whose_part_matches_the_next_word() {
        // «dag» could match the reference's second «dag» instead; the split is still found.
        let score = Score::new("arbeidsdag dag", "arbeids dag dag");
        assert_eq!(splits(&score), vec![pair("arbeidsdag", "arbeids dag")]);
        assert_eq!(score.word_errors(), 2);
    }

    #[test]
    fn parts_that_do_not_join_to_the_word_are_no_split() {
        let score = Score::new("en arbeidsdag", "en arbeid dag");
        assert!(splits(&score).is_empty());
        assert_eq!(score.word_errors(), 2);
        assert_eq!(score.spacing_word_errors(), 0);
    }

    #[test]
    fn a_word_that_differs_only_in_its_hyphen_is_a_substitution() {
        let score = Score::new("pull-requesten", "pullrequesten");
        assert_eq!(score.word_errors(), 1);
        assert_eq!((score.splits(), score.joins()), (0, 0));
    }

    #[test]
    fn an_empty_side_is_all_insertions_or_all_deletions() {
        let silent = Score::new("", "Takk for meg.");
        assert_eq!(silent.word_errors(), 3);
        assert!(
            silent
                .steps
                .iter()
                .all(|s| matches!(s, Step::Insert { .. }))
        );
        let mut tally = Tally::default();
        tally.add(&silent);
        assert_eq!(tally.word_error_rate(), None);

        let nothing = Score::new("Takk for meg.", "");
        assert_eq!(nothing.word_errors(), 3);
        assert!(
            nothing
                .steps
                .iter()
                .all(|s| matches!(s, Step::Delete { .. }))
        );
        assert_eq!(Score::new("", "").word_errors(), 0);
    }

    #[test]
    fn word_errors_always_equal_the_plain_word_error_count() {
        let cases = [
            (
                "I dag var en lang arbeidsdag",
                "Idag var en lang arbeids dag",
            ),
            ("arbeidsdag dag", "arbeids dag dag"),
            ("i dag i dag", "idag i dag"),
            ("a b c d", "ab cd"),
            ("abc", "a b c"),
            ("ab ab ab", "a b ab a b"),
            (
                "pull-requesten er merget",
                "pull requesten ble merget i går",
            ),
            ("en to tre fire", "fire tre to en"),
            ("", "noe"),
        ];
        for (reference, text) in cases {
            let score = Score::new(reference, text);
            assert_eq!(
                score.word_errors(),
                levenshtein(&score.reference, &score.text),
                "{reference:?} → {text:?}"
            );
        }
    }

    #[test]
    fn steps_cover_every_word_once_in_order() {
        let score = Score::new("a b c d e arbeidsdag", "ab x d arbeids dag f");
        let (mut next_ref, mut next_text) = (0, 0);
        for step in &score.steps {
            let (refs, texts) = match step {
                Step::Match { reference, text } | Step::Substitute { reference, text } => {
                    (*reference..reference + 1, *text..text + 1)
                }
                Step::Delete { reference } => (*reference..reference + 1, next_text..next_text),
                Step::Insert { text } => (next_ref..next_ref, *text..text + 1),
                Step::Split { reference, text } => (*reference..reference + 1, text.clone()),
                Step::Join { reference, text } => (reference.clone(), *text..text + 1),
            };
            assert_eq!((refs.start, texts.start), (next_ref, next_text), "{step:?}");
            (next_ref, next_text) = (refs.end, texts.end);
        }
        assert_eq!(
            (next_ref, next_text),
            (score.reference.len(), score.text.len())
        );
    }

    #[test]
    fn tally_adds_up_clips() {
        let mut tally = Tally::default();
        tally.add(&Score::new("en lang arbeidsdag", "en lang arbeids dag"));
        tally.add(&Score::new("vi sees i dag", "vi ses idag"));
        assert_eq!(tally.clips, 2);
        assert_eq!(tally.reference_words, 7);
        assert_eq!(tally.word_errors, 5);
        assert_eq!(tally.spacing_word_errors, 4);
        assert_eq!((tally.splits, tally.joins), (1, 1));
        assert_eq!(tally.word_error_rate(), Some(500.0 / 7.0));
        assert_eq!(tally.other_error_rate(), Some(100.0 / 7.0));
    }
}
