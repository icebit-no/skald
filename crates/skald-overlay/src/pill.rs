//! The pill's states as the app drives them, and what the pill shows at a given moment: which states are
//! on screen, how opaque, and how far each movement has come. How it looks is the theme's.
//!
//! Every time is a [`Duration`] on one clock the caller keeps, so the timing can be tested without waiting.

use std::time::Duration;

use crate::bars::{LIVE_BARS, Levels, LiveBar};
use crate::chisel::{Chisel, Pace};
use crate::futhark::{Mark, transliterate};

/// The pill appears and leaves over this long.
const FADE: Duration = Duration::from_millis(180);
/// One state gives way to the next over this long.
const CROSSFADE: Duration = Duration::from_millis(120);
/// On release the bars fold into the whole take over this long; then the chisel starts.
pub(crate) const FOLD: Duration = Duration::from_millis(280);
/// On release the dot and the time go out over this long.
const HEAD_OUT: Duration = Duration::from_millis(120);
/// On release the hint gives way to «rister …» over this long.
const LABEL_SWAP: Duration = Duration::from_millis(150);
/// The finished runes stand this long before Satt inn. The text is already inserted; the runes only show
/// once it has arrived, so they are held long enough to be seen.
const RUNES_HOLD: Duration = Duration::from_millis(400);
/// The shortest the chisel crosses the take in, so a short take still shows it moving.
const SHORTEST_CARVE: Duration = Duration::from_millis(250);
/// One breath of the recording dot.
const PULSE: Duration = Duration::from_millis(1800);

const INSERTED_FOR: Duration = Duration::from_millis(1500);
/// Hørte ingenting, and each message that a take ended without its text being inserted.
const NOTICE_FOR: Duration = Duration::from_millis(2000);
const MICROPHONE_CHANGED_FOR: Duration = Duration::from_millis(2500);

/// Whether the pill moves.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Motion {
    #[default]
    Full,
    /// Each state shows without movement: nothing fades, folds, scrolls or pulses, and the chisel stands
    /// halfway until the text arrives. The recording bars are a still picture of the last 2.4 s, replaced
    /// once a second with the time.
    Reduced,
}

#[derive(Clone, Debug, PartialEq)]
enum State {
    Recording {
        hands_free: bool,
        started: Duration,
        levels: Levels,
    },
    Carving(Box<Carving>),
    Inserted {
        text: String,
    },
    HeardNothing {
        microphone: String,
    },
    MicrophoneChanged {
        microphone: String,
    },
    NotInserted,
    TranscriptionFailed,
    NoMicrophone,
}

#[derive(Clone, Debug, PartialEq)]
struct Carving {
    hands_free: bool,
    take: Duration,
    released: Duration,
    /// The newest bars of the recording row at the release, one per slot.
    from: Vec<LiveBar>,
    /// The whole take, one height per slot.
    to: Vec<f32>,
    levels: Levels,
    chisel: Chisel,
    text: Option<(String, Vec<Mark>)>,
}

impl State {
    fn is_take(&self) -> bool {
        matches!(self, State::Recording { .. } | State::Carving(_))
    }
}

#[derive(Clone, Debug, PartialEq)]
struct Shown {
    appeared: Duration,
    state: State,
    since: Duration,
    previous: Option<State>,
    leaving: Option<Duration>,
}

/// The pill: hidden, or showing one state, perhaps while the one before it fades.
#[derive(Clone, Debug, Default)]
pub(crate) struct Pill {
    shown: Option<Shown>,
    /// A microphone change that came during a take, shown once the take is done.
    waiting: Option<State>,
    pace: Pace,
}

impl Pill {
    pub fn is_hidden(&self) -> bool {
        self.shown.is_none()
    }

    pub fn record(&mut self, hands_free: bool, now: Duration) {
        self.show(
            State::Recording {
                hands_free,
                started: now,
                levels: Levels::default(),
            },
            now,
        );
    }

    pub fn level(&mut self, dbfs: f32, now: Duration) {
        if let Some(Shown {
            state: State::Recording { levels, .. },
            leaving: None,
            ..
        }) = &mut self.shown
        {
            levels.push(dbfs, now);
        }
    }

    pub fn release(&mut self, now: Duration) {
        let Some(Shown {
            state:
                State::Recording {
                    hands_free,
                    started,
                    levels,
                },
            leaving: None,
            ..
        }) = &self.shown
        else {
            return;
        };
        // The row holds one bar more than the slots, the one leaving at the left.
        let from = levels.row(levels.position(now)).skip(1).collect();
        let expected = self.pace.expected_work(levels.audio()).saturating_sub(FOLD);
        let carving = Carving {
            hands_free: *hands_free,
            take: now.saturating_sub(*started),
            released: now,
            from,
            to: levels.fold(LIVE_BARS),
            levels: levels.clone(),
            chisel: Chisel::new(expected.max(SHORTEST_CARVE)),
            text: None,
        };
        self.replace(State::Carving(Box::new(carving)), now);
    }

    /// The text arrived and was inserted. During a carving the chisel runs out and Satt inn follows; at any
    /// other time Satt inn shows at once.
    pub fn inserted(&mut self, text: &str, now: Duration) {
        if let Some(Shown {
            state: State::Carving(carving),
            leaving: None,
            ..
        }) = &mut self.shown
        {
            if carving.text.is_some() {
                return;
            }
            let work = now.saturating_sub(carving.released);
            self.pace.record(carving.levels.audio(), work);
            carving.chisel.text_arrived(work.saturating_sub(FOLD));
            let mut runes = transliterate(text);
            runes.truncate(LIVE_BARS);
            carving.text = Some((text.to_owned(), runes));
            return;
        }
        self.show(
            State::Inserted {
                text: text.to_owned(),
            },
            now,
        );
    }

    pub fn heard_nothing(&mut self, microphone: &str, now: Duration) {
        self.show(
            State::HeardNothing {
                microphone: microphone.to_owned(),
            },
            now,
        );
    }

    pub fn not_inserted(&mut self, now: Duration) {
        self.show(State::NotInserted, now);
    }

    pub fn transcription_failed(&mut self, now: Duration) {
        self.show(State::TranscriptionFailed, now);
    }

    pub fn no_microphone(&mut self, now: Duration) {
        self.show(State::NoMicrophone, now);
    }

    /// Shown at once, or after the take on screen is done.
    pub fn microphone_changed(&mut self, microphone: &str, now: Duration) {
        let notice = State::MicrophoneChanged {
            microphone: microphone.to_owned(),
        };
        match &self.shown {
            Some(shown) if shown.leaving.is_none() && shown.state.is_take() => {
                self.waiting = Some(notice);
            }
            _ => self.show(notice, now),
        }
    }

    pub fn cancel(&mut self, now: Duration) {
        if let Some(shown) = &mut self.shown {
            shown.leaving.get_or_insert(now);
        }
    }

    /// Shows a state: replaces the one on screen, or brings the pill back.
    fn show(&mut self, state: State, now: Duration) {
        match &mut self.shown {
            Some(shown) => {
                if let Some(left) = shown.leaving.take() {
                    // Back before it was gone: appear again from as far as it had faded.
                    let faded = now.saturating_sub(left).min(FADE);
                    shown.appeared = now.saturating_sub(FADE - faded);
                }
                self.replace(state, now);
            }
            None => {
                self.shown = Some(Shown {
                    appeared: now,
                    state,
                    since: now,
                    previous: None,
                    leaving: None,
                });
            }
        }
    }

    fn replace(&mut self, state: State, at: Duration) {
        if let Some(shown) = &mut self.shown {
            let previous = std::mem::replace(&mut shown.state, state);
            shown.previous = Some(previous);
            shown.since = at;
        }
    }

    /// Moves on to whatever the clock has reached: Satt inn once the runes have stood, the end of a
    /// message, the end of leaving.
    pub fn advance(&mut self, now: Duration) {
        while let Some(shown) = &self.shown {
            if let Some(left) = shown.leaving {
                if now >= left + FADE {
                    self.shown = None;
                }
                return;
            }
            let message_ends = |shown_for| (self.waiting.clone(), shown.since + shown_for, true);
            let (next, at, takes_waiting) = match &shown.state {
                State::Carving(carving) => match (&carving.text, carving.chisel.done_at()) {
                    (Some((text, _)), Some(done)) => (
                        Some(State::Inserted { text: text.clone() }),
                        carving.released + FOLD + done + RUNES_HOLD,
                        false,
                    ),
                    _ => return,
                },
                State::Inserted { .. } => message_ends(INSERTED_FOR),
                State::HeardNothing { .. }
                | State::NotInserted
                | State::TranscriptionFailed
                | State::NoMicrophone => message_ends(NOTICE_FOR),
                State::MicrophoneChanged { .. } => message_ends(MICROPHONE_CHANGED_FOR),
                State::Recording { .. } => return,
            };
            if now < at {
                return;
            }
            if takes_waiting {
                self.waiting = None;
            }
            match next {
                Some(state) => self.replace(state, at),
                None => {
                    if let Some(shown) = &mut self.shown {
                        shown.leaving = Some(at);
                    }
                }
            }
        }
    }

    /// What the pill shows at `now`.
    pub fn scene(&self, now: Duration, motion: Motion) -> Scene<'_> {
        let Some(shown) = &self.shown else {
            return Scene {
                opacity: 0.0,
                layers: Vec::new(),
            };
        };
        let still = motion == Motion::Reduced;
        let opacity = if still {
            1.0
        } else {
            let appearing = share(now.saturating_sub(shown.appeared), FADE);
            let leaving = shown
                .leaving
                .map_or(1.0, |left| 1.0 - share(now.saturating_sub(left), FADE));
            appearing.min(leaving)
        };
        let swap = if still {
            1.0
        } else {
            share(now.saturating_sub(shown.since), CROSSFADE)
        };
        let mut layers = Vec::with_capacity(2);
        if let Some(previous) = shown.previous.as_ref().filter(|_| swap < 1.0) {
            layers.push(Layer {
                alpha: 1.0 - swap,
                look: look(previous, now, still),
            });
        }
        layers.push(Layer {
            alpha: swap,
            look: look(&shown.state, now, still),
        });
        Scene { opacity, layers }
    }
}

/// The pill at one moment.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Scene<'a> {
    /// The whole pill's opacity, as it appears and leaves.
    pub opacity: f32,
    /// Drawn in order, each at its alpha: one state, or two while one gives way to the next.
    pub layers: Vec<Layer<'a>>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Layer<'a> {
    pub alpha: f32,
    pub look: Look<'a>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Look<'a> {
    Recording {
        hands_free: bool,
        elapsed: Duration,
        /// The recording dot's breath, from 0 to 1.
        pulse: f32,
        bars: Vec<LiveBar>,
    },
    Carving {
        hands_free: bool,
        elapsed: Duration,
        /// The dot and the time, going out.
        head: f32,
        /// From the hint at 0 to «rister …» at 1.
        label: f32,
        /// From the recording row at 0 to the whole take at 1.
        fold: f32,
        from: &'a [LiveBar],
        to: &'a [f32],
        /// How far across the take the chisel is, from 0 to 1.
        chisel: f32,
        /// The runes behind the chisel, once the text has arrived.
        runes: Option<&'a [Mark]>,
    },
    Inserted {
        text: &'a str,
    },
    HeardNothing {
        microphone: &'a str,
    },
    MicrophoneChanged {
        microphone: &'a str,
    },
    NotInserted,
    TranscriptionFailed,
    NoMicrophone,
}

/// The dot's breath for a moment with no movement.
const STILL_PULSE: f32 = 0.6;
/// Where the chisel stands with no movement, until the text arrives.
const STILL_CHISEL: f32 = 0.5;

fn look(state: &State, now: Duration, still: bool) -> Look<'_> {
    match state {
        State::Recording {
            hands_free,
            started,
            levels,
        } => {
            let elapsed = now.saturating_sub(*started);
            let (pulse, position) = if still {
                let picture = *started + Duration::from_secs(elapsed.as_secs());
                (STILL_PULSE, levels.position(picture).round())
            } else {
                let breath = elapsed.as_secs_f32() / PULSE.as_secs_f32();
                (
                    0.5 + 0.5 * (breath * std::f32::consts::TAU).sin(),
                    levels.position(now),
                )
            };
            Look::Recording {
                hands_free: *hands_free,
                elapsed,
                pulse,
                bars: levels.row(position).collect(),
            }
        }
        State::Carving(carving) => {
            let since = now.saturating_sub(carving.released);
            let runes = carving.text.as_ref().map(|(_, runes)| runes.as_slice());
            let (head, label, fold, chisel) = if still {
                let chisel = if runes.is_some() { 1.0 } else { STILL_CHISEL };
                (0.0, 1.0, 1.0, chisel)
            } else {
                (
                    1.0 - share(since, HEAD_OUT),
                    share(since, LABEL_SWAP),
                    smooth(share(since, FOLD)),
                    carving.chisel.progress(since.saturating_sub(FOLD)),
                )
            };
            Look::Carving {
                hands_free: carving.hands_free,
                elapsed: carving.take,
                head,
                label,
                fold,
                from: &carving.from,
                to: &carving.to,
                chisel,
                runes,
            }
        }
        State::Inserted { text } => Look::Inserted { text },
        State::HeardNothing { microphone } => Look::HeardNothing { microphone },
        State::MicrophoneChanged { microphone } => Look::MicrophoneChanged { microphone },
        State::NotInserted => Look::NotInserted,
        State::TranscriptionFailed => Look::TranscriptionFailed,
        State::NoMicrophone => Look::NoMicrophone,
    }
}

/// How much of `whole` has passed, from 0 to 1.
fn share(part: Duration, whole: Duration) -> f32 {
    (part.as_secs_f32() / whole.as_secs_f32()).clamp(0.0, 1.0)
}

fn smooth(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bars::READING;

    fn ms(millis: u64) -> Duration {
        Duration::from_millis(millis)
    }

    /// A pill that recorded `seconds` of loud speech from 1 s on the clock.
    fn recorded(seconds: u32) -> Pill {
        let mut pill = Pill::default();
        pill.record(false, ms(1000));
        for reading in 0..seconds * 1000 / 60 {
            pill.level(-20.0, ms(1000) + READING * reading);
        }
        pill
    }

    fn only<'a>(scene: &'a Scene<'a>) -> &'a Look<'a> {
        assert_eq!(scene.layers.len(), 1, "{scene:?}");
        &scene.layers[0].look
    }

    #[test]
    fn a_hidden_pill_shows_nothing() {
        let pill = Pill::default();
        assert!(pill.is_hidden());
        let scene = pill.scene(ms(0), Motion::Full);
        assert_eq!(scene.opacity, 0.0);
        assert!(scene.layers.is_empty());
    }

    #[test]
    fn recording_fades_in_and_scrolls() {
        let mut pill = recorded(3);
        assert!(!pill.is_hidden());
        assert_eq!(pill.scene(ms(1000), Motion::Full).opacity, 0.0);
        assert!((pill.scene(ms(1090), Motion::Full).opacity - 0.5).abs() < 1e-5);
        assert_eq!(pill.scene(ms(1180), Motion::Full).opacity, 1.0);
        pill.advance(ms(3000));
        let scene = pill.scene(ms(3000), Motion::Full);
        let Look::Recording {
            elapsed,
            bars,
            hands_free,
            ..
        } = only(&scene)
        else {
            panic!("{scene:?}");
        };
        assert_eq!(*elapsed, ms(2000));
        assert!(!hands_free);
        assert_eq!(bars.len(), LIVE_BARS + 1);
    }

    #[test]
    fn the_dot_breathes() {
        let pill = recorded(3);
        let pulse = |now| match only(&pill.scene(now, Motion::Full)) {
            Look::Recording { pulse, .. } => *pulse,
            other => panic!("{other:?}"),
        };
        assert!((pulse(ms(1000)) - 0.5).abs() < 1e-5);
        assert!((pulse(ms(1450)) - 1.0).abs() < 1e-4);
        assert!((pulse(ms(2350)) - 0.0).abs() < 1e-4);
    }

    #[test]
    fn release_folds_the_take_and_starts_the_chisel_after_the_fold() {
        let mut pill = recorded(4);
        pill.release(ms(5000));
        let scene = pill.scene(ms(5000), Motion::Full);
        assert_eq!(scene.layers.len(), 2, "the recording gives way to carving");
        let carving = |now| match pill
            .scene(now, Motion::Full)
            .layers
            .last()
            .unwrap()
            .look
            .clone()
        {
            Look::Carving {
                fold,
                chisel,
                head,
                label,
                from,
                to,
                runes,
                elapsed,
                ..
            } => {
                assert_eq!(from.len(), LIVE_BARS);
                assert_eq!(to.len(), LIVE_BARS);
                assert_eq!(runes, None);
                assert_eq!(elapsed, ms(4000));
                (fold, chisel, head, label)
            }
            other => panic!("{other:?}"),
        };
        assert_eq!(carving(ms(5000)), (0.0, 0.0, 1.0, 0.0));
        let (fold, chisel, head, label) = carving(ms(5140));
        assert!((fold - 0.5).abs() < 1e-5 && chisel == 0.0 && head == 0.0 && label < 1.0);
        let (fold, chisel, _, label) = carving(ms(5280 + 100));
        assert_eq!((fold, label), (1.0, 1.0));
        assert!(chisel > 0.0);
    }

    #[test]
    fn the_chisel_is_paced_to_end_when_the_text_is_expected() {
        let mut pill = recorded(9);
        let audio = ms(9000);
        pill.release(ms(10_000));
        let expected = Pace::default().expected_work(audio);
        let chisel = |now| match &pill.scene(now, Motion::Full).layers.last().unwrap().look {
            Look::Carving { chisel, .. } => *chisel,
            other => panic!("{other:?}"),
        };
        let at_96 = ms(10_000) + FOLD + (expected - FOLD).mul_f32(0.96);
        assert!((chisel(at_96) - 0.96).abs() < 1e-3, "{}", chisel(at_96));
    }

    #[test]
    fn the_text_runs_the_chisel_out_then_the_runes_stand_then_satt_inn() {
        let mut pill = recorded(4);
        pill.release(ms(5000));
        pill.inserted("Hei, dette er en prøve.", ms(5400));
        pill.advance(ms(5400));
        let scene = pill.scene(ms(5400), Motion::Full);
        let Look::Carving {
            runes: Some(runes), ..
        } = &only(&scene)
        else {
            panic!("{scene:?}");
        };
        assert_eq!(runes.len(), transliterate("Hei, dette er en prøve.").len());

        let done = ms(5400) + ms(150);
        pill.advance(done);
        let Look::Carving { chisel, .. } = only(&pill.scene(done, Motion::Full)).clone() else {
            panic!();
        };
        assert_eq!(chisel, 1.0);

        let satt_inn = done + RUNES_HOLD;
        pill.advance(satt_inn - ms(1));
        assert!(matches!(
            only(&pill.scene(satt_inn - ms(1), Motion::Full)),
            Look::Carving { .. }
        ));
        pill.advance(satt_inn + ms(60));
        let scene = pill.scene(satt_inn + ms(60), Motion::Full);
        assert_eq!(scene.layers.len(), 2);
        assert_eq!(
            scene.layers[1].look,
            Look::Inserted {
                text: "Hei, dette er en prøve."
            }
        );
        assert!((scene.layers[1].alpha - 0.5).abs() < 1e-4);

        pill.advance(satt_inn + INSERTED_FOR + FADE / 2);
        let scene = pill.scene(satt_inn + INSERTED_FOR + FADE / 2, Motion::Full);
        assert!((scene.opacity - 0.5).abs() < 1e-4, "{scene:?}");
        pill.advance(satt_inn + INSERTED_FOR + FADE);
        assert!(pill.is_hidden());
    }

    #[test]
    fn the_wait_for_the_text_teaches_the_pace() {
        let mut pill = recorded(10);
        pill.release(ms(11_000));
        pill.inserted("Hei.", ms(13_000));
        assert!((pill.pace.seconds_per_second() - 2.0 / 9.96).abs() < 1e-3);
    }

    #[test]
    fn text_during_the_fold_runs_out_once_the_fold_is_done() {
        let mut pill = recorded(2);
        pill.release(ms(3000));
        pill.inserted("Ja.", ms(3100));
        let chisel = |now| match &pill.scene(now, Motion::Full).layers.last().unwrap().look {
            Look::Carving { chisel, .. } => *chisel,
            other => panic!("{other:?}"),
        };
        assert_eq!(chisel(ms(3000) + FOLD), 0.0);
        assert_eq!(chisel(ms(3000) + FOLD + ms(150)), 1.0);
    }

    #[test]
    fn heard_nothing_shows_two_seconds_and_leaves() {
        let mut pill = recorded(2);
        pill.release(ms(3000));
        pill.heard_nothing("MacBook Pro-mikrofon", ms(3500));
        pill.advance(ms(3700));
        assert_eq!(
            only(&pill.scene(ms(3700), Motion::Full)),
            &Look::HeardNothing {
                microphone: "MacBook Pro-mikrofon"
            }
        );
        pill.advance(ms(5500) + FADE);
        assert!(pill.is_hidden());
    }

    #[test]
    fn a_text_not_inserted_or_a_failed_transcription_shows_two_seconds_and_leaves() {
        let ends: [(fn(&mut Pill, Duration), Look<'static>); 2] = [
            (Pill::not_inserted, Look::NotInserted),
            (Pill::transcription_failed, Look::TranscriptionFailed),
        ];
        for (end, shown) in ends {
            let mut pill = recorded(2);
            pill.release(ms(3000));
            end(&mut pill, ms(3500));
            pill.advance(ms(3700));
            assert_eq!(only(&pill.scene(ms(3700), Motion::Full)), &shown);
            pill.advance(ms(5499));
            assert!(!pill.is_hidden(), "{shown:?} stands 2 s");
            pill.advance(ms(5500) + FADE);
            assert!(pill.is_hidden());
        }
    }

    #[test]
    fn no_microphone_shows_two_seconds_with_nothing_on_screen_before_it() {
        let mut pill = Pill::default();
        pill.no_microphone(ms(1000));
        assert_eq!(
            only(&pill.scene(ms(1500), Motion::Full)),
            &Look::NoMicrophone
        );
        pill.advance(ms(3000) + FADE);
        assert!(pill.is_hidden());
    }

    #[test]
    fn a_microphone_change_waits_for_a_failed_take_s_message_to_be_done() {
        let mut pill = recorded(2);
        pill.microphone_changed("Shure MV7", ms(2000));
        pill.release(ms(3000));
        pill.transcription_failed(ms(3500));
        pill.advance(ms(6000));
        assert_eq!(
            only(&pill.scene(ms(6000), Motion::Full)),
            &Look::MicrophoneChanged {
                microphone: "Shure MV7"
            }
        );
    }

    #[test]
    fn a_microphone_change_during_a_take_waits_for_the_take() {
        let mut pill = recorded(2);
        pill.microphone_changed("Shure MV7", ms(2000));
        pill.release(ms(3000));
        pill.inserted("Ja.", ms(3500));
        let satt_inn = ms(3500) + ms(150) + RUNES_HOLD;
        pill.advance(satt_inn + INSERTED_FOR + ms(500));
        let scene = pill.scene(satt_inn + INSERTED_FOR + ms(500), Motion::Full);
        assert_eq!(
            only(&scene),
            &Look::MicrophoneChanged {
                microphone: "Shure MV7"
            }
        );
        pill.advance(satt_inn + INSERTED_FOR + MICROPHONE_CHANGED_FOR + FADE);
        assert!(pill.is_hidden());
    }

    #[test]
    fn a_microphone_change_with_nothing_on_screen_shows_at_once() {
        let mut pill = Pill::default();
        pill.microphone_changed("Shure MV7", ms(0));
        assert_eq!(
            only(&pill.scene(ms(500), Motion::Full)),
            &Look::MicrophoneChanged {
                microphone: "Shure MV7"
            }
        );
        pill.advance(MICROPHONE_CHANGED_FOR + FADE);
        assert!(pill.is_hidden());
    }

    #[test]
    fn cancel_fades_the_pill_out() {
        let mut pill = recorded(1);
        pill.cancel(ms(2000));
        pill.level(-20.0, ms(2050));
        pill.advance(ms(2090));
        assert!((pill.scene(ms(2090), Motion::Full).opacity - 0.5).abs() < 1e-4);
        pill.advance(ms(2000) + FADE);
        assert!(pill.is_hidden());
    }

    #[test]
    fn a_new_take_while_leaving_comes_back_without_starting_over() {
        let mut pill = recorded(1);
        pill.cancel(ms(2000));
        pill.record(true, ms(2090));
        let scene = pill.scene(ms(2090), Motion::Full);
        assert!((scene.opacity - 0.5).abs() < 1e-4, "{scene:?}");
        pill.advance(ms(5000));
        assert!(!pill.is_hidden());
        assert!(matches!(
            only(&pill.scene(ms(5000), Motion::Full)),
            Look::Recording {
                hands_free: true,
                ..
            }
        ));
    }

    #[test]
    fn hands_free_carries_into_carving() {
        let mut pill = Pill::default();
        pill.record(true, ms(0));
        pill.release(ms(2000));
        let scene = pill.scene(ms(3000), Motion::Full);
        assert!(matches!(
            only(&scene),
            Look::Carving {
                hands_free: true,
                ..
            }
        ));
    }

    #[test]
    fn levels_after_the_release_are_not_kept() {
        let mut pill = recorded(1);
        pill.release(ms(2000));
        pill.level(-20.0, ms(2010));
        pill.release(ms(2020));
        let scene = pill.scene(ms(3000), Motion::Full);
        assert!(matches!(only(&scene), Look::Carving { .. }));
    }

    #[test]
    fn satt_inn_without_a_carving_shows_at_once() {
        let mut pill = Pill::default();
        pill.inserted("Hei.", ms(0));
        assert_eq!(
            only(&pill.scene(ms(500), Motion::Full)),
            &Look::Inserted { text: "Hei." }
        );
    }

    #[test]
    fn a_long_text_carves_as_many_runes_as_there_are_bars() {
        let mut pill = recorded(3);
        pill.release(ms(4000));
        pill.inserted(&"hei ".repeat(30), ms(4500));
        let scene = pill.scene(ms(4500), Motion::Full);
        let Look::Carving {
            runes: Some(runes), ..
        } = only(&scene)
        else {
            panic!();
        };
        assert_eq!(runes.len(), LIVE_BARS);
    }

    #[test]
    fn reduced_motion_shows_each_state_still() {
        let mut pill = recorded(3);
        fn still(pill: &Pill, now: Duration) -> Scene<'_> {
            pill.scene(now, Motion::Reduced)
        }
        assert_eq!(still(&pill, ms(1000)).opacity, 1.0);
        let bars = |now| match only(&still(&pill, now)) {
            Look::Recording { bars, pulse, .. } => {
                assert_eq!(*pulse, STILL_PULSE);
                bars.clone()
            }
            other => panic!("{other:?}"),
        };
        assert_eq!(
            bars(ms(2100)),
            bars(ms(2900)),
            "the picture holds for a second"
        );
        assert_ne!(
            bars(ms(2900)),
            bars(ms(3100)),
            "and is replaced with the time"
        );
        for bar in bars(ms(2500)) {
            assert_eq!(
                bar.from_right,
                bar.from_right.round(),
                "no bar stands between places"
            );
        }

        pill.release(ms(4000));
        let carving = |pill: &Pill, now| match only(&still(pill, now)).clone() {
            Look::Carving {
                fold,
                chisel,
                head,
                label,
                ..
            } => (fold, chisel, head, label),
            other => panic!("{other:?}"),
        };
        assert_eq!(carving(&pill, ms(4000)), (1.0, STILL_CHISEL, 0.0, 1.0));
        assert_eq!(carving(&pill, ms(9000)), (1.0, STILL_CHISEL, 0.0, 1.0));
        pill.inserted("Ja.", ms(9000));
        assert_eq!(carving(&pill, ms(9000)).1, 1.0);
        pill.advance(ms(9000) + ms(150) + RUNES_HOLD);
        assert_eq!(
            only(&still(&pill, ms(9000) + ms(150) + RUNES_HOLD)),
            &Look::Inserted { text: "Ja." }
        );
    }
}
