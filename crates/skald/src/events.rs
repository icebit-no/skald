//! What the dictation loop tells the app's screen, the overlay and the menu bar item, as it happens. In
//! the terminal nobody is told; the lines in `say` carry the same news in words.

use std::sync::Arc;

use skald_core::Config;
use skald_core::state::State;

use crate::pipeline::{Outcome, Report};

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Event {
    /// A take started recording. Takes are numbered as they start, so a late end reaches the overlay only
    /// while its take is the one on screen.
    Recording {
        take: u64,
        hands_free: bool,
    },
    /// One level reading of the take recording, in dBFS.
    Level(f32),
    /// The take ended and waits for its text.
    Carving {
        take: u64,
    },
    Cancelled {
        take: u64,
    },
    /// A take armed with no microphone connected, so none started.
    NoMicrophone,
    /// The take's text was inserted, or why not.
    Ended {
        take: u64,
        ending: Ending,
    },
    /// The microphone takes are made with changed to this one.
    MicrophoneChanged(String),
    Microphones(MicrophoneList),
    /// The history and the correction counts, read at start and again after each take or correction.
    State(State),
    /// The speech model has loaded, so it can count the ordskatt's tokens.
    ModelLoaded,
    /// The settings in use, read at start and again whenever they change.
    Settings(Arc<Config>),
    /// The loop has ended and the model is dropped; the process may exit.
    Stopped,
}

/// How a take ended, as the overlay shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Ending {
    Inserted(String),
    HeardNothing {
        microphone: String,
    },
    /// The text is in the history but could not be inserted.
    NotInserted,
    /// The model failed on the take.
    Failed,
}

impl Ending {
    pub(crate) fn of(report: &Report) -> Self {
        match &report.outcome {
            Outcome::Inserted { text, .. } => Self::Inserted(text.clone()),
            Outcome::HeardNothing(_) => Self::HeardNothing {
                microphone: report.microphone.clone(),
            },
            Outcome::NotInserted { .. } => Self::NotInserted,
            Outcome::Failed(_) => Self::Failed,
        }
    }
}

/// The microphones connected, by the names the system gives them, in its order, and the one a take started
/// now would use.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct MicrophoneList {
    pub(crate) connected: Vec<String>,
    pub(crate) in_use: Option<String>,
}

#[cfg(test)]
mod tests {
    use skald_core::silence_gate::Reason;
    use skald_input::{InsertError, Inserted};

    use super::*;
    use crate::pipeline::Timings;

    fn report(outcome: Outcome) -> Report {
        Report {
            take: 3,
            microphone: "Shure MV7".into(),
            outcome,
            timings: Timings::default(),
            state_error: None,
            state: State::default(),
        }
    }

    #[test]
    fn a_take_s_end_is_its_text_the_microphone_it_heard_nothing_on_or_what_went_wrong() {
        assert_eq!(
            Ending::of(&report(Outcome::Inserted {
                text: "Hei.".into(),
                inserted: Inserted::PreviousRestored,
                complete: true,
            })),
            Ending::Inserted("Hei.".into())
        );
        assert_eq!(
            Ending::of(&report(Outcome::HeardNothing(Reason::TooLittleSpeech))),
            Ending::HeardNothing {
                microphone: "Shure MV7".into()
            }
        );
        assert_eq!(
            Ending::of(&report(Outcome::NotInserted {
                text: "Hei.".into(),
                error: InsertError::KeysHeld,
                complete: true,
            })),
            Ending::NotInserted
        );
        assert_eq!(
            Ending::of(&report(Outcome::Failed("out of memory".into()))),
            Ending::Failed
        );
    }
}
