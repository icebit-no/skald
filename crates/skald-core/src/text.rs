//! How corrections and the silence check see letters: what belongs to a word, and what counts as the same
//! letter regardless of case.

/// Letters of every script and digits; æ, ø and å are letters like any other. Spaces, punctuation and
/// hyphens end a word, so «SQL-databasen» holds the word «SQL».
pub(crate) fn is_word_char(c: char) -> bool {
    c.is_alphanumeric()
}

/// The lower-case form when it is a single character, which covers Æ, Ø and Å. The few characters whose
/// lower case is longer are compared as written, so one character in the text always stands against one
/// character of a rule.
pub(crate) fn fold(c: char) -> char {
    let mut lower = c.to_lowercase();
    match (lower.next(), lower.next()) {
        (Some(single), None) => single,
        _ => c,
    }
}

/// The words of a text, folded, with the spaces and punctuation between them left out.
pub(crate) fn folded_words(text: &str) -> Vec<String> {
    text.split(|c: char| !is_word_char(c))
        .filter(|word| !word.is_empty())
        .map(|word| word.chars().map(fold).collect())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn norwegian_letters_are_word_characters() {
        for c in ['æ', 'ø', 'å', 'Æ', 'Ø', 'Å', 'é', '7'] {
            assert!(is_word_char(c), "{c}");
        }
        for c in [' ', '-', '.', ',', '«', '»', '\n'] {
            assert!(!is_word_char(c), "{c:?}");
        }
    }

    #[test]
    fn folds_norwegian_capitals() {
        assert_eq!(fold('Æ'), 'æ');
        assert_eq!(fold('Ø'), 'ø');
        assert_eq!(fold('Å'), 'å');
        assert_eq!(fold('å'), 'å');
        // Its lower case is two characters, so it stays as written.
        assert_eq!(fold('İ'), 'İ');
    }

    #[test]
    fn splits_folded_words_on_punctuation() {
        assert_eq!(folded_words(" Ja, takk!"), ["ja", "takk"]);
        assert_eq!(folded_words("Blåbær-SYLTETØY."), ["blåbær", "syltetøy"]);
        assert!(folded_words(" … ").is_empty());
    }
}
