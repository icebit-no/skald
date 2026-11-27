//! Historikk: the five latest texts, «Kopier», and correcting a word: «Bytt "mercha" med [merget]», with
//! «Rett dette automatisk neste gang» and «Legg "merget" i ordskatten» (docs/design.md, History).

use crate::action::{Action, Change};
use crate::model::SettingsModel;
use crate::words;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryTab {
    /// Newest first.
    pub entries: Vec<HistoryRow>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryRow {
    pub text: String,
    pub unix_ms: i64,
    /// When and with which microphone: «for 2 minutter siden · Shure MV7».
    pub details: String,
    /// The text cut into words and what stands between them, so a word can be clicked.
    pub pieces: Vec<Piece>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Piece {
    pub text: String,
    pub is_word: bool,
}

/// The words picked in one entry, from the first piece to the last, both words.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pick {
    pub entry: usize,
    pub first: usize,
    pub last: usize,
}

impl HistoryTab {
    /// `now_ms` is the clock's time, so the tab says how long ago each take ended.
    pub fn of(model: &SettingsModel, now_ms: i64) -> Self {
        Self {
            entries: model
                .state
                .history
                .entries()
                .iter()
                .map(|entry| {
                    let mut details: Vec<String> =
                        words::ago(entry.unix_ms, now_ms).into_iter().collect();
                    if !entry.microphone.is_empty() {
                        details.push(entry.microphone.clone());
                    }
                    HistoryRow {
                        text: entry.text.clone(),
                        unix_ms: entry.unix_ms,
                        details: details.join(" · "),
                        pieces: pieces(&entry.text),
                    }
                })
                .collect(),
        }
    }

    /// Clicking a word picks it. With `extend`, a pick in the same entry grows to reach the word, so a
    /// correction can cover words the model split, as «Post gres SQL».
    pub fn pick(&self, entry: usize, piece: usize, extend: Option<Pick>) -> Option<Pick> {
        let row = self.entries.get(entry)?;
        if !row.pieces.get(piece)?.is_word {
            return None;
        }
        Some(match extend {
            Some(pick) if pick.entry == entry => Pick {
                entry,
                first: pick.first.min(piece),
                last: pick.last.max(piece),
            },
            _ => Pick {
                entry,
                first: piece,
                last: piece,
            },
        })
    }

    /// The picked words as the text has them.
    pub fn picked(&self, pick: Pick) -> Option<String> {
        let row = self.entries.get(pick.entry)?;
        Some(
            row.pieces
                .get(pick.first..=pick.last)?
                .iter()
                .map(|piece| piece.text.as_str())
                .collect(),
        )
    }

    /// «Bytt»: the entry with the picked words replaced, and what the two boxes ask for. A replacement
    /// written in lower case takes a capital first letter where the picked words had one, as a correction
    /// does.
    pub fn fix(
        &self,
        pick: Pick,
        replacement: &str,
        correct_next_time: bool,
        add_to_vocabulary: bool,
    ) -> Result<Vec<Action>, String> {
        let replacement = replacement.trim();
        if replacement.is_empty() {
            return Err("Skriv hva det skal byttes med.".to_owned());
        }
        let (Some(row), Some(wrong)) = (self.entries.get(pick.entry), self.picked(pick)) else {
            return Ok(Vec::new());
        };
        let mut actions = Vec::new();
        if replacement != wrong {
            let written = capitalised_like(replacement, &wrong);
            let after: String = row.pieces[..pick.first]
                .iter()
                .map(|piece| piece.text.as_str())
                .chain([written.as_str()])
                .chain(row.pieces[pick.last + 1..].iter().map(|p| p.text.as_str()))
                .collect();
            actions.push(Action::CorrectHistory {
                unix_ms: row.unix_ms,
                before: row.text.clone(),
                after,
            });
            if correct_next_time {
                actions.push(Action::Change(Change::AddCorrection {
                    from: wrong,
                    to: replacement.to_owned(),
                }));
            }
        }
        if add_to_vocabulary {
            actions.push(Action::Change(Change::AddWord(replacement.to_owned())));
        }
        Ok(actions)
    }
}

/// «Bytt "mercha" med»
pub fn replace_label(wrong: &str) -> String {
    format!("Bytt \"{wrong}\" med")
}

/// «Legg "merget" i ordskatten»
pub fn add_word_label(replacement: &str) -> String {
    format!("Legg \"{}\" i ordskatten", replacement.trim())
}

pub const CORRECT_NEXT_TIME: &str = "Rett dette automatisk neste gang";

/// Letters and digits make a word, as corrections see them; everything else stands between words.
fn pieces(text: &str) -> Vec<Piece> {
    let mut pieces: Vec<Piece> = Vec::new();
    for c in text.chars() {
        let is_word = c.is_alphanumeric();
        match pieces.last_mut() {
            Some(last) if last.is_word == is_word => last.text.push(c),
            _ => pieces.push(Piece {
                text: c.to_string(),
                is_word,
            }),
        }
    }
    pieces
}

fn capitalised_like(replacement: &str, original: &str) -> String {
    let starts_upper = original.chars().next().is_some_and(char::is_uppercase);
    if !starts_upper || replacement.chars().any(char::is_uppercase) {
        return replacement.to_owned();
    }
    let mut chars = replacement.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use skald_core::history::Entry;

    use super::*;

    const NOW: i64 = 1_795_800_000_000;

    fn tab(texts: &[&str]) -> HistoryTab {
        let mut model = SettingsModel::default();
        for (age, text) in texts.iter().enumerate().rev() {
            model.state.history.push(Entry {
                text: (*text).to_owned(),
                unix_ms: NOW - age as i64 * 120_000,
                microphone: "Shure MV7".to_owned(),
            });
        }
        HistoryTab::of(&model, NOW)
    }

    #[test]
    fn shows_the_texts_newest_first_with_when_and_where() {
        let tab = tab(&["Vi mercha den.", "Hei, dette er en prøve."]);
        let rows: Vec<(&str, &str)> = tab
            .entries
            .iter()
            .map(|row| (row.text.as_str(), row.details.as_str()))
            .collect();
        assert_eq!(
            rows,
            [
                ("Vi mercha den.", "nå nettopp · Shure MV7"),
                (
                    "Hei, dette er en prøve.",
                    "for 2 minutter siden · Shure MV7"
                )
            ]
        );
        assert!(
            HistoryTab::of(&SettingsModel::default(), NOW)
                .entries
                .is_empty()
        );
    }

    #[test]
    fn cuts_a_text_into_words_with_æøå_and_what_stands_between() {
        let tab = tab(&["Bø i Telemark, pull-requesten."]);
        let pieces: Vec<(&str, bool)> = tab.entries[0]
            .pieces
            .iter()
            .map(|piece| (piece.text.as_str(), piece.is_word))
            .collect();
        assert_eq!(
            pieces,
            [
                ("Bø", true),
                (" ", false),
                ("i", true),
                (" ", false),
                ("Telemark", true),
                (", ", false),
                ("pull", true),
                ("-", false),
                ("requesten", true),
                (".", false)
            ]
        );
    }

    #[test]
    fn a_pick_is_one_word_and_grows_within_its_entry() {
        let tab = tab(&["Bruk Post gres SQL her.", "Annen tekst."]);
        assert_eq!(tab.pick(0, 1, None), None, "a space is not a word");
        let pick = tab.pick(0, 2, None).unwrap();
        assert_eq!(tab.picked(pick).as_deref(), Some("Post"));
        let pick = tab.pick(0, 6, Some(pick)).unwrap();
        assert_eq!(tab.picked(pick).as_deref(), Some("Post gres SQL"));
        let other = tab.pick(1, 0, Some(pick)).unwrap();
        assert_eq!(tab.picked(other).as_deref(), Some("Annen"), "a new pick");
    }

    #[test]
    fn bytt_corrects_the_entry_and_does_what_the_boxes_ask() {
        let tab = tab(&["Mercha du den?"]);
        let pick = tab.pick(0, 0, None).unwrap();
        assert_eq!(replace_label("Mercha"), "Bytt \"Mercha\" med");
        assert_eq!(add_word_label(" merget "), "Legg \"merget\" i ordskatten");
        assert_eq!(
            tab.fix(pick, "merget", true, true),
            Ok(vec![
                Action::CorrectHistory {
                    unix_ms: NOW,
                    before: "Mercha du den?".into(),
                    after: "Merget du den?".into(),
                },
                Action::Change(Change::AddCorrection {
                    from: "Mercha".into(),
                    to: "merget".into()
                }),
                Action::Change(Change::AddWord("merget".into())),
            ])
        );
        assert_eq!(
            tab.fix(pick, "merget", false, false),
            Ok(vec![Action::CorrectHistory {
                unix_ms: NOW,
                before: "Mercha du den?".into(),
                after: "Merget du den?".into(),
            }])
        );
        assert_eq!(
            tab.fix(pick, " ", true, false),
            Err("Skriv hva det skal byttes med.".into())
        );
    }

    #[test]
    fn a_replacement_with_capitals_is_written_as_given() {
        let tab = tab(&["bruk post gres sql her"]);
        let pick = tab.pick(0, 2, None).unwrap();
        let pick = tab.pick(0, 6, Some(pick)).unwrap();
        let actions = tab.fix(pick, "PostgreSQL", true, false).unwrap();
        assert_eq!(
            actions,
            [
                Action::CorrectHistory {
                    unix_ms: NOW,
                    before: "bruk post gres sql her".into(),
                    after: "bruk PostgreSQL her".into(),
                },
                Action::Change(Change::AddCorrection {
                    from: "post gres sql".into(),
                    to: "PostgreSQL".into()
                }),
            ]
        );
    }

    #[test]
    fn the_same_word_again_only_adds_it_to_the_vocabulary() {
        let tab = tab(&["Vi bruker Skald."]);
        let pick = tab.pick(0, 4, None).unwrap();
        assert_eq!(tab.fix(pick, "Skald", true, false), Ok(Vec::new()));
        assert_eq!(
            tab.fix(pick, "Skald", true, true),
            Ok(vec![Action::Change(Change::AddWord("Skald".into()))])
        );
    }
}
