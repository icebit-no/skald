//! The last of the silence checks (docs/design.md, Silence, point 3): Whisper turns silence into «ja»,
//! «ja takk» or «den satt», so a take whose whole text is such a phrase, and in which VAD found almost no
//! speech, ends with «Hørte ingenting».

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::text::folded_words;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Silence {
    /// What the model writes for silence. The list grows from real cases, never from guesses.
    pub known_phrases: Vec<String>,
    /// A known phrase counts as silence only when VAD found less speech than this.
    pub known_phrase_max_speech_ms: u32,
    /// Keys a newer Skald wrote, written back unchanged (see the `config` module).
    #[serde(flatten)]
    unknown: toml::Table,
}

impl Default for Silence {
    fn default() -> Self {
        Self {
            known_phrases: ["ja", "ja takk", "den satt", "det er"]
                .map(String::from)
                .to_vec(),
            known_phrase_max_speech_ms: 1000,
            unknown: toml::Table::new(),
        }
    }
}

impl Silence {
    /// Whether `text`, as the model wrote it before corrections, is one of the known phrases while `speech`,
    /// the speech VAD measured in the take, stays under the limit. Case, punctuation and spacing are
    /// ignored, so «Ja, takk!» is «ja takk»; the phrase must be the whole text.
    pub fn is_known_phrase(&self, text: &str, speech: Duration) -> bool {
        if speech >= Duration::from_millis(self.known_phrase_max_speech_ms.into()) {
            return false;
        }
        let words = folded_words(text);
        !words.is_empty()
            && self
                .known_phrases
                .iter()
                .any(|phrase| folded_words(phrase) == words)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LITTLE: Duration = Duration::from_millis(400);
    const PLENTY: Duration = Duration::from_millis(2500);

    #[test]
    fn a_known_phrase_with_almost_no_speech_is_silence() {
        let silence = Silence::default();
        assert!(silence.is_known_phrase("Ja.", LITTLE));
        assert!(silence.is_known_phrase(" Ja takk.", LITTLE));
        assert!(silence.is_known_phrase("Den satt.", LITTLE));
    }

    #[test]
    fn bokfinks_text_for_silence_in_nynorsk_is_known() {
        assert!(Silence::default().is_known_phrase("Det er.", LITTLE));
        assert!(!Silence::default().is_known_phrase("Det er fint.", LITTLE));
    }

    #[test]
    fn case_punctuation_and_spacing_are_ignored() {
        let silence = Silence::default();
        assert!(silence.is_known_phrase("JA, TAKK!", LITTLE));
        assert!(silence.is_known_phrase("ja  …  takk", LITTLE));
    }

    #[test]
    fn real_speech_is_never_silence() {
        let silence = Silence::default();
        assert!(!silence.is_known_phrase("Ja takk.", PLENTY));
        let limit = Duration::from_millis(silence.known_phrase_max_speech_ms.into());
        assert!(!silence.is_known_phrase("Ja takk.", limit));
        assert!(silence.is_known_phrase("Ja takk.", limit - Duration::from_millis(1)));
    }

    #[test]
    fn the_phrase_must_be_the_whole_text() {
        let silence = Silence::default();
        assert!(!silence.is_known_phrase("Ja takk for maten.", LITTLE));
        assert!(!silence.is_known_phrase("Takk.", LITTLE));
        assert!(!silence.is_known_phrase("Jada.", LITTLE));
        assert!(!silence.is_known_phrase("", LITTLE));
        assert!(!silence.is_known_phrase(" . ", LITTLE));
    }

    #[test]
    fn phrases_with_æøå_match_regardless_of_case() {
        let silence = Silence {
            known_phrases: vec!["Å ja".into(), "nå så".into()],
            ..Silence::default()
        };
        assert!(silence.is_known_phrase("å ja!", LITTLE));
        assert!(silence.is_known_phrase("NÅ SÅ.", LITTLE));
        assert!(!silence.is_known_phrase("Ja.", LITTLE));
    }

    #[test]
    fn a_blank_phrase_matches_nothing() {
        let silence = Silence {
            known_phrases: vec![String::new(), " , ".into()],
            ..Silence::default()
        };
        assert!(!silence.is_known_phrase("", LITTLE));
        assert!(!silence.is_known_phrase("…", LITTLE));
    }
}
