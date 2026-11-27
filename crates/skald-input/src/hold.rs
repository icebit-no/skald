//! Hold-to-talk as pure logic over timestamped key changes, so every rule is tested with synthetic events
//! and a fake clock.
//!
//! A take arms once the hold key has been down alone for [`ARM_AFTER`]. Any other key pressed while the
//! hold key is down spoils the hold: before arming nothing starts, after arming the take is cancelled, and
//! either way nothing more happens until the hold key comes up. While a take runs, the key's real state is
//! checked every [`WATCHDOG_EVERY`], which ends a take whose release was never reported.

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
    /// The take starts. Speech may have begun at `pressed_at`, before arming.
    Armed { pressed_at: Instant },
    /// The hold key came up: the take ends and is transcribed.
    Released,
    /// The hold key was found up although its release was never reported: the take ends and is
    /// transcribed, as for [`HoldEvent::Released`].
    ReleaseMissed,
    /// Another key joined the hold key: the take is thrown away.
    Cancelled,
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
}

#[derive(Debug, Clone)]
pub struct HoldMachine {
    state: State,
}

impl Default for HoldMachine {
    fn default() -> Self {
        Self::new()
    }
}

impl HoldMachine {
    pub fn new() -> Self {
        Self { state: State::Idle }
    }

    /// Whether a take is running.
    pub fn is_armed(&self) -> bool {
        matches!(self.state, State::Armed { .. })
    }

    /// The hold key went down or up; `sample` says which.
    pub fn hold_key_changed(&mut self, now: Instant, sample: KeySample) -> Option<HoldEvent> {
        if !sample.hold_key_down {
            let was_armed = self.is_armed();
            self.state = State::Idle;
            return was_armed.then_some(HoldEvent::Released);
        }
        if self.state == State::Idle {
            self.state = if sample.other_modifiers_down {
                State::WaitingForRelease { checked: now }
            } else {
                State::Pressed { since: now }
            };
        }
        None
    }

    /// Any key other than the hold key went down: a key, a modifier or a mouse button.
    pub fn other_key_pressed(&mut self, now: Instant) -> Option<HoldEvent> {
        match self.state {
            State::Idle | State::WaitingForRelease { .. } => None,
            State::Pressed { .. } => {
                self.state = State::WaitingForRelease { checked: now };
                None
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
            State::Idle => None,
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
            State::Idle => None,
            State::Pressed { since } => {
                if !sample.hold_key_down {
                    self.state = State::Idle;
                    None
                } else if sample.other_modifiers_down {
                    self.state = State::WaitingForRelease { checked: now };
                    None
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

    /// Forgets the hold, for when key events can no longer be trusted to arrive. A running take is
    /// cancelled.
    pub fn reset(&mut self) -> Option<HoldEvent> {
        let was_armed = self.is_armed();
        self.state = State::Idle;
        was_armed.then_some(HoldEvent::Cancelled)
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
        machine.hold_key_changed(clock.at(0), ALONE);
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

        assert_eq!(machine.hold_key_changed(clock.at(0), ALONE), None);
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
    fn a_tap_shorter_than_200_ms_starts_nothing() {
        let clock = Clock::new();
        let mut machine = HoldMachine::new();
        machine.hold_key_changed(clock.at(0), ALONE);

        assert_eq!(machine.hold_key_changed(clock.at(120), UP), None);
        assert_eq!(machine.next_tick(), None);
        assert_eq!(run_until(&mut machine, clock.at(1000), UP), []);
    }

    #[test]
    fn a_shortcut_typed_before_arming_starts_nothing() {
        // ⌘C: the hold key, then C within 200 ms.
        let clock = Clock::new();
        let mut machine = HoldMachine::new();
        machine.hold_key_changed(clock.at(0), ALONE);

        assert_eq!(machine.other_key_pressed(clock.at(80)), None);
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
    fn a_modifier_already_down_when_the_key_is_pressed_starts_nothing() {
        // ⇧ held, then the hold key: a chord, not dictation.
        let clock = Clock::new();
        let mut machine = HoldMachine::new();

        assert_eq!(machine.hold_key_changed(clock.at(0), WITH_OTHERS), None);
        assert_eq!(run_until(&mut machine, clock.at(2000), ALONE), []);
        assert!(!machine.is_armed());
    }

    #[test]
    fn a_modifier_found_down_at_arming_time_starts_nothing() {
        let clock = Clock::new();
        let mut machine = HoldMachine::new();
        machine.hold_key_changed(clock.at(0), ALONE);

        assert_eq!(run_until(&mut machine, clock.at(200), WITH_OTHERS), []);
        assert!(!machine.is_armed());
        assert_eq!(run_until(&mut machine, clock.at(2000), ALONE), []);
    }

    #[test]
    fn a_key_found_up_at_arming_time_starts_nothing() {
        let clock = Clock::new();
        let mut machine = HoldMachine::new();
        machine.hold_key_changed(clock.at(0), ALONE);

        assert_eq!(run_until(&mut machine, clock.at(200), UP), []);
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
    fn a_repeated_down_report_while_armed_changes_nothing() {
        let clock = Clock::new();
        let mut machine = armed_machine(&clock);

        assert_eq!(machine.hold_key_changed(clock.at(300), ALONE), None);
        assert!(machine.is_armed());
    }

    #[test]
    fn reset_cancels_a_running_take_and_only_that() {
        let clock = Clock::new();
        let mut machine = armed_machine(&clock);
        assert_eq!(machine.reset(), Some(HoldEvent::Cancelled));
        assert_eq!(machine.next_tick(), None);

        machine.hold_key_changed(clock.at(1000), ALONE);
        assert_eq!(machine.reset(), None);
        assert_eq!(machine.next_tick(), None);
    }
}
