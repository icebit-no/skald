//! Hold-to-talk as pure logic over timestamped key changes, so every rule is tested with synthetic events
//! and a fake clock.
//!
//! A take arms once the hold key has been down alone for [`ARM_AFTER`]. The press is reported at once,
//! before arming, so the caller can open the microphone and keep the speech of those first moments; each
//! press then ends in exactly one of [`HoldEvent::Armed`] and [`HoldEvent::NotArmed`]. Any other key pressed
//! while the hold key is down spoils the hold: before arming it is not armed, after arming the take is
//! cancelled, and either way nothing more happens until the hold key comes up. While a take runs, the key's
//! real state is checked every [`WATCHDOG_EVERY`], which ends a take whose release was never reported.
//!
//! A hands-free take starts at one press of the hands-free chord and stops at the next, with no key held
//! between. While it runs the hold key starts nothing and other keys cancel nothing, so the user can type
//! and use shortcuts. It can also end without the chord, at the cap or when its microphone goes away;
//! the caller then says so with [`HoldMachine::hands_free_ended`], so the next press starts a new one.

use std::time::{Duration, Instant};

/// How long the hold key must be down alone before a take starts.
pub const ARM_AFTER: Duration = Duration::from_millis(200);

/// How often the key's real state is checked while it is down past arming: the longest a missed release
/// keeps a take running.
pub const WATCHDOG_EVERY: Duration = Duration::from_millis(250);

/// The keyboard as the operating system sees it at one moment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeySample {
    pub hold_key_down: bool,
    /// Whether a modifier other than the hold key is down, its twin on the other side included.
    pub other_modifiers_down: bool,
}

/// What a hold means for the take.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HoldEvent {
    /// The hold key went down alone. A take may follow, and speech may begin at once, so this is the moment
    /// to start listening; [`HoldEvent::Armed`] or [`HoldEvent::NotArmed`] follows.
    Pressed { at: Instant },
    /// The take starts. Speech may have begun at `pressed_at`, before arming.
    Armed { pressed_at: Instant },
    /// The hold ended before it armed: the key came up too soon, or another key joined it, as in ⌘C.
    /// Nothing starts, and whatever was heard since [`HoldEvent::Pressed`] is thrown away.
    NotArmed,
    /// The hold key came up: the take ends and is transcribed.
    Released,
    /// The hold key was found up although its release was never reported: the take ends and is
    /// transcribed, as for [`HoldEvent::Released`].
    ReleaseMissed,
    /// Another key joined the hold key: the take is thrown away. Also a hands-free take whose key
    /// events can no longer be trusted, as after [`HoldMachine::reset`].
    Cancelled,
    /// The hands-free chord was pressed: a hands-free take starts, and speech may begin at once.
    HandsFreeStarted { at: Instant },
    /// The hands-free chord was pressed again: the hands-free take ends and is transcribed.
    HandsFreeStopped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Idle,
    Pressed {
        since: Instant,
    },
    Armed {
        checked: Instant,
    },
    /// The key is still down, but this hold will not start a take.
    WaitingForRelease {
        checked: Instant,
    },
    HandsFree,
}

#[derive(Debug, Clone)]
pub struct HoldMachine {
    state: State,
    /// The hands-free chord's key is down, so its autorepeat is not another press.
    chord_down: bool,
}

impl Default for HoldMachine {
    fn default() -> Self {
        Self::new()
    }
}

impl HoldMachine {
    pub fn new() -> Self {
        Self {
            state: State::Idle,
            chord_down: false,
        }
    }

    /// Whether a take held with the hold key is running.
    pub fn is_armed(&self) -> bool {
        matches!(self.state, State::Armed { .. })
    }

    pub fn is_hands_free(&self) -> bool {
        self.state == State::HandsFree
    }

    /// The hold key went down or up; `sample` says which.
    pub fn hold_key_changed(&mut self, now: Instant, sample: KeySample) -> Option<HoldEvent> {
        if self.state == State::HandsFree {
            return None;
        }
        if !sample.hold_key_down {
            let event = match self.state {
                State::Armed { .. } => Some(HoldEvent::Released),
                State::Pressed { .. } => Some(HoldEvent::NotArmed),
                State::Idle | State::WaitingForRelease { .. } | State::HandsFree => None,
            };
            self.state = State::Idle;
            return event;
        }
        if self.state != State::Idle {
            return None;
        }
        if sample.other_modifiers_down {
            self.state = State::WaitingForRelease { checked: now };
            None
        } else {
            self.state = State::Pressed { since: now };
            Some(HoldEvent::Pressed { at: now })
        }
    }

    /// Any key other than the hold key went down: a key, a modifier or a mouse button.
    pub fn other_key_pressed(&mut self, now: Instant) -> Option<HoldEvent> {
        match self.state {
            State::Idle | State::WaitingForRelease { .. } | State::HandsFree => None,
            State::Pressed { .. } => {
                self.state = State::WaitingForRelease { checked: now };
                Some(HoldEvent::NotArmed)
            }
            State::Armed { .. } => {
                self.state = State::WaitingForRelease { checked: now };
                Some(HoldEvent::Cancelled)
            }
        }
    }

    /// When [`HoldMachine::tick`] is next due, if at all.
    pub fn next_tick(&self) -> Option<Instant> {
        match self.state {
            State::Idle | State::HandsFree => None,
            State::Pressed { since } => Some(since + ARM_AFTER),
            State::Armed { checked } | State::WaitingForRelease { checked } => {
                Some(checked + WATCHDOG_EVERY)
            }
        }
    }

    /// Arms a take that has been held long enough, and checks a running one against the key's real
    /// state. A tick before it is due only applies what `sample` contradicts.
    pub fn tick(&mut self, now: Instant, sample: KeySample) -> Option<HoldEvent> {
        match self.state {
            State::Idle | State::HandsFree => None,
            State::Pressed { since } => {
                if !sample.hold_key_down {
                    self.state = State::Idle;
                    Some(HoldEvent::NotArmed)
                } else if sample.other_modifiers_down {
                    self.state = State::WaitingForRelease { checked: now };
                    Some(HoldEvent::NotArmed)
                } else if now >= since + ARM_AFTER {
                    self.state = State::Armed { checked: now };
                    Some(HoldEvent::Armed { pressed_at: since })
                } else {
                    None
                }
            }
            State::Armed { .. } => {
                if !sample.hold_key_down {
                    self.state = State::Idle;
                    Some(HoldEvent::ReleaseMissed)
                } else if sample.other_modifiers_down {
                    self.state = State::WaitingForRelease { checked: now };
                    Some(HoldEvent::Cancelled)
                } else {
                    self.state = State::Armed { checked: now };
                    None
                }
            }
            State::WaitingForRelease { .. } => {
                self.state = if sample.hold_key_down {
                    State::WaitingForRelease { checked: now }
                } else {
                    State::Idle
                };
                None
            }
        }
    }

    /// The hands-free chord went down: starts a hands-free take, or stops the one running. Its key
    /// repeating while held is not another press. A hold that has not yet ended is spoiled by it, as by
    /// any other key, and starts nothing.
    pub fn hands_free_pressed(&mut self, now: Instant) -> Option<HoldEvent> {
        if std::mem::replace(&mut self.chord_down, true) {
            return None;
        }
        match self.state {
            State::HandsFree => {
                self.state = State::Idle;
                Some(HoldEvent::HandsFreeStopped)
            }
            State::Idle | State::WaitingForRelease { .. } => {
                self.state = State::HandsFree;
                Some(HoldEvent::HandsFreeStarted { at: now })
            }
            State::Pressed { .. } | State::Armed { .. } => self.other_key_pressed(now),
        }
    }

    /// The hands-free chord's key came up, so its next press counts.
    pub fn hands_free_released(&mut self) {
        self.chord_down = false;
    }

    /// The hands-free take ended without the chord: at the cap, when its microphone went away, or
    /// because it could not start. The next press of the chord starts a new one.
    pub fn hands_free_ended(&mut self) {
        if self.state == State::HandsFree {
            self.state = State::Idle;
        }
    }

    /// Forgets the hold, for when key events can no longer be trusted to arrive. A running take, held or
    /// hands-free, is cancelled, and a press not yet armed is not armed.
    pub fn reset(&mut self) -> Option<HoldEvent> {
        let event = match self.state {
            State::Armed { .. } | State::HandsFree => Some(HoldEvent::Cancelled),
            State::Pressed { .. } => Some(HoldEvent::NotArmed),
            State::Idle | State::WaitingForRelease { .. } => None,
        };
        self.state = State::Idle;
        self.chord_down = false;
        event
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALONE: KeySample = KeySample {
        hold_key_down: true,
        other_modifiers_down: false,
    };
    const WITH_OTHERS: KeySample = KeySample {
        hold_key_down: true,
        other_modifiers_down: true,
    };
    const UP: KeySample = KeySample {
        hold_key_down: false,
        other_modifiers_down: false,
    };

    /// A clock that only moves when told to.
    struct Clock {
        start: Instant,
    }

    impl Clock {
        fn new() -> Self {
            Self {
                start: Instant::now(),
            }
        }

        fn at(&self, ms: u64) -> Instant {
            self.start + Duration::from_millis(ms)
        }
    }

    /// Ticks the machine at every due time up to `until`, as the listener does, and collects the events.
    fn run_until(machine: &mut HoldMachine, until: Instant, sample: KeySample) -> Vec<HoldEvent> {
        let mut events = Vec::new();
        while let Some(due) = machine.next_tick() {
            if due > until {
                break;
            }
            events.extend(machine.tick(due, sample));
        }
        events
    }

    fn armed_machine(clock: &Clock) -> HoldMachine {
        let mut machine = HoldMachine::new();
        assert_eq!(
            machine.hold_key_changed(clock.at(0), ALONE),
            Some(HoldEvent::Pressed { at: clock.at(0) })
        );
        assert_eq!(
            run_until(&mut machine, clock.at(200), ALONE),
            [HoldEvent::Armed {
                pressed_at: clock.at(0)
            }]
        );
        machine
    }

    #[test]
    fn a_hold_arms_after_200_ms_and_a_release_ends_it() {
        let clock = Clock::new();
        let mut machine = HoldMachine::new();

        assert_eq!(
            machine.hold_key_changed(clock.at(0), ALONE),
            Some(HoldEvent::Pressed { at: clock.at(0) })
        );
        assert_eq!(machine.next_tick(), Some(clock.at(200)));
        assert_eq!(run_until(&mut machine, clock.at(199), ALONE), []);
        assert_eq!(
            run_until(&mut machine, clock.at(200), ALONE),
            [HoldEvent::Armed {
                pressed_at: clock.at(0)
            }]
        );
        assert!(machine.is_armed());
        assert_eq!(
            machine.hold_key_changed(clock.at(3000), UP),
            Some(HoldEvent::Released)
        );
        assert_eq!(machine.next_tick(), None);
    }

    #[test]
    fn a_late_tick_still_arms_and_reports_when_the_key_went_down() {
        let clock = Clock::new();
        let mut machine = HoldMachine::new();
        machine.hold_key_changed(clock.at(0), ALONE);

        assert_eq!(
            machine.tick(clock.at(350), ALONE),
            Some(HoldEvent::Armed {
                pressed_at: clock.at(0)
            })
        );
    }

    #[test]
    fn an_early_tick_does_not_arm() {
        let clock = Clock::new();
        let mut machine = HoldMachine::new();
        machine.hold_key_changed(clock.at(0), ALONE);

        assert_eq!(machine.tick(clock.at(150), ALONE), None);
        assert_eq!(machine.next_tick(), Some(clock.at(200)));
    }

    #[test]
    fn a_tap_shorter_than_200_ms_is_not_armed() {
        let clock = Clock::new();
        let mut machine = HoldMachine::new();
        machine.hold_key_changed(clock.at(0), ALONE);

        assert_eq!(
            machine.hold_key_changed(clock.at(120), UP),
            Some(HoldEvent::NotArmed)
        );
        assert_eq!(machine.next_tick(), None);
        assert_eq!(run_until(&mut machine, clock.at(1000), UP), []);
    }

    #[test]
    fn a_shortcut_typed_before_arming_is_not_armed() {
        // ⌘C: the hold key, then C within 200 ms.
        let clock = Clock::new();
        let mut machine = HoldMachine::new();
        machine.hold_key_changed(clock.at(0), ALONE);

        assert_eq!(
            machine.other_key_pressed(clock.at(80)),
            Some(HoldEvent::NotArmed)
        );
        assert_eq!(machine.other_key_pressed(clock.at(90)), None);
        assert_eq!(run_until(&mut machine, clock.at(2000), ALONE), []);
        assert!(!machine.is_armed());
        assert_eq!(machine.hold_key_changed(clock.at(2100), UP), None);
    }

    #[test]
    fn another_key_after_arming_cancels_the_take() {
        let clock = Clock::new();
        let mut machine = armed_machine(&clock);

        assert_eq!(
            machine.other_key_pressed(clock.at(900)),
            Some(HoldEvent::Cancelled)
        );
        assert!(!machine.is_armed());
        assert_eq!(machine.other_key_pressed(clock.at(950)), None);
        assert_eq!(run_until(&mut machine, clock.at(5000), ALONE), []);
        assert_eq!(machine.hold_key_changed(clock.at(5100), UP), None);
    }

    #[test]
    fn a_cancelled_hold_does_not_arm_again_while_the_key_stays_down() {
        let clock = Clock::new();
        let mut machine = HoldMachine::new();
        machine.hold_key_changed(clock.at(0), ALONE);
        machine.other_key_pressed(clock.at(50));

        // A repeated down report for the same hold changes nothing.
        assert_eq!(machine.hold_key_changed(clock.at(400), ALONE), None);
        assert_eq!(run_until(&mut machine, clock.at(3000), ALONE), []);
        assert!(!machine.is_armed());
    }

    #[test]
    fn the_key_works_again_after_a_cancelled_hold_is_released() {
        let clock = Clock::new();
        let mut machine = HoldMachine::new();
        machine.hold_key_changed(clock.at(0), ALONE);
        machine.other_key_pressed(clock.at(50));
        machine.hold_key_changed(clock.at(300), UP);

        machine.hold_key_changed(clock.at(1000), ALONE);
        assert_eq!(
            run_until(&mut machine, clock.at(1200), ALONE),
            [HoldEvent::Armed {
                pressed_at: clock.at(1000)
            }]
        );
    }

    #[test]
    fn a_modifier_already_down_when_the_key_is_pressed_is_not_even_a_press() {
        // ⇧ held, then the hold key: a chord, not dictation.
        let clock = Clock::new();
        let mut machine = HoldMachine::new();

        assert_eq!(machine.hold_key_changed(clock.at(0), WITH_OTHERS), None);
        assert_eq!(run_until(&mut machine, clock.at(2000), ALONE), []);
        assert!(!machine.is_armed());
    }

    #[test]
    fn a_modifier_found_down_at_arming_time_is_not_armed() {
        let clock = Clock::new();
        let mut machine = HoldMachine::new();
        machine.hold_key_changed(clock.at(0), ALONE);

        assert_eq!(
            run_until(&mut machine, clock.at(200), WITH_OTHERS),
            [HoldEvent::NotArmed]
        );
        assert!(!machine.is_armed());
        assert_eq!(run_until(&mut machine, clock.at(2000), ALONE), []);
    }

    #[test]
    fn a_key_found_up_at_arming_time_is_not_armed() {
        let clock = Clock::new();
        let mut machine = HoldMachine::new();
        machine.hold_key_changed(clock.at(0), ALONE);

        assert_eq!(
            run_until(&mut machine, clock.at(200), UP),
            [HoldEvent::NotArmed]
        );
        assert_eq!(machine.next_tick(), None);
    }

    #[test]
    fn the_watchdog_checks_every_250_ms_while_the_key_is_down() {
        let clock = Clock::new();
        let mut machine = armed_machine(&clock);

        assert_eq!(machine.next_tick(), Some(clock.at(450)));
        assert_eq!(run_until(&mut machine, clock.at(10_000), ALONE), []);
        assert!(machine.is_armed());
        assert_eq!(machine.next_tick(), Some(clock.at(10_200)));
    }

    #[test]
    fn the_watchdog_ends_a_take_whose_release_was_missed() {
        let clock = Clock::new();
        let mut machine = armed_machine(&clock);
        run_until(&mut machine, clock.at(4000), ALONE);

        assert_eq!(
            run_until(&mut machine, clock.at(4250), UP),
            [HoldEvent::ReleaseMissed]
        );
        assert_eq!(machine.next_tick(), None);
        // The release, reported late, changes nothing.
        assert_eq!(machine.hold_key_changed(clock.at(4300), UP), None);
    }

    #[test]
    fn the_watchdog_cancels_a_take_when_it_finds_another_modifier_down() {
        let clock = Clock::new();
        let mut machine = armed_machine(&clock);

        assert_eq!(
            run_until(&mut machine, clock.at(450), WITH_OTHERS),
            [HoldEvent::Cancelled]
        );
        assert_eq!(run_until(&mut machine, clock.at(3000), ALONE), []);
    }

    #[test]
    fn the_watchdog_ends_a_cancelled_hold_whose_release_was_missed() {
        let clock = Clock::new();
        let mut machine = HoldMachine::new();
        machine.hold_key_changed(clock.at(0), ALONE);
        machine.other_key_pressed(clock.at(50));

        assert_eq!(run_until(&mut machine, clock.at(300), UP), []);
        assert_eq!(machine.next_tick(), None);
        machine.hold_key_changed(clock.at(1000), ALONE);
        assert_eq!(
            run_until(&mut machine, clock.at(1200), ALONE),
            [HoldEvent::Armed {
                pressed_at: clock.at(1000)
            }]
        );
    }

    #[test]
    fn other_keys_and_ticks_while_idle_do_nothing() {
        let clock = Clock::new();
        let mut machine = HoldMachine::new();

        assert_eq!(machine.other_key_pressed(clock.at(0)), None);
        assert_eq!(machine.tick(clock.at(10), ALONE), None);
        assert_eq!(machine.hold_key_changed(clock.at(20), UP), None);
        assert_eq!(machine.next_tick(), None);
    }

    #[test]
    fn a_repeated_down_report_while_pressed_is_not_a_second_press() {
        let clock = Clock::new();
        let mut machine = HoldMachine::new();
        machine.hold_key_changed(clock.at(0), ALONE);

        assert_eq!(machine.hold_key_changed(clock.at(100), ALONE), None);
        assert_eq!(machine.next_tick(), Some(clock.at(200)));
    }

    #[test]
    fn a_repeated_down_report_while_armed_changes_nothing() {
        let clock = Clock::new();
        let mut machine = armed_machine(&clock);

        assert_eq!(machine.hold_key_changed(clock.at(300), ALONE), None);
        assert!(machine.is_armed());
    }

    #[test]
    fn reset_cancels_a_running_take_and_ends_a_press_not_yet_armed() {
        let clock = Clock::new();
        let mut machine = armed_machine(&clock);
        assert_eq!(machine.reset(), Some(HoldEvent::Cancelled));
        assert_eq!(machine.next_tick(), None);

        machine.hold_key_changed(clock.at(1000), ALONE);
        assert_eq!(machine.reset(), Some(HoldEvent::NotArmed));
        assert_eq!(machine.next_tick(), None);

        machine.hold_key_changed(clock.at(2000), WITH_OTHERS);
        assert_eq!(machine.reset(), None);
        assert_eq!(machine.reset(), None);
    }

    fn hands_free_machine(clock: &Clock) -> HoldMachine {
        let mut machine = HoldMachine::new();
        assert_eq!(
            machine.hands_free_pressed(clock.at(0)),
            Some(HoldEvent::HandsFreeStarted { at: clock.at(0) })
        );
        machine.hands_free_released();
        assert!(machine.is_hands_free());
        machine
    }

    #[test]
    fn the_chord_starts_a_hands_free_take_and_the_next_press_stops_it() {
        let clock = Clock::new();
        let mut machine = hands_free_machine(&clock);
        assert!(!machine.is_armed());
        assert_eq!(machine.next_tick(), None, "no key is held to watch");

        assert_eq!(
            machine.hands_free_pressed(clock.at(30_000)),
            Some(HoldEvent::HandsFreeStopped)
        );
        assert!(!machine.is_hands_free());
        machine.hands_free_released();
        assert_eq!(
            machine.hands_free_pressed(clock.at(31_000)),
            Some(HoldEvent::HandsFreeStarted {
                at: clock.at(31_000)
            })
        );
    }

    #[test]
    fn the_chord_s_key_repeating_while_held_is_not_another_press() {
        let clock = Clock::new();
        let mut machine = HoldMachine::new();
        machine.hands_free_pressed(clock.at(0));
        for ms in [500, 530, 560] {
            assert_eq!(machine.hands_free_pressed(clock.at(ms)), None);
        }
        assert!(machine.is_hands_free());
        machine.hands_free_released();
        assert_eq!(
            machine.hands_free_pressed(clock.at(2000)),
            Some(HoldEvent::HandsFreeStopped)
        );
    }

    #[test]
    fn a_hands_free_take_ignores_the_hold_key_and_other_keys() {
        let clock = Clock::new();
        let mut machine = hands_free_machine(&clock);
        // Right ⌘C, typing, and right ⌘ held long enough to arm.
        assert_eq!(machine.hold_key_changed(clock.at(1000), ALONE), None);
        assert_eq!(machine.other_key_pressed(clock.at(1050)), None);
        assert_eq!(run_until(&mut machine, clock.at(5000), ALONE), []);
        assert_eq!(machine.tick(clock.at(5000), ALONE), None);
        assert_eq!(machine.hold_key_changed(clock.at(5100), UP), None);
        assert!(machine.is_hands_free());
        assert!(!machine.is_armed());
    }

    #[test]
    fn the_chord_pressed_with_the_hold_key_starts_hands_free_after_the_hold_is_spoiled() {
        // Right ⌘, then ⌃, then X: the press is not armed, then hands-free starts.
        let clock = Clock::new();
        let mut machine = HoldMachine::new();
        machine.hold_key_changed(clock.at(0), ALONE);
        assert_eq!(
            machine.other_key_pressed(clock.at(40)),
            Some(HoldEvent::NotArmed)
        );
        assert_eq!(
            machine.hands_free_pressed(clock.at(80)),
            Some(HoldEvent::HandsFreeStarted { at: clock.at(80) })
        );
        // The hold key coming up afterwards does not end it.
        assert_eq!(machine.hold_key_changed(clock.at(300), UP), None);
        assert!(machine.is_hands_free());

        // Stopped the same way, the hold key still down: it starts nothing on its way up.
        machine.hands_free_released();
        machine.hold_key_changed(clock.at(5000), ALONE);
        assert_eq!(
            machine.hands_free_pressed(clock.at(5080)),
            Some(HoldEvent::HandsFreeStopped)
        );
        assert_eq!(machine.hold_key_changed(clock.at(5200), UP), None);
        assert_eq!(machine.next_tick(), None);
    }

    #[test]
    fn the_chord_spoils_a_hold_whose_other_keys_were_missed() {
        let clock = Clock::new();
        let mut machine = HoldMachine::new();
        machine.hold_key_changed(clock.at(0), ALONE);
        assert_eq!(
            machine.hands_free_pressed(clock.at(50)),
            Some(HoldEvent::NotArmed)
        );
        assert!(!machine.is_hands_free());

        let mut machine = armed_machine(&clock);
        assert_eq!(
            machine.hands_free_pressed(clock.at(900)),
            Some(HoldEvent::Cancelled)
        );
        assert!(!machine.is_hands_free());
    }

    #[test]
    fn a_hands_free_take_ended_by_the_app_lets_the_next_press_start_one() {
        let clock = Clock::new();
        let mut machine = hands_free_machine(&clock);
        // The cap, at 120 s.
        machine.hands_free_ended();
        assert!(!machine.is_hands_free());
        assert_eq!(
            machine.hands_free_pressed(clock.at(121_000)),
            Some(HoldEvent::HandsFreeStarted {
                at: clock.at(121_000)
            })
        );
        // Told while no hands-free take runs, it changes nothing.
        let mut machine = armed_machine(&clock);
        machine.hands_free_ended();
        assert!(machine.is_armed());
    }

    #[test]
    fn reset_cancels_a_hands_free_take() {
        let clock = Clock::new();
        let mut machine = hands_free_machine(&clock);
        assert_eq!(machine.reset(), Some(HoldEvent::Cancelled));
        assert!(!machine.is_hands_free());
    }

    /// Where a caller stands after the events so far: whether a microphone opened at the press is still
    /// open, and whether a take is running.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Expect {
        Nothing,
        ArmedOrNotArmed,
        TakeEnd,
        HandsFreeEnd,
    }

    fn follow(expect: Expect, event: HoldEvent) -> Expect {
        match (expect, event) {
            (Expect::Nothing, HoldEvent::Pressed { .. }) => Expect::ArmedOrNotArmed,
            (Expect::ArmedOrNotArmed, HoldEvent::Armed { .. }) => Expect::TakeEnd,
            (Expect::ArmedOrNotArmed, HoldEvent::NotArmed) => Expect::Nothing,
            (Expect::Nothing, HoldEvent::HandsFreeStarted { .. }) => Expect::HandsFreeEnd,
            (Expect::HandsFreeEnd, HoldEvent::HandsFreeStopped | HoldEvent::Cancelled) => {
                Expect::Nothing
            }
            (
                Expect::TakeEnd,
                HoldEvent::Released | HoldEvent::ReleaseMissed | HoldEvent::Cancelled,
            ) => Expect::Nothing,
            (expect, event) => panic!("{event:?} while expecting {expect:?}"),
        }
    }

    #[test]
    fn every_press_ends_in_armed_or_not_armed_and_every_take_ends_once() {
        // Hands-free takes and the chord's key join the hold key's events; a take the app ends itself
        // ends without an event.
        // A small linear congruential generator, so the sequences are the same on every run.
        let mut seed: u64 = 0x5eed;
        let mut next = |below: u64| {
            seed = seed
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            (seed >> 33) % below
        };
        let samples = [ALONE, WITH_OTHERS, UP];
        for _ in 0..200 {
            let clock = Clock::new();
            let mut machine = HoldMachine::new();
            let mut expect = Expect::Nothing;
            let mut ms = 0;
            for _ in 0..60 {
                ms += next(400);
                let now = clock.at(ms);
                let sample = samples[next(3) as usize];
                let event = match next(7) {
                    0 => machine.hold_key_changed(now, sample),
                    1 => machine.other_key_pressed(now),
                    2 => machine.tick(now, sample),
                    3 => machine.hands_free_pressed(now),
                    4 => {
                        machine.hands_free_released();
                        None
                    }
                    5 => {
                        if next(4) == 0 {
                            if expect == Expect::HandsFreeEnd {
                                expect = Expect::Nothing;
                            }
                            machine.hands_free_ended();
                        }
                        None
                    }
                    _ => (next(10) == 0).then(|| machine.reset()).flatten(),
                };
                if let Some(event) = event {
                    expect = follow(expect, event);
                }
                assert_eq!(
                    machine.is_armed(),
                    expect == Expect::TakeEnd,
                    "{machine:?} at {ms} ms"
                );
                assert_eq!(
                    machine.is_hands_free(),
                    expect == Expect::HandsFreeEnd,
                    "{machine:?} at {ms} ms"
                );
            }
            if let Some(event) = machine.reset() {
                expect = follow(expect, event);
            }
            assert_eq!(expect, Expect::Nothing);
        }
    }
}
