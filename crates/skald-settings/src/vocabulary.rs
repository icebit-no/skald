//! Ordskatt: the sentence, the words with the token meter, the corrections with how often each applied,
//! «+ Ny retting» and «Prøv på en tekst» (docs/design.md, Vocabulary and corrections).

use skald_core::Config;

use crate::action::Change;
use crate::model::SettingsModel;
use crate::words;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VocabularyTab {
    pub sentence: String,
    /// The prompt is too long and the model never sees the sentence: it is dropped first.
    pub sentence_dropped: bool,
    /// In prompt order: the most important word is last.
    pub words: Vec<WordRow>,
    pub meter: Meter,
    pub corrections: Vec<CorrectionRow>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WordRow {
    pub word: String,
    /// Left out of what the model sees, since the words before the last ones are dropped when the prompt
    /// is too long.
    pub dropped: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Meter {
    /// No model is loaded to count with.
    Uncounted,
    Counted {
        used: usize,
        limit: usize,
    },
}

impl Meter {
    pub fn text(self) -> String {
        match self {
            Self::Uncounted => "Tokens: telles når talemodellen er lastet.".to_owned(),
            Self::Counted { used, limit } => format!("Tokens: {used} av {limit}"),
        }
    }

    /// What the user should know beside the count, when anything.
    pub fn warning(self) -> Option<&'static str> {
        self.is_over()
            .then_some("Modellen beholder bare slutten, så starten faller bort.")
    }

    pub fn is_over(self) -> bool {
        matches!(self, Self::Counted { used, limit } if used > limit)
    }

    /// How full the meter is, from 0 to 1.
    pub fn fill(self) -> f32 {
        match self {
            Self::Uncounted => 0.0,
            Self::Counted { used, limit } => (used as f32 / limit.max(1) as f32).min(1.0),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CorrectionRow {
    pub from: String,
    pub to: String,
    pub count: u32,
    /// «brukt 12 ganger»
    pub applied: String,
}

/// A text with the corrections applied, for «Prøv på en tekst».
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tried {
    pub text: String,
    /// The rules that changed it, as «mercha → merget», in the order they stand in the text.
    pub applied: Vec<String>,
}

impl VocabularyTab {
    pub fn of(model: &SettingsModel) -> Self {
        let vocabulary = &model.config.vocabulary;
        let prompt = vocabulary.prompt();
        let has_sentence = !vocabulary.sentence.trim().is_empty();
        let (meter, dropped) = match &model.tokens {
            None => (Meter::Uncounted, 0),
            Some(counter) => {
                let count = |text: &str| counter.count(text);
                let fitted = prompt.fit(counter.limit, &count);
                (
                    Meter::Counted {
                        used: prompt.tokens(&count),
                        limit: counter.limit,
                    },
                    fitted.dropped,
                )
            }
        };
        let first_word = usize::from(has_sentence);
        let words = vocabulary
            .words
            .iter()
            .map(|word| word.trim())
            .filter(|word| !word.is_empty())
            .enumerate()
            .map(|(index, word)| WordRow {
                word: word.to_owned(),
                dropped: first_word + index < dropped,
            })
            .collect();
        let counts = &model.state.counts;
        Self {
            sentence: vocabulary.sentence.clone(),
            sentence_dropped: has_sentence && dropped > 0,
            words,
            meter,
            corrections: model
                .config
                .corrections
                .rules
                .iter()
                .map(|rule| {
                    let count = counts.get(&rule.from);
                    CorrectionRow {
                        from: rule.from.clone(),
                        to: rule.to.clone(),
                        count,
                        applied: words::times_applied(count),
                    }
                })
                .collect(),
        }
    }

    /// The sentence as typed; `None` when it is the one already set.
    pub fn set_sentence(&self, sentence: &str) -> Option<Change> {
        (sentence.trim() != self.sentence.trim())
            .then(|| Change::Sentence(sentence.trim().to_owned()))
    }

    /// `Ok(None)` for a blank word; a word already there is refused with the reason.
    pub fn add_word(&self, word: &str) -> Result<Option<Change>, String> {
        let word = word.trim();
        if word.is_empty() {
            return Ok(None);
        }
        if self.words.iter().any(|row| row.word == word) {
            return Err(format!("«{word}» står allerede i ordskatten."));
        }
        Ok(Some(Change::AddWord(word.to_owned())))
    }

    pub fn remove_word(&self, index: usize) -> Option<Change> {
        let row = self.words.get(index)?;
        Some(Change::RemoveWord(row.word.clone()))
    }

    /// A new correction, or a new replacement for the correction of the same words.
    pub fn add_correction(&self, from: &str, to: &str) -> Result<Change, String> {
        let (from, to) = checked(from, to)?;
        Ok(Change::AddCorrection { from, to })
    }

    /// The correction at `index` with the words and replacement as edited; `Ok(None)` when nothing changed.
    pub fn edit_correction(
        &self,
        index: usize,
        from: &str,
        to: &str,
    ) -> Result<Option<Change>, String> {
        let Some(row) = self.corrections.get(index) else {
            return Ok(None);
        };
        let (from, to) = checked(from, to)?;
        if from == row.from && to == row.to {
            return Ok(None);
        }
        Ok(Some(if from == row.from {
            Change::AddCorrection { from, to }
        } else {
            Change::EditCorrection {
                before: row.from.clone(),
                from,
                to,
            }
        }))
    }

    pub fn remove_correction(&self, index: usize) -> Option<Change> {
        let row = self.corrections.get(index)?;
        Some(Change::RemoveCorrection {
            from: row.from.clone(),
        })
    }
}

fn checked(from: &str, to: &str) -> Result<(String, String), String> {
    let (from, to) = (from.trim(), to.trim());
    if from.is_empty() {
        return Err("Skriv ordene slik modellen skriver dem.".to_owned());
    }
    if to.is_empty() {
        return Err("Skriv hva de skal byttes med.".to_owned());
    }
    Ok((from.to_owned(), to.to_owned()))
}

/// «Prøv på en tekst»: the text as Skald would insert it with the corrections as they are.
pub fn try_corrections(config: &Config, text: &str) -> Tried {
    let corrected = config.corrections.apply(text);
    let rules = &config.corrections.rules;
    Tried {
        text: corrected.text,
        applied: corrected
            .replacements
            .iter()
            .map(|&index| format!("{} → {}", rules[index].from, rules[index].to))
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use skald_core::corrections::Corrections;
    use skald_core::state::State;

    use super::*;
    use crate::model::TokenCounter;

    /// One token per word or punctuation mark, close enough to a real tokenizer to test the meter.
    fn count(text: &str) -> usize {
        text.split_whitespace()
            .map(|word| 1 + word.chars().filter(|c| c.is_ascii_punctuation()).count())
            .sum()
    }

    fn settings(sentence: &str, words: &[&str], limit: Option<usize>) -> SettingsModel {
        let mut config = Config::default();
        config.vocabulary.sentence = sentence.to_owned();
        for word in words {
            config.vocabulary.add_word(word);
        }
        SettingsModel {
            config,
            tokens: limit.map(|limit| TokenCounter::new(limit, count)),
            ..SettingsModel::default()
        }
    }

    #[test]
    fn the_meter_counts_the_whole_prompt() {
        let tab = VocabularyTab::of(&settings(
            "Vi merget pull-requesten.",
            &["Skald", "PostgreSQL"],
            Some(223),
        ));
        // "Vi merget pull-requesten. Skald, PostgreSQL." counts nine: five words and four marks.
        assert_eq!(
            tab.meter,
            Meter::Counted {
                used: 9,
                limit: 223
            }
        );
        assert_eq!(tab.meter.text(), "Tokens: 9 av 223");
        assert_eq!(tab.meter.warning(), None);
        assert!(!tab.sentence_dropped);
        assert!(tab.words.iter().all(|row| !row.dropped));
    }

    #[test]
    fn a_prompt_too_long_drops_the_sentence_then_the_first_words() {
        let tab = VocabularyTab::of(&settings(
            "Vi merget pull-requesten i går.",
            &["Skald", "Ærfugl", "PostgreSQL"],
            Some(3),
        ));
        assert!(tab.meter.is_over());
        assert_eq!(
            tab.meter.warning(),
            Some("Modellen beholder bare slutten, så starten faller bort.")
        );
        assert!(tab.sentence_dropped);
        let dropped: Vec<(&str, bool)> = tab
            .words
            .iter()
            .map(|row| (row.word.as_str(), row.dropped))
            .collect();
        assert_eq!(
            dropped,
            [("Skald", true), ("Ærfugl", true), ("PostgreSQL", false)]
        );
        assert_eq!(tab.meter.fill(), 1.0);
    }

    #[test]
    fn without_a_model_the_tokens_are_not_counted() {
        let tab = VocabularyTab::of(&settings("", &["Skald"], None));
        assert_eq!(tab.meter, Meter::Uncounted);
        assert_eq!(
            tab.meter.text(),
            "Tokens: telles når talemodellen er lastet."
        );
        assert!(!tab.words[0].dropped);
    }

    #[test]
    fn words_are_added_last_once_and_removed() {
        let tab = VocabularyTab::of(&settings("", &["Skald"], None));
        assert_eq!(tab.add_word("  "), Ok(None));
        assert_eq!(
            tab.add_word(" merget "),
            Ok(Some(Change::AddWord("merget".into())))
        );
        assert_eq!(
            tab.add_word("Skald"),
            Err("«Skald» står allerede i ordskatten.".into())
        );
        assert_eq!(tab.remove_word(0), Some(Change::RemoveWord("Skald".into())));
        assert_eq!(tab.remove_word(1), None);
    }

    #[test]
    fn the_sentence_changes_only_when_it_differs() {
        let tab = VocabularyTab::of(&settings("Vi merget den.", &[], None));
        assert_eq!(tab.set_sentence(" Vi merget den. "), None);
        assert_eq!(
            tab.set_sentence("Vi merget pull-requesten."),
            Some(Change::Sentence("Vi merget pull-requesten.".into()))
        );
        assert_eq!(tab.set_sentence(""), Some(Change::Sentence(String::new())));
    }

    #[test]
    fn corrections_show_how_often_each_applied() {
        let mut model = settings("", &[], None);
        model.config.corrections.add("mercha", "merget").unwrap();
        model
            .config
            .corrections
            .add("Post gres SQL", "PostgreSQL")
            .unwrap();
        let mut state = State::default();
        for _ in 0..3 {
            model
                .config
                .corrections
                .apply_and_count("Vi mercha den.", &mut state.counts);
        }
        model.state = state;
        let tab = VocabularyTab::of(&model);
        let rows: Vec<(&str, &str, &str)> = tab
            .corrections
            .iter()
            .map(|row| (row.from.as_str(), row.to.as_str(), row.applied.as_str()))
            .collect();
        assert_eq!(
            rows,
            [
                ("mercha", "merget", "brukt 3 ganger"),
                ("Post gres SQL", "PostgreSQL", "ikke brukt ennå")
            ]
        );
    }

    #[test]
    fn a_new_correction_needs_both_sides() {
        let tab = VocabularyTab::of(&settings("", &[], None));
        assert_eq!(
            tab.add_correction(" mercha ", " merget "),
            Ok(Change::AddCorrection {
                from: "mercha".into(),
                to: "merget".into()
            })
        );
        assert_eq!(
            tab.add_correction("", "merget"),
            Err("Skriv ordene slik modellen skriver dem.".into())
        );
        assert_eq!(
            tab.add_correction("mercha", " "),
            Err("Skriv hva de skal byttes med.".into())
        );
    }

    #[test]
    fn an_edited_correction_keeps_its_words_or_replaces_the_rule() {
        let mut model = settings("", &[], None);
        model.config.corrections.add("mercha", "merget").unwrap();
        let tab = VocabularyTab::of(&model);
        assert_eq!(tab.edit_correction(0, "mercha", "merget"), Ok(None));
        assert_eq!(
            tab.edit_correction(0, "mercha", "merga"),
            Ok(Some(Change::AddCorrection {
                from: "mercha".into(),
                to: "merga".into()
            }))
        );
        assert_eq!(
            tab.edit_correction(0, "merka", "merget"),
            Ok(Some(Change::EditCorrection {
                before: "mercha".into(),
                from: "merka".into(),
                to: "merget".into()
            }))
        );
        assert_eq!(
            tab.remove_correction(0),
            Some(Change::RemoveCorrection {
                from: "mercha".into()
            })
        );
    }

    #[test]
    fn trying_a_text_shows_the_result_and_the_rules_that_applied() {
        let mut config = Config::default();
        config.corrections = Corrections::default();
        config.corrections.add("mercha", "merget").unwrap();
        config
            .corrections
            .add("Post gres SQL", "PostgreSQL")
            .unwrap();
        let tried = try_corrections(&config, "Mercha du Post gres SQL-basen?");
        assert_eq!(tried.text, "Merget du PostgreSQL-basen?");
        assert_eq!(
            tried.applied,
            ["mercha → merget", "Post gres SQL → PostgreSQL"]
        );
        assert!(
            try_corrections(&config, "Ingenting her.")
                .applied
                .is_empty()
        );
    }
}
