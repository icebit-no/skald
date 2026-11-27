//! Rettinger: exact replacements for words the model hears right but writes wrong, such as «Post gres SQL»
//! → «PostgreSQL» and «mercha» → «merget». They are applied to the text before it is inserted.
//!
//! # How a rule matches
//!
//! - **Whole words.** A rule fires only where its words stand alone: «mercha» is not replaced inside
//!   «merchandise», nor «bær» inside «blåbær». Letters of every script and digits belong to a word; spaces,
//!   punctuation and hyphens do not, so «Post gres SQL-databasen» becomes «PostgreSQL-databasen». A rule
//!   whose words begin or end with a sign other than a letter or digit is not held to a word edge there.
//! - **Case.** Case is ignored when matching, «Æ», «Ø» and «Å» included, so «mercha», «Mercha» and «MERCHA»
//!   all match. The replacement is written as the rule gives it, with one exception: a replacement written
//!   all in lower case takes a capital first letter where the matched text begins with one, so a sentence
//!   that starts with a corrected word still starts with a capital. «PostgreSQL» is always «PostgreSQL».
//! - **Spaces.** A space in a rule matches any run of spaces or line breaks in the text; spaces around the
//!   rule's words are ignored.
//! - **Letters as written.** Text and rules are compared letter by letter without Unicode normalisation.
//!   Æ and ø are letters of their own with no other form. Å has a second form, an «a» followed by a
//!   combining ring, which a Norwegian keyboard does not type; a rule pasted from text in that form does
//!   not match the single letter.
//!
//! # Order
//!
//! The text is read once from the start. Where several rules match at the same place, the one that matches
//! the longer stretch of text wins, and between equally long matches the earlier rule. The text a rule
//! writes is never read again, so one rule never fires on another's replacement.

use serde::{Deserialize, Serialize};

use crate::text::{fold, is_word_char};

/// The rules in the order the user keeps them; the order decides only between two rules that match the
/// same stretch of text.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Corrections {
    pub rules: Vec<Correction>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Correction {
    /// The words as the model writes them.
    pub from: String,
    pub to: String,
    /// How many replacements this rule has made, so the rules that work are visible.
    #[serde(default)]
    pub applied: u32,
    /// Keys a newer Skald wrote, written back unchanged (see the `config` module).
    #[serde(flatten)]
    unknown: toml::Table,
}

impl Correction {
    pub fn new(from: impl Into<String>, to: impl Into<String>) -> Self {
        Self {
            from: from.into(),
            to: to.into(),
            applied: 0,
            unknown: toml::Table::new(),
        }
    }
}

/// The text after corrections, and which rules made the change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Corrected {
    pub text: String,
    /// The index of the rule behind each replacement, in the order they stand in the text.
    pub replacements: Vec<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleError {
    /// A rule needs at least one letter, digit or sign to look for.
    NothingToReplace,
}

impl std::fmt::Display for RuleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NothingToReplace => f.write_str("a correction needs the words to replace"),
        }
    }
}

impl std::error::Error for RuleError {}

impl Corrections {
    /// Corrects a text without counting, for trying the rules on a text.
    pub fn apply(&self, text: &str) -> Corrected {
        let patterns: Vec<Option<Pattern>> = self
            .rules
            .iter()
            .map(|rule| Pattern::new(&rule.from))
            .collect();
        let chars: Vec<char> = text.chars().collect();
        let mut corrected = String::with_capacity(text.len());
        let mut replacements = Vec::new();
        let mut at = 0;
        while at < chars.len() {
            let mut best: Option<(usize, usize)> = None;
            for (rule, pattern) in patterns.iter().enumerate() {
                let Some(end) = pattern.as_ref().and_then(|p| p.match_at(&chars, at)) else {
                    continue;
                };
                if best.is_none_or(|(best_end, _)| end > best_end) {
                    best = Some((end, rule));
                }
            }
            match best {
                Some((end, rule)) => {
                    write_replacement(&mut corrected, &self.rules[rule].to, &chars[at..end]);
                    replacements.push(rule);
                    at = end;
                }
                None => {
                    corrected.push(chars[at]);
                    at += 1;
                }
            }
        }
        Corrected {
            text: corrected,
            replacements,
        }
    }

    /// Corrects the text of a take and counts each replacement on the rule that made it.
    pub fn apply_and_count(&mut self, text: &str) -> String {
        let corrected = self.apply(text);
        for &rule in &corrected.replacements {
            let applied = &mut self.rules[rule].applied;
            *applied = applied.saturating_add(1);
        }
        corrected.text
    }

    /// Adds a rule, or gives a new replacement to the rule that already matches the same words, so two
    /// rules never compete for one phrase.
    pub fn add(&mut self, from: &str, to: &str) -> Result<(), RuleError> {
        let pattern = Pattern::new(from).ok_or(RuleError::NothingToReplace)?;
        let (from, to) = (from.trim(), to.trim());
        match self.position(&pattern) {
            Some(index) => {
                let rule = &mut self.rules[index];
                rule.from = from.to_owned();
                rule.to = to.to_owned();
            }
            None => self.rules.push(Correction::new(from, to)),
        }
        Ok(())
    }

    /// Removes the rule that matches the same words, whatever their case and spacing. Returns whether there
    /// was one.
    pub fn remove(&mut self, from: &str) -> bool {
        let Some(index) = Pattern::new(from).and_then(|pattern| self.position(&pattern)) else {
            return false;
        };
        self.rules.remove(index);
        true
    }

    fn position(&self, pattern: &Pattern) -> Option<usize> {
        self.rules
            .iter()
            .position(|rule| Pattern::new(&rule.from).as_ref() == Some(pattern))
    }
}

/// A rule's words, folded, with each run of spaces as one unit.
#[derive(Debug, PartialEq, Eq)]
struct Pattern {
    units: Vec<Unit>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Unit {
    Char(char),
    Space,
}

impl Pattern {
    /// None when the words are blank, since an empty rule would match everywhere.
    fn new(from: &str) -> Option<Self> {
        let mut units = Vec::new();
        for c in from.trim().chars() {
            if !c.is_whitespace() {
                units.push(Unit::Char(fold(c)));
            } else if units.last() != Some(&Unit::Space) {
                units.push(Unit::Space);
            }
        }
        (!units.is_empty()).then_some(Self { units })
    }

    /// The end of the match that starts at `start`, as an index into `text`.
    fn match_at(&self, text: &[char], start: usize) -> Option<usize> {
        if self.edge_is_word_char(self.units.first()) && start > 0 && is_word_char(text[start - 1])
        {
            return None;
        }
        let mut at = start;
        for unit in &self.units {
            match unit {
                Unit::Char(c) => {
                    if text.get(at).copied().map(fold) != Some(*c) {
                        return None;
                    }
                    at += 1;
                }
                Unit::Space => {
                    if !text.get(at).is_some_and(|c| c.is_whitespace()) {
                        return None;
                    }
                    while text.get(at).is_some_and(|c| c.is_whitespace()) {
                        at += 1;
                    }
                }
            }
        }
        if self.edge_is_word_char(self.units.last())
            && text.get(at).is_some_and(|&c| is_word_char(c))
        {
            return None;
        }
        Some(at)
    }

    fn edge_is_word_char(&self, unit: Option<&Unit>) -> bool {
        matches!(unit, Some(Unit::Char(c)) if is_word_char(*c))
    }
}

fn write_replacement(out: &mut String, to: &str, matched: &[char]) {
    let capital =
        matched.first().is_some_and(|c| c.is_uppercase()) && !to.chars().any(char::is_uppercase);
    let mut chars = to.chars();
    match chars.next() {
        Some(first) if capital => {
            out.extend(first.to_uppercase());
            out.push_str(chars.as_str());
        }
        _ => out.push_str(to),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules(pairs: &[(&str, &str)]) -> Corrections {
        Corrections {
            rules: pairs
                .iter()
                .map(|(from, to)| Correction::new(*from, *to))
                .collect(),
        }
    }

    fn apply(pairs: &[(&str, &str)], text: &str) -> String {
        rules(pairs).apply(text).text
    }

    #[test]
    fn replaces_a_phrase_across_word_boundaries() {
        let pairs = [("Post gres SQL", "PostgreSQL")];
        assert_eq!(
            apply(&pairs, "Vi bruker Post gres SQL i dag."),
            "Vi bruker PostgreSQL i dag."
        );
    }

    #[test]
    fn replaces_a_single_word() {
        let pairs = [("mercha", "merget")];
        assert_eq!(
            apply(&pairs, "Jeg mercha den i går."),
            "Jeg merget den i går."
        );
    }

    #[test]
    fn never_fires_inside_another_word() {
        let pairs = [("mercha", "merget")];
        assert_eq!(apply(&pairs, "merchandise"), "merchandise");
        assert_eq!(apply(&pairs, "omercha"), "omercha");
    }

    #[test]
    fn treats_norwegian_letters_as_part_of_the_word() {
        // An ASCII idea of a word would see the å and the e as edges and replace «bær» in both.
        let pairs = [("bær", "BÆR")];
        assert_eq!(apply(&pairs, "blåbær"), "blåbær");
        assert_eq!(apply(&pairs, "bæret"), "bæret");
        assert_eq!(apply(&pairs, "et bær."), "et BÆR.");
    }

    #[test]
    fn matches_æøå_regardless_of_case() {
        let pairs = [("blå bær", "blåbær")];
        assert_eq!(apply(&pairs, "Jeg plukket blÅ bÆR."), "Jeg plukket blåbær.");
        assert_eq!(apply(&pairs, "Jeg plukket BLÅ BÆR."), "Jeg plukket Blåbær.");
        assert_eq!(apply(&pairs, "To blå bær og ett."), "To blåbær og ett.");
        let pairs = [("smør brød", "smørbrød")];
        assert_eq!(
            apply(&pairs, "SMØR BRØD, smør brød og smør brødet."),
            "Smørbrød, smørbrød og smør brødet."
        );
    }

    #[test]
    fn a_lower_case_replacement_keeps_a_capital_start() {
        let pairs = [("mercha", "merget"), ("blå bær", "blåbær")];
        assert_eq!(apply(&pairs, "Mercha den."), "Merget den.");
        assert_eq!(apply(&pairs, "Blå bær er godt."), "Blåbær er godt.");
    }

    #[test]
    fn a_replacement_with_capitals_is_written_as_given() {
        let pairs = [("Post gres SQL", "PostgreSQL"), ("i fon", "iPhone")];
        assert_eq!(apply(&pairs, "post gres sql"), "PostgreSQL");
        assert_eq!(
            apply(&pairs, "I fon er en telefon."),
            "iPhone er en telefon."
        );
    }

    #[test]
    fn a_space_matches_any_run_of_spaces() {
        let pairs = [("Post gres SQL", "PostgreSQL")];
        assert_eq!(apply(&pairs, "Post  gres\nSQL"), "PostgreSQL");
        assert_eq!(apply(&pairs, "PostgresSQL"), "PostgresSQL");
    }

    #[test]
    fn punctuation_and_hyphens_end_a_word() {
        let pairs = [("Post gres SQL", "PostgreSQL"), ("mercha", "merget")];
        assert_eq!(
            apply(&pairs, "Post gres SQL-databasen"),
            "PostgreSQL-databasen"
        );
        assert_eq!(apply(&pairs, "(mercha), «mercha»!"), "(merget), «merget»!");
    }

    #[test]
    fn a_rule_ending_in_a_sign_is_not_held_to_a_word_edge() {
        let pairs = [("c sharp", "C#")];
        assert_eq!(apply(&pairs, "Skrevet i c sharp."), "Skrevet i C#.");
        let pairs = [("C #", "C#")];
        assert_eq!(apply(&pairs, "C #-kode"), "C#-kode");
    }

    #[test]
    fn the_longer_match_wins_whatever_the_order() {
        let short_first = [("Post gres", "Postgres"), ("Post gres SQL", "PostgreSQL")];
        let long_first = [("Post gres SQL", "PostgreSQL"), ("Post gres", "Postgres")];
        for pairs in [&short_first, &long_first] {
            assert_eq!(
                apply(pairs, "Post gres SQL og Post gres."),
                "PostgreSQL og Postgres."
            );
        }
    }

    #[test]
    fn the_earlier_of_two_equal_matches_wins() {
        let pairs = [("mercha", "merget"), ("Mercha", "mergeta")];
        assert_eq!(apply(&pairs, "mercha"), "merget");
    }

    #[test]
    fn the_leftmost_of_overlapping_matches_wins() {
        let pairs = [("gres SQL", "gresSQL"), ("Post gres", "Postgres")];
        assert_eq!(apply(&pairs, "Post gres SQL"), "Postgres SQL");
    }

    #[test]
    fn a_rule_never_fires_on_another_rules_replacement() {
        let pairs = [("mercha", "merget"), ("merget", "sammenslått")];
        assert_eq!(apply(&pairs, "mercha"), "merget");
        assert_eq!(apply(&pairs, "merget"), "sammenslått");
    }

    #[test]
    fn reports_which_rules_replaced_what() {
        let corrected = rules(&[("mercha", "merget"), ("Post gres SQL", "PostgreSQL")])
            .apply("Post gres SQL er mercha, og mercha igjen.");
        assert_eq!(corrected.text, "PostgreSQL er merget, og merget igjen.");
        assert_eq!(corrected.replacements, [1, 0, 0]);
    }

    #[test]
    fn counting_happens_only_for_a_take() {
        let mut corrections = rules(&[("mercha", "merget"), ("Post gres SQL", "PostgreSQL")]);
        corrections.apply("mercha");
        assert_eq!(corrections.rules[0].applied, 0);

        let text = corrections.apply_and_count("Jeg mercha og mercha Post gres SQL.");
        assert_eq!(text, "Jeg merget og merget PostgreSQL.");
        assert_eq!(corrections.rules[0].applied, 2);
        assert_eq!(corrections.rules[1].applied, 1);
    }

    #[test]
    fn a_blank_rule_never_matches() {
        let pairs = [("", "x"), ("  ", "y")];
        assert_eq!(
            apply(&pairs, "Hei, dette er en prøve"),
            "Hei, dette er en prøve"
        );
    }

    #[test]
    fn add_replaces_the_rule_for_the_same_words() {
        let mut corrections = Corrections::default();
        corrections.add(" mercha ", " merget ").unwrap();
        corrections.rules[0].applied = 4;
        corrections.add("MERCHA", "merga").unwrap();
        assert_eq!(corrections.rules.len(), 1);
        assert_eq!(corrections.rules[0].to, "merga");
        assert_eq!(corrections.rules[0].applied, 4);

        corrections.add("Post  gres SQL", "PostgreSQL").unwrap();
        assert_eq!(corrections.rules.len(), 2);
        assert_eq!(corrections.add(" ", "x"), Err(RuleError::NothingToReplace));
    }

    #[test]
    fn remove_finds_the_rule_whatever_its_case_and_spacing() {
        let mut corrections = rules(&[("Post gres SQL", "PostgreSQL"), ("mercha", "merget")]);
        assert!(corrections.remove("post   GRES sql"));
        assert!(!corrections.remove("post gres sql"));
        assert_eq!(corrections.rules.len(), 1);
        assert_eq!(corrections.rules[0].from, "mercha");
    }
}
