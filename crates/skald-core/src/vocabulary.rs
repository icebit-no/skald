//! Ordskatt: the sentence and words sent to the model before each take, and the prompt built from them.
//!
//! The prompt opens with the sentence, which shows the inflected forms the words take («merget»,
//! «pull-requesten»); a list of bare words does not teach them. The words follow, joined by commas and
//! closed by a full stop, in the order they are kept. Whisper keeps only the end of a prompt that is too
//! long, so the most important word is the last one, and a prompt is shortened by dropping whole parts from
//! the front: the sentence first, then the words in order.
//!
//! Counting tokens needs the model's tokenizer, which this crate does not have; the caller passes a
//! [`CountTokens`], and the limit to fit within.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Vocabulary {
    pub sentence: String,
    /// In prompt order: the most important word is last.
    pub words: Vec<String>,
    /// Keys a newer Skald wrote, written back unchanged (see the `config` module).
    #[serde(flatten)]
    unknown: toml::Table,
}

impl Vocabulary {
    /// Adds a word last, where the model weighs it most: words are added when they break in a real
    /// dictation. Returns false for a blank word or one already in the list.
    pub fn add_word(&mut self, word: &str) -> bool {
        let word = word.trim();
        if word.is_empty() || self.words.iter().any(|known| known == word) {
            return false;
        }
        self.words.push(word.to_owned());
        true
    }

    /// Returns whether the word was in the list.
    pub fn remove_word(&mut self, word: &str) -> bool {
        let word = word.trim();
        let before = self.words.len();
        self.words.retain(|known| known != word);
        self.words.len() != before
    }

    pub fn prompt(&self) -> Prompt {
        let sentence = Some(self.sentence.trim())
            .filter(|sentence| !sentence.is_empty())
            .map(str::to_owned);
        let words = self
            .words
            .iter()
            .map(|word| word.trim())
            .filter(|word| !word.is_empty())
            .map(str::to_owned);
        Prompt {
            has_sentence: sentence.is_some(),
            parts: sentence.into_iter().chain(words).collect(),
        }
    }
}

/// Counts the tokens a text takes for the model the prompt is meant for.
pub trait CountTokens {
    fn count_tokens(&self, text: &str) -> usize;
}

impl<F: Fn(&str) -> usize> CountTokens for F {
    fn count_tokens(&self, text: &str) -> usize {
        self(text)
    }
}

/// The prompt in the parts it is shortened by: the sentence, then the words, the most important last.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prompt {
    parts: Vec<String>,
    has_sentence: bool,
}

/// A prompt shortened to fit within a number of tokens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fitted {
    pub text: String,
    pub tokens: usize,
    /// How many parts were dropped from the front, the sentence counting as one.
    pub dropped: usize,
}

impl Prompt {
    pub fn parts(&self) -> &[String] {
        &self.parts
    }

    pub fn is_empty(&self) -> bool {
        self.parts.is_empty()
    }

    pub fn text(&self) -> String {
        self.text_from(0)
    }

    /// The tokens the whole prompt takes, for the meter.
    pub fn tokens(&self, counter: &impl CountTokens) -> usize {
        counter.count_tokens(&self.text())
    }

    /// Drops whole parts from the front until the rest fits within `max_tokens`, which keeps every word
    /// whole where the model would cut through the first one it keeps.
    pub fn fit(&self, max_tokens: usize, counter: &impl CountTokens) -> Fitted {
        for dropped in 0..self.parts.len() {
            let text = self.text_from(dropped);
            let tokens = counter.count_tokens(&text);
            if tokens <= max_tokens {
                return Fitted {
                    text,
                    tokens,
                    dropped,
                };
            }
        }
        Fitted {
            text: String::new(),
            tokens: 0,
            dropped: self.parts.len(),
        }
    }

    fn text_from(&self, first: usize) -> String {
        let mut text = String::new();
        let mut last_is_word = false;
        for (index, part) in self.parts.iter().enumerate().skip(first) {
            let is_word = !(self.has_sentence && index == 0);
            if !text.is_empty() {
                text.push_str(if last_is_word { ", " } else { " " });
            }
            text.push_str(part);
            last_is_word = is_word;
        }
        if last_is_word && !text.ends_with(['.', '!', '?']) {
            text.push('.');
        }
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vocabulary(sentence: &str, words: &[&str]) -> Vocabulary {
        Vocabulary {
            sentence: sentence.to_owned(),
            words: words.iter().map(|word| word.to_string()).collect(),
            ..Vocabulary::default()
        }
    }

    /// One token per word or punctuation mark, close enough to a real tokenizer to test the trimming.
    fn count(text: &str) -> usize {
        text.split_whitespace()
            .map(|word| 1 + word.chars().filter(|c| c.is_ascii_punctuation()).count())
            .sum()
    }

    #[test]
    fn opens_with_the_sentence_and_ends_with_the_most_important_word() {
        let prompt =
            vocabulary("Vi merget pull-requesten i går.", &["Skald", "PostgreSQL"]).prompt();
        assert_eq!(
            prompt.text(),
            "Vi merget pull-requesten i går. Skald, PostgreSQL."
        );
        assert_eq!(
            prompt.parts(),
            ["Vi merget pull-requesten i går.", "Skald", "PostgreSQL"]
        );
    }

    #[test]
    fn words_alone_and_a_sentence_alone() {
        assert_eq!(
            vocabulary("", &["Skald", "PostgreSQL"]).prompt().text(),
            "Skald, PostgreSQL."
        );
        assert_eq!(
            vocabulary("Vi merget pull-requesten", &[]).prompt().text(),
            "Vi merget pull-requesten"
        );
        assert!(vocabulary("  ", &[" ", ""]).prompt().is_empty());
        assert_eq!(vocabulary("", &[]).prompt().text(), "");
    }

    #[test]
    fn a_word_ending_a_sentence_gets_no_second_full_stop() {
        assert_eq!(
            vocabulary("", &["Skald", "osv."]).prompt().text(),
            "Skald, osv."
        );
    }

    #[test]
    fn blank_words_and_spaces_around_them_are_left_out() {
        let prompt = vocabulary("  Vi merget den.  ", &[" Skald ", "", "  "]).prompt();
        assert_eq!(prompt.text(), "Vi merget den. Skald.");
        assert_eq!(prompt.parts().len(), 2);
    }

    #[test]
    fn fit_keeps_the_whole_prompt_when_it_fits() {
        let prompt = vocabulary("Vi merget pull-requesten.", &["Skald", "PostgreSQL"]).prompt();
        let fitted = prompt.fit(223, &count);
        assert_eq!(fitted.text, prompt.text());
        assert_eq!(fitted.tokens, count(&prompt.text()));
        assert_eq!(fitted.dropped, 0);
    }

    #[test]
    fn fit_drops_the_sentence_first_then_the_first_words() {
        let prompt = vocabulary(
            "Vi merget pull-requesten i går.",
            &["Skald", "Ærfugl", "PostgreSQL"],
        )
        .prompt();
        // "Skald, Ærfugl, PostgreSQL." counts six: three words and three marks.
        let fitted = prompt.fit(6, &count);
        assert_eq!(fitted.text, "Skald, Ærfugl, PostgreSQL.");
        assert_eq!(fitted.dropped, 1);

        let fitted = prompt.fit(3, &count);
        assert_eq!(fitted.text, "PostgreSQL.");
        assert_eq!(fitted.tokens, 2);
        assert_eq!(fitted.dropped, 3);
    }

    #[test]
    fn fit_gives_nothing_when_not_even_the_last_word_fits() {
        let prompt = vocabulary("", &["PostgreSQL"]).prompt();
        let fitted = prompt.fit(1, &count);
        assert_eq!(fitted.text, "");
        assert_eq!(fitted.tokens, 0);
        assert_eq!(fitted.dropped, 1);
    }

    #[test]
    fn counts_with_the_callers_tokenizer() {
        struct Chars;
        impl CountTokens for Chars {
            fn count_tokens(&self, text: &str) -> usize {
                text.chars().count()
            }
        }
        let prompt = vocabulary("", &["blåbær", "ø"]).prompt();
        assert_eq!(prompt.tokens(&Chars), "blåbær, ø.".chars().count());
    }

    #[test]
    fn adds_new_words_last_and_never_twice() {
        let mut vocabulary = vocabulary("", &["Skald"]);
        assert!(vocabulary.add_word(" merget "));
        assert!(!vocabulary.add_word("merget"));
        assert!(!vocabulary.add_word("  "));
        assert_eq!(vocabulary.words, ["Skald", "merget"]);

        assert!(vocabulary.remove_word("Skald"));
        assert!(!vocabulary.remove_word("Skald"));
        assert_eq!(vocabulary.words, ["merget"]);
    }
}
