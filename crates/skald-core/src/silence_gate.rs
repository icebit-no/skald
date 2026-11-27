//! The silence gate (docs/design.md, Silence): whether a take becomes text. Whisper turns silence into
//! «ja», «ja takk» or «den satt», so a take passes these checks in order, and the first that fails ends
//! it with «Hørte ingenting»:
//!
//! 1. **Level:** the loudest reading reached speech level. Otherwise neither VAD nor the model runs.
//! 2. **Speech detection:** Silero VAD found at least [`MIN_SPEECH`]. Otherwise the model does not run.
//! 3. **Known phrases:** the whole text is not a known silence phrase said with almost no speech.
//!
//! A text without a word in it, nothing or only punctuation («...»), ends the take the same way, before
//! the known phrases: there is nothing to insert. The model's no-speech probability is not a check:
//! NB-Whisper's is 0.000 on every silent take, and Bokfink has none (D-22).
//!
//! Each check is a function of its own, so a caller skips the work a failed check makes pointless;
//! [`decide`] runs them all on a take whose text is known.

use std::time::Duration;

use crate::silence::Silence;
use crate::text::is_word_char;

/// What a take that ends without text shows.
pub const HEARD_NOTHING: &str = "Hørte ingenting";

/// A take whose loudest level reading, the RMS of 60 ms in dBFS, stays below this holds no speech.
///
/// Evidence: in the takes of docs/research/engine-check.md, every take with speech reads −17.9 dBFS or
/// more at its loudest, and the room tone of three FLEURS recordings before the speaker starts −62.0,
/// −55.1 and −54.0. diktafon on a MacBook microphone read −33 at its loudest in a dictation, −48 for
/// ordinary speech and −62 to −67 between words. The limit sits low on purpose: VAD is the check that
/// tells speech from noise (it found none in clicks and noise as loud as −31 dBFS), and a quiet voice far
/// from the microphone must still get through. M3's recordings with real microphones are the real test.
pub const SPEECH_LEVEL_DBFS: f32 = -50.0;

/// The least speech VAD must find in a take, the sum of its segments (docs/design.md, Silence).
///
/// Evidence: Silero VAD v6.2 found 0 ms in each of the engine check's ten takes without speech and at
/// least 380 ms in every take with speech, the shortest a lone «Hei.» (docs/research/engine-check.md,
/// section 3).
pub const MIN_SPEECH: Duration = Duration::from_millis(250);

/// Why a take ended with «Hørte ingenting».
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    BelowSpeechLevel,
    TooLittleSpeech,
    NoWords,
    KnownPhrase,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Insert,
    HeardNothing(Reason),
}

/// Check 1, from the take's loudest level reading in dBFS.
pub fn check_level(loudest_dbfs: f32) -> Result<(), Reason> {
    if loudest_dbfs >= SPEECH_LEVEL_DBFS {
        Ok(())
    } else {
        Err(Reason::BelowSpeechLevel)
    }
}

/// Check 2, from the speech VAD found in the take.
pub fn check_speech(speech: Duration) -> Result<(), Reason> {
    if speech >= MIN_SPEECH {
        Ok(())
    } else {
        Err(Reason::TooLittleSpeech)
    }
}

/// Words, then check 3, from the model's text before corrections and the speech VAD found.
pub fn check_text(text: &str, speech: Duration, silence: &Silence) -> Result<(), Reason> {
    if !text.chars().any(is_word_char) {
        Err(Reason::NoWords)
    } else if silence.is_known_phrase(text, speech) {
        Err(Reason::KnownPhrase)
    } else {
        Ok(())
    }
}

/// The whole gate, in order, for a take with its loudest level reading in dBFS, the speech VAD found and
/// the model's text before corrections.
pub fn decide(loudest_dbfs: f32, speech: Duration, text: &str, silence: &Silence) -> Decision {
    let checks = check_level(loudest_dbfs)
        .and_then(|()| check_speech(speech))
        .and_then(|()| check_text(text, speech, silence));
    match checks {
        Ok(()) => Decision::Insert,
        Err(reason) => Decision::HeardNothing(reason),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOUD: f32 = -20.0;
    const PLENTY: Duration = Duration::from_millis(2500);
    const LITTLE: Duration = Duration::from_millis(400);

    fn decide(loudest_dbfs: f32, speech: Duration, text: &str) -> Decision {
        super::decide(loudest_dbfs, speech, text, &Silence::default())
    }

    #[test]
    fn speech_with_words_is_inserted() {
        assert_eq!(
            decide(LOUD, PLENTY, "Hei, dette er en prøve."),
            Decision::Insert
        );
    }

    #[test]
    fn a_take_below_speech_level_heard_nothing() {
        let quiet = Decision::HeardNothing(Reason::BelowSpeechLevel);
        assert_eq!(decide(-100.0, PLENTY, "Hei."), quiet);
        assert_eq!(decide(-54.0, PLENTY, "Hei."), quiet);
        assert_eq!(decide(SPEECH_LEVEL_DBFS - 0.1, PLENTY, "Hei."), quiet);
        assert_eq!(decide(SPEECH_LEVEL_DBFS, PLENTY, "Hei."), Decision::Insert);
    }

    #[test]
    fn less_than_250_ms_of_speech_heard_nothing() {
        let little = Decision::HeardNothing(Reason::TooLittleSpeech);
        assert_eq!(
            decide(LOUD, Duration::ZERO, "Hva er det som skjer?"),
            little
        );
        assert_eq!(decide(LOUD, Duration::from_millis(249), "Hei."), little);
        assert_eq!(decide(LOUD, MIN_SPEECH, "Hei."), Decision::Insert);
        // The shortest word in the engine check.
        assert_eq!(
            decide(LOUD, Duration::from_millis(380), "Hei."),
            Decision::Insert
        );
    }

    #[test]
    fn a_text_without_words_heard_nothing() {
        let nothing = Decision::HeardNothing(Reason::NoWords);
        assert_eq!(decide(LOUD, PLENTY, ""), nothing);
        assert_eq!(decide(LOUD, PLENTY, " ... "), nothing);
        assert_eq!(decide(LOUD, PLENTY, "... ..."), nothing);
        assert_eq!(decide(LOUD, PLENTY, "Å."), Decision::Insert);
        assert_eq!(decide(LOUD, PLENTY, "42"), Decision::Insert);
    }

    #[test]
    fn a_known_phrase_with_almost_no_speech_heard_nothing() {
        let known = Decision::HeardNothing(Reason::KnownPhrase);
        assert_eq!(decide(LOUD, LITTLE, "Ja takk."), known);
        // Bokfink's text for silence in nynorsk.
        assert_eq!(decide(LOUD, LITTLE, "Det er."), known);
        assert_eq!(decide(LOUD, PLENTY, "Ja takk."), Decision::Insert);
    }

    #[test]
    fn the_first_failed_check_is_the_reason() {
        assert_eq!(
            decide(-80.0, Duration::ZERO, ""),
            Decision::HeardNothing(Reason::BelowSpeechLevel)
        );
        assert_eq!(
            decide(LOUD, Duration::ZERO, "Ja."),
            Decision::HeardNothing(Reason::TooLittleSpeech)
        );
    }

    #[test]
    fn each_check_alone() {
        assert_eq!(check_level(LOUD), Ok(()));
        assert_eq!(check_level(-60.0), Err(Reason::BelowSpeechLevel));
        assert_eq!(check_speech(PLENTY), Ok(()));
        assert_eq!(check_speech(Duration::ZERO), Err(Reason::TooLittleSpeech));
        let silence = Silence::default();
        assert_eq!(check_text("Hei.", LITTLE, &silence), Ok(()));
        assert_eq!(check_text("…", LITTLE, &silence), Err(Reason::NoWords));
        assert_eq!(
            check_text("Ja.", LITTLE, &silence),
            Err(Reason::KnownPhrase)
        );
    }
}
