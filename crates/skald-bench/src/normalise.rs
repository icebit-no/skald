//! The words a text is scored by. The reference and the model's text go through the same steps, so
//! differences in writing that do not change what was said are not counted as errors:
//!
//! 1. **Letters.** Lower case. Æ, ø, å and other letters are kept as letters; a letter written as a base
//!    letter and a combining mark («a» + ring) is joined to the one letter («å»), for the accented
//!    letters Norwegian uses.
//! 2. **Numbers.** Digit groups split by one space, a no-break space or a thin space are one number
//!    («10 000» is «10000»), and so are groups split by full stops when every group after the first has
//!    three digits («10.000»). A full stop between digits otherwise is a decimal mark and becomes a comma
//!    («3.5» is «3,5»); a comma or colon between digits stays («3,5», «14:30»). Numbers written as words
//!    are not turned into digits, so «tre» and «3» differ: write a reference the way the model is
//!    expected to write it. Letters right after a number are a word of their own, so «2.4GHz» is «2,4»
//!    and «ghz», as «2,4 GHz» is. «%» is the word «prosent».
//! 3. **Hyphens and other marks inside a word.** A hyphen, full stop or apostrophe between two letters or
//!    digits stays in the word: «pull-requesten», «f.eks», «github.com» and «rock'n'roll» are one word
//!    each. Typographic hyphens and apostrophes count as the plain ones; a soft hyphen is dropped.
//! 4. **Everything else** that is not a letter or digit separates words and is dropped: spaces, «» and
//!    quotes, brackets, dashes, slashes, and a comma, full stop, colon, question or exclamation mark at
//!    the edge of a word.
//!
//! A word that differs only in its hyphens («pullrequesten», «pull-requesten») is a word error. Where
//! the text splits a word or joins words, the alignment compares them without their hyphens (the
//! `score` module).

/// The words of `text` after the steps above.
pub fn words(text: &str) -> Vec<String> {
    let chars = compose(text);
    let chars = join_digit_groups(&chars);
    split_words(&chars)
        .into_iter()
        .map(|word| {
            if word == "%" {
                "prosent".to_owned()
            } else {
                normalise_number(&word)
            }
        })
        .collect()
}

/// Lower case, typographic marks as plain ones, and the combining marks Norwegian letters are written
/// with joined to their letter.
fn compose(text: &str) -> Vec<char> {
    let mut out: Vec<char> = Vec::with_capacity(text.len());
    for c in text.chars() {
        let c = match c {
            '\u{2010}' | '\u{2011}' => '-',
            '\u{2019}' | '\u{02BC}' => '\'',
            '\u{00AD}' => continue,
            _ => c,
        };
        if let Some(base) = out.last_mut()
            && let Some(letter) = with_mark(*base, c)
        {
            *base = letter;
            continue;
        }
        out.extend(c.to_lowercase());
    }
    out
}

/// The letter a lower-case base letter and a combining mark make: «å», and the accents Norwegian
/// spelling uses, as in «idé», «fôr» and «à».
fn with_mark(base: char, mark: char) -> Option<char> {
    Some(match (base, mark) {
        ('a', '\u{030A}') => 'å',
        ('e', '\u{0301}') => 'é',
        ('e', '\u{0300}') => 'è',
        ('e', '\u{0302}') => 'ê',
        ('o', '\u{0301}') => 'ó',
        ('o', '\u{0300}') => 'ò',
        ('o', '\u{0302}') => 'ô',
        ('u', '\u{0308}') => 'ü',
        ('a', '\u{0300}') => 'à',
        _ => return None,
    })
}

fn is_group_space(c: char) -> bool {
    matches!(c, ' ' | '\u{00A0}' | '\u{202F}' | '\u{2009}')
}

/// Removes the space between digit groups of one number: a run of one to three digits followed by
/// runs of exactly three digits, each after one space. A decimal part may follow («10 000,50»).
fn join_digit_groups(chars: &[char]) -> Vec<char> {
    let digit_run = |from: usize| {
        chars[from..]
            .iter()
            .take_while(|c| c.is_ascii_digit())
            .count()
    };
    let mut out = Vec::with_capacity(chars.len());
    let mut i = 0;
    while i < chars.len() {
        let starts_number = chars[i].is_ascii_digit()
            && (i == 0 || (!is_number_char(chars[i - 1]) && !chars[i - 1].is_alphanumeric()));
        if !starts_number {
            out.push(chars[i]);
            i += 1;
            continue;
        }
        let first = digit_run(i);
        out.extend_from_slice(&chars[i..i + first]);
        i += first;
        if first <= 3 {
            while i + 4 <= chars.len()
                && is_group_space(chars[i])
                && digit_run(i + 1) == 3
                && chars.get(i + 4).is_none_or(|c| !c.is_ascii_digit())
            {
                out.extend_from_slice(&chars[i + 1..i + 4]);
                i += 4;
            }
        }
    }
    out
}

/// A character that can be part of a number written with marks: a digit, or a decimal or group mark.
fn is_number_char(c: char) -> bool {
    c.is_ascii_digit() || matches!(c, ',' | '.' | ':')
}

/// Splits into words at every character that is not a letter or digit, except a hyphen, full stop or
/// apostrophe between two letters or digits, and a comma or colon between two digits; and between a
/// number and the letters right after it.
fn split_words(chars: &[char]) -> Vec<String> {
    let mut words = Vec::new();
    let mut word = String::new();
    for (i, &c) in chars.iter().enumerate() {
        let before = i.checked_sub(1).map(|j| chars[j]);
        let after = chars.get(i + 1).copied();
        let between = |test: fn(char) -> bool| before.is_some_and(test) && after.is_some_and(test);
        let keep = c.is_alphanumeric()
            || (matches!(c, '-' | '.' | '\'') && between(char::is_alphanumeric))
            || (matches!(c, ',' | ':') && between(|c| c.is_ascii_digit()));
        if keep {
            if c.is_alphabetic() && !word.is_empty() && word.chars().all(is_number_char) {
                words.push(std::mem::take(&mut word));
            }
            word.push(c);
        } else if c == '%' {
            if !word.is_empty() {
                words.push(std::mem::take(&mut word));
            }
            words.push("%".to_owned());
        } else if !word.is_empty() {
            words.push(std::mem::take(&mut word));
        }
    }
    if !word.is_empty() {
        words.push(word);
    }
    words
}

/// A number with full stops: group marks when every group after the first has three digits, a decimal
/// mark otherwise. Anything else is left as it is.
fn normalise_number(word: &str) -> String {
    if !word.contains('.') || !word.chars().all(|c| c.is_ascii_digit() || c == '.') {
        return word.to_owned();
    }
    let groups: Vec<&str> = word.split('.').collect();
    let grouped = groups[0].len() <= 3 && groups[1..].iter().all(|group| group.len() == 3);
    if grouped {
        groups.concat()
    } else if groups.len() == 2 {
        format!("{},{}", groups[0], groups[1])
    } else {
        word.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(text: &str) -> Vec<String> {
        words(text)
    }

    fn list(words: &[&str]) -> Vec<String> {
        words.iter().map(|&w| w.to_owned()).collect()
    }

    #[test]
    fn lower_case_without_punctuation() {
        assert_eq!(
            w("Hei, Kari! Er du der? «Ja», sa hun."),
            list(&["hei", "kari", "er", "du", "der", "ja", "sa", "hun"])
        );
    }

    #[test]
    fn norwegian_letters_are_kept() {
        assert_eq!(
            w("Ærlig talt, Øyvind på Ålesund"),
            list(&["ærlig", "talt", "øyvind", "på", "ålesund"])
        );
    }

    #[test]
    fn decomposed_letters_are_joined() {
        // «på» and «idé» as a Mac may write them in a file name: a base letter and a combining mark.
        assert_eq!(w("pa\u{030A} ide\u{0301}"), list(&["på", "idé"]));
        assert_eq!(w("A\u{030A}lesund"), list(&["ålesund"]));
    }

    #[test]
    fn hyphens_inside_a_word_stay() {
        assert_eq!(
            w("Pull-requesten er merget."),
            list(&["pull-requesten", "er", "merget"])
        );
        assert_eq!(
            w("e\u{2011}post"),
            list(&["e-post"]),
            "a non-breaking hyphen is a hyphen"
        );
    }

    #[test]
    fn dashes_and_loose_hyphens_separate_words() {
        assert_eq!(w("Oslo – Bergen"), list(&["oslo", "bergen"]));
        assert_eq!(w("Oslo–Bergen"), list(&["oslo", "bergen"]));
        assert_eq!(w("inn- og utgang"), list(&["inn", "og", "utgang"]));
        assert_eq!(w("- første punkt"), list(&["første", "punkt"]));
    }

    #[test]
    fn a_soft_hyphen_is_dropped() {
        assert_eq!(w("arbeids\u{00AD}dag"), list(&["arbeidsdag"]));
    }

    #[test]
    fn abbreviations_and_addresses_are_one_word() {
        assert_eq!(w("f.eks. github.com"), list(&["f.eks", "github.com"]));
        assert_eq!(
            w("Det er rock\u{2019}n\u{2019}roll."),
            list(&["det", "er", "rock'n'roll"])
        );
        assert_eq!(w("Thomas' bil"), list(&["thomas", "bil"]));
    }

    #[test]
    fn digit_groups_make_one_number() {
        assert_eq!(w("10 000 kroner"), list(&["10000", "kroner"]));
        assert_eq!(w("1\u{00A0}250\u{202F}000"), list(&["1250000"]));
        assert_eq!(w("10.000 kroner"), list(&["10000", "kroner"]));
        assert_eq!(w("10000 kroner"), list(&["10000", "kroner"]));
        assert_eq!(w("10 000,50 kroner"), list(&["10000,50", "kroner"]));
    }

    #[test]
    fn numbers_in_a_list_stay_apart() {
        assert_eq!(w("10, 200, 300"), list(&["10", "200", "300"]));
        assert_eq!(
            w("i 2024 300 ganger"),
            list(&["i", "2024", "300", "ganger"])
        );
        assert_eq!(w("1 2 3"), list(&["1", "2", "3"]));
        assert_eq!(w("10 0000"), list(&["10", "0000"]));
        assert_eq!(w("M4 300"), list(&["m4", "300"]));
    }

    #[test]
    fn decimal_marks() {
        assert_eq!(w("3,5 og 3.5"), list(&["3,5", "og", "3,5"]));
        assert_eq!(w("kl. 14:30."), list(&["kl", "14:30"]));
        assert_eq!(w("versjon 1.5.2"), list(&["versjon", "1.5.2"]));
    }

    #[test]
    fn edge_punctuation_after_a_number_goes() {
        assert_eq!(w("Den 17. mai, 1814."), list(&["den", "17", "mai", "1814"]));
    }

    #[test]
    fn letters_after_a_number_are_a_word_of_their_own() {
        assert_eq!(
            w("2.4GHz og 5 GHz"),
            list(&["2,4", "ghz", "og", "5", "ghz"])
        );
        assert_eq!(w("5km"), list(&["5", "km"]));
        assert_eq!(w("802.11n-standarden"), list(&["802,11", "n-standarden"]));
        assert_eq!(
            w("1980-tallet, M4 og H2O"),
            list(&["1980-tallet", "m4", "og", "h2o"])
        );
    }

    #[test]
    fn percent_is_a_word() {
        assert_eq!(w("50 %"), list(&["50", "prosent"]));
        assert_eq!(w("50%"), list(&["50", "prosent"]));
        assert_eq!(w("50 prosent"), list(&["50", "prosent"]));
    }

    #[test]
    fn nothing_but_punctuation_has_no_words() {
        assert!(w(" … !? «» ").is_empty());
        assert!(w("").is_empty());
    }
}
