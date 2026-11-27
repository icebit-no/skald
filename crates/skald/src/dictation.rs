//! The rules between the hold key and the microphone: which microphone a take uses, when it opens and
//! closes, and when a recording goes on to be transcribed. The keys arrive as events and the microphones
//! behind a trait, so every rule is tested without either.
//!
//! The microphone opens at the press, before the take arms, so the first words are kept; a press that does
//! not arm closes it again and throws the audio away. A take ends at the release (or when the watchdog
//! finds the key up), at the 120-second cap, or when its microphone disappears; another key pressed while
//! it runs cancels it.

use std::mem;
use std::sync::Arc;
use std::sync::mpsc::Receiver;
use std::time::{Instant, SystemTime};

use skald_audio::{
    CaptureError, MicId, Microphone, MicrophoneAccess, MicrophoneChange, MicrophoneWatcher,
    Recording, Take, TakeEvent,
};
use skald_core::Config;
use skald_input::HoldEvent;

use crate::pipeline::Job;
use crate::say::{Exit, Line};

pub(crate) trait Microphones {
    type Take: RunningTake;

    fn access(&self) -> MicrophoneAccess;
    /// The microphone a take started now would use.
    fn active(&self) -> Option<Microphone>;
    fn set_order(&mut self, order: Vec<Microphone>);
    /// Each change of the active microphone since last asked.
    fn changes(&mut self) -> Vec<MicrophoneChange>;
    fn start(&mut self, microphone: &Microphone) -> Result<Self::Take, CaptureError>;
}

/// A take while it records. Dropping it throws its audio away.
pub(crate) trait RunningTake {
    fn microphone(&self) -> &Microphone;
    /// What the take reported since last asked.
    fn events(&mut self) -> Vec<TakeEvent>;
    fn stop(self) -> Recording;
}

/// The order a take's microphone is chosen by: the pinned microphone, then the user's order (D-20). The
/// configuration keeps microphones by name, so every entry matches by name.
pub(crate) fn microphone_order(config: &Config) -> Vec<Microphone> {
    let pinned = config
        .pinned_microphone
        .as_deref()
        .filter(|name| !name.trim().is_empty());
    let ordered = config
        .microphones
        .iter()
        .map(String::as_str)
        .filter(|name| Some(*name) != pinned);
    pinned
        .into_iter()
        .chain(ordered)
        .map(|name| Microphone {
            id: MicId::new(""),
            name: name.to_owned(),
        })
        .collect()
}

enum Hold<T> {
    Idle,
    /// Down and not yet armed: the take opened at the press, or why there is none.
    Pressed(Result<T, Unopened>),
    Recording(T),
}

enum Unopened {
    NoMicrophone,
    Failed {
        microphone: String,
        error: CaptureError,
    },
}

pub(crate) struct Dictation<M: Microphones> {
    microphones: M,
    hold: Hold<M::Take>,
    config: Arc<Config>,
    order: Vec<Microphone>,
    lines: Vec<Line>,
    jobs: Vec<Job>,
}

impl<M: Microphones> Dictation<M> {
    /// `microphones` already follows the order the configuration gives.
    pub(crate) fn new(microphones: M, config: Arc<Config>) -> Self {
        Self {
            microphones,
            hold: Hold::Idle,
            order: microphone_order(&config),
            config,
            lines: Vec::new(),
            jobs: Vec::new(),
        }
    }

    pub(crate) fn config(&self) -> &Config {
        &self.config
    }

    /// No take is opening or running, so the hold key may be listened for anew.
    pub(crate) fn is_idle(&self) -> bool {
        matches!(self.hold, Hold::Idle)
    }

    /// Takes the settings for the takes that end from now on, and moves to another microphone at once if
    /// the order changed.
    pub(crate) fn set_config(&mut self, config: Arc<Config>) {
        let order = microphone_order(&config);
        if order != self.order {
            self.microphones.set_order(order.clone());
            self.order = order;
        }
        self.config = config;
    }

    pub(crate) fn on_hold(&mut self, event: HoldEvent) -> Result<(), Exit> {
        match event {
            HoldEvent::Pressed { .. } => {
                // Whatever an earlier hold left open closes before the next take opens.
                self.hold = Hold::Idle;
                self.hold = Hold::Pressed(self.open()?);
            }
            HoldEvent::NotArmed => self.hold = Hold::Idle,
            HoldEvent::Armed { .. } => {
                let opened = match mem::replace(&mut self.hold, Hold::Idle) {
                    Hold::Pressed(opened) => opened,
                    Hold::Recording(take) => Ok(take),
                    // A press this loop never saw, as one made while it started.
                    Hold::Idle => self.open()?,
                };
                match opened {
                    Ok(take) => {
                        self.lines
                            .push(Line::Recording(take.microphone().name.clone()));
                        self.hold = Hold::Recording(take);
                    }
                    Err(Unopened::NoMicrophone) => self.lines.push(Line::NoMicrophone),
                    Err(Unopened::Failed { microphone, error }) => {
                        self.lines.push(Line::CaptureFailed { microphone, error });
                    }
                }
            }
            HoldEvent::Released | HoldEvent::ReleaseMissed => {
                if let Hold::Recording(take) = mem::replace(&mut self.hold, Hold::Idle) {
                    self.finish(take);
                }
            }
            HoldEvent::Cancelled => {
                if let Hold::Recording(_) = mem::replace(&mut self.hold, Hold::Idle) {
                    self.lines.push(Line::Cancelled);
                }
            }
        }
        Ok(())
    }

    /// Ends a take at the cap or when its microphone disappears, and reports microphone changes.
    pub(crate) fn poll(&mut self) {
        if let Hold::Recording(take) = &mut self.hold {
            let ended = take.events().into_iter().find_map(|event| match event {
                TakeEvent::Level(_) => None,
                TakeEvent::Capped => Some(Line::Capped),
                TakeEvent::MicrophoneLost => {
                    Some(Line::MicrophoneLost(take.microphone().name.clone()))
                }
            });
            if let Some(line) = ended {
                self.lines.push(line);
                if let Hold::Recording(take) = mem::replace(&mut self.hold, Hold::Idle) {
                    self.finish(take);
                }
            }
        }
        for change in self.microphones.changes() {
            self.lines
                .push(Line::MicrophoneChanged(change.to.map(|m| m.name)));
        }
    }

    pub(crate) fn take_lines(&mut self) -> Vec<Line> {
        mem::take(&mut self.lines)
    }

    pub(crate) fn take_jobs(&mut self) -> Vec<Job> {
        mem::take(&mut self.jobs)
    }

    fn open(&mut self) -> Result<Result<M::Take, Unopened>, Exit> {
        match self.microphones.access() {
            MicrophoneAccess::Denied => return Err(Exit::MicrophoneDenied),
            MicrophoneAccess::Restricted => return Err(Exit::MicrophoneRestricted),
            MicrophoneAccess::Granted | MicrophoneAccess::NotAsked => {}
        }
        let Some(microphone) = self.microphones.active() else {
            return Ok(Err(Unopened::NoMicrophone));
        };
        match self.microphones.start(&microphone) {
            Ok(take) => Ok(Ok(take)),
            Err(CaptureError::PermissionDenied) => Err(Exit::MicrophoneDenied),
            Err(error) => Ok(Err(Unopened::Failed {
                microphone: microphone.name,
                error,
            })),
        }
    }

    fn finish(&mut self, take: M::Take) {
        let recording = take.stop();
        self.jobs.push(Job {
            recording,
            config: Arc::clone(&self.config),
            ended_at: Instant::now(),
            unix_ms: unix_ms(SystemTime::now()),
        });
        self.lines.push(Line::Carving);
    }
}

fn unix_ms(time: SystemTime) -> i64 {
    time.duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |since| {
            i64::try_from(since.as_millis()).unwrap_or(i64::MAX)
        })
}

/// The system's microphones, followed as they come and go.
pub(crate) struct SystemMicrophones {
    watcher: MicrophoneWatcher,
    changes: Receiver<MicrophoneChange>,
}

impl SystemMicrophones {
    pub(crate) fn start(order: Vec<Microphone>) -> Result<Self, CaptureError> {
        let (watcher, changes) = MicrophoneWatcher::start(order)?;
        Ok(Self { watcher, changes })
    }
}

impl Microphones for SystemMicrophones {
    type Take = Take;

    fn access(&self) -> MicrophoneAccess {
        skald_audio::microphone_access()
    }

    fn active(&self) -> Option<Microphone> {
        self.watcher.active()
    }

    fn set_order(&mut self, order: Vec<Microphone>) {
        self.watcher.set_order(order);
    }

    fn changes(&mut self) -> Vec<MicrophoneChange> {
        self.changes.try_iter().collect()
    }

    fn start(&mut self, microphone: &Microphone) -> Result<Take, CaptureError> {
        Take::start(microphone)
    }
}

impl RunningTake for Take {
    fn microphone(&self) -> &Microphone {
        Take::microphone(self)
    }

    fn events(&mut self) -> Vec<TakeEvent> {
        Take::events(self).try_iter().collect()
    }

    fn stop(self) -> Recording {
        Take::stop(self)
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use skald_audio::Ending;

    use super::*;

    fn mic(name: &str) -> Microphone {
        Microphone {
            id: MicId::new(format!("id-{name}")),
            name: name.to_owned(),
        }
    }

    #[derive(Default)]
    struct Log {
        opened: Vec<String>,
        stopped: usize,
        thrown_away: usize,
        orders: Vec<Vec<String>>,
    }

    struct FakeTake {
        microphone: Microphone,
        events: Rc<RefCell<Vec<TakeEvent>>>,
        log: Rc<RefCell<Log>>,
        stopped: bool,
    }

    impl RunningTake for FakeTake {
        fn microphone(&self) -> &Microphone {
            &self.microphone
        }

        fn events(&mut self) -> Vec<TakeEvent> {
            mem::take(&mut self.events.borrow_mut())
        }

        fn stop(mut self) -> Recording {
            self.stopped = true;
            self.log.borrow_mut().stopped += 1;
            Recording {
                samples: vec![0.1; 1600],
                levels: vec![-20.0],
                ending: Ending::Stopped,
                microphone: self.microphone.clone(),
            }
        }
    }

    impl Drop for FakeTake {
        fn drop(&mut self) {
            if !self.stopped {
                self.log.borrow_mut().thrown_away += 1;
            }
        }
    }

    struct FakeMicrophones {
        access: MicrophoneAccess,
        active: Option<Microphone>,
        fails_with: Option<CaptureError>,
        changes: Vec<MicrophoneChange>,
        events: Rc<RefCell<Vec<TakeEvent>>>,
        log: Rc<RefCell<Log>>,
    }

    impl Microphones for FakeMicrophones {
        type Take = FakeTake;

        fn access(&self) -> MicrophoneAccess {
            self.access
        }

        fn active(&self) -> Option<Microphone> {
            self.active.clone()
        }

        fn set_order(&mut self, order: Vec<Microphone>) {
            let names = order.into_iter().map(|m| m.name).collect();
            self.log.borrow_mut().orders.push(names);
        }

        fn changes(&mut self) -> Vec<MicrophoneChange> {
            mem::take(&mut self.changes)
        }

        fn start(&mut self, microphone: &Microphone) -> Result<FakeTake, CaptureError> {
            if let Some(error) = self.fails_with.clone() {
                return Err(error);
            }
            self.log.borrow_mut().opened.push(microphone.name.clone());
            Ok(FakeTake {
                microphone: microphone.clone(),
                events: Rc::clone(&self.events),
                log: Rc::clone(&self.log),
                stopped: false,
            })
        }
    }

    struct Setup {
        dictation: Dictation<FakeMicrophones>,
        events: Rc<RefCell<Vec<TakeEvent>>>,
        log: Rc<RefCell<Log>>,
    }

    fn setup() -> Setup {
        let events = Rc::new(RefCell::new(Vec::new()));
        let log = Rc::new(RefCell::new(Log::default()));
        let microphones = FakeMicrophones {
            access: MicrophoneAccess::Granted,
            active: Some(mic("Shure MV7")),
            fails_with: None,
            changes: Vec::new(),
            events: Rc::clone(&events),
            log: Rc::clone(&log),
        };
        Setup {
            dictation: Dictation::new(microphones, Arc::new(Config::default())),
            events,
            log,
        }
    }

    fn pressed() -> HoldEvent {
        HoldEvent::Pressed { at: Instant::now() }
    }

    fn armed() -> HoldEvent {
        HoldEvent::Armed {
            pressed_at: Instant::now(),
        }
    }

    fn hold(dictation: &mut Dictation<FakeMicrophones>, events: &[HoldEvent]) {
        for &event in events {
            dictation.on_hold(event).unwrap();
        }
    }

    #[test]
    fn the_microphone_opens_at_the_press_and_a_release_hands_the_take_over() {
        let Setup {
            mut dictation, log, ..
        } = setup();
        hold(&mut dictation, &[pressed()]);
        assert_eq!(log.borrow().opened, ["Shure MV7"]);
        assert!(
            dictation.take_lines().is_empty(),
            "nothing shows before arming"
        );

        hold(&mut dictation, &[armed()]);
        assert_eq!(
            dictation.take_lines(),
            [Line::Recording("Shure MV7".into())]
        );
        assert!(!dictation.is_idle());

        hold(&mut dictation, &[HoldEvent::Released]);
        assert_eq!(dictation.take_lines(), [Line::Carving]);
        let jobs = dictation.take_jobs();
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].recording.microphone.name, "Shure MV7");
        assert_eq!(log.borrow().opened.len(), 1, "opened once");
        assert_eq!(log.borrow().stopped, 1);
        assert!(dictation.is_idle());
    }

    #[test]
    fn a_missed_release_ends_the_take_like_a_release() {
        let Setup { mut dictation, .. } = setup();
        hold(
            &mut dictation,
            &[pressed(), armed(), HoldEvent::ReleaseMissed],
        );
        assert_eq!(dictation.take_jobs().len(), 1);
    }

    #[test]
    fn a_press_that_does_not_arm_throws_the_audio_away_quietly() {
        let Setup {
            mut dictation, log, ..
        } = setup();
        hold(&mut dictation, &[pressed(), HoldEvent::NotArmed]);
        assert_eq!(log.borrow().thrown_away, 1);
        assert!(dictation.take_lines().is_empty());
        assert!(dictation.take_jobs().is_empty());
        assert!(dictation.is_idle());
    }

    #[test]
    fn another_key_cancels_a_running_take() {
        let Setup {
            mut dictation, log, ..
        } = setup();
        hold(&mut dictation, &[pressed(), armed(), HoldEvent::Cancelled]);
        assert_eq!(
            dictation.take_lines(),
            [Line::Recording("Shure MV7".into()), Line::Cancelled]
        );
        assert!(dictation.take_jobs().is_empty());
        assert_eq!(log.borrow().thrown_away, 1);
    }

    #[test]
    fn the_cap_ends_the_take_and_the_late_release_changes_nothing() {
        let Setup {
            mut dictation,
            events,
            ..
        } = setup();
        hold(&mut dictation, &[pressed(), armed()]);
        events
            .borrow_mut()
            .extend([TakeEvent::Level(-20.0), TakeEvent::Capped]);
        dictation.poll();
        assert_eq!(
            dictation.take_lines(),
            [
                Line::Recording("Shure MV7".into()),
                Line::Capped,
                Line::Carving
            ]
        );
        assert_eq!(dictation.take_jobs().len(), 1);

        hold(&mut dictation, &[HoldEvent::Released]);
        assert!(dictation.take_lines().is_empty());
        assert!(dictation.take_jobs().is_empty());
    }

    #[test]
    fn a_microphone_lost_mid_take_ends_it_with_what_was_recorded() {
        let Setup {
            mut dictation,
            events,
            ..
        } = setup();
        hold(&mut dictation, &[pressed(), armed()]);
        dictation.take_lines();
        events.borrow_mut().push(TakeEvent::MicrophoneLost);
        dictation.microphones.changes.push(MicrophoneChange {
            from: Some(mic("Shure MV7")),
            to: Some(mic("MacBook Pro-mikrofon")),
        });
        dictation.poll();
        assert_eq!(
            dictation.take_lines(),
            [
                Line::MicrophoneLost("Shure MV7".into()),
                Line::Carving,
                Line::MicrophoneChanged(Some("MacBook Pro-mikrofon".into())),
            ]
        );
        assert_eq!(dictation.take_jobs().len(), 1);
        hold(&mut dictation, &[HoldEvent::Cancelled]);
        assert!(dictation.take_lines().is_empty(), "the take had ended");
    }

    #[test]
    fn levels_alone_keep_the_take_running() {
        let Setup {
            mut dictation,
            events,
            ..
        } = setup();
        hold(&mut dictation, &[pressed(), armed()]);
        events.borrow_mut().push(TakeEvent::Level(-30.0));
        dictation.poll();
        assert!(!dictation.is_idle());
        assert!(dictation.take_jobs().is_empty());
    }

    #[test]
    fn without_a_microphone_the_take_says_so_once_armed() {
        let Setup {
            mut dictation, log, ..
        } = setup();
        dictation.microphones.active = None;
        hold(&mut dictation, &[pressed()]);
        assert!(dictation.take_lines().is_empty(), "a shortcut says nothing");
        hold(&mut dictation, &[armed(), HoldEvent::Released]);
        assert_eq!(dictation.take_lines(), [Line::NoMicrophone]);
        assert!(dictation.take_jobs().is_empty());
        assert!(log.borrow().opened.is_empty());
    }

    #[test]
    fn a_microphone_that_will_not_open_is_named_with_the_reason() {
        let Setup { mut dictation, .. } = setup();
        dictation.microphones.fails_with = Some(CaptureError::Busy);
        hold(&mut dictation, &[pressed(), armed(), HoldEvent::Released]);
        assert_eq!(
            dictation.take_lines(),
            [Line::CaptureFailed {
                microphone: "Shure MV7".into(),
                error: CaptureError::Busy,
            }]
        );
        assert!(dictation.take_jobs().is_empty());
    }

    #[test]
    fn missing_microphone_permission_ends_skald() {
        for (access, fails_with, exit) in [
            (MicrophoneAccess::Denied, None, Exit::MicrophoneDenied),
            (
                MicrophoneAccess::Restricted,
                None,
                Exit::MicrophoneRestricted,
            ),
            (
                MicrophoneAccess::Granted,
                Some(CaptureError::PermissionDenied),
                Exit::MicrophoneDenied,
            ),
        ] {
            let Setup { mut dictation, .. } = setup();
            dictation.microphones.access = access;
            dictation.microphones.fails_with = fails_with;
            assert_eq!(dictation.on_hold(pressed()).unwrap_err(), exit);
        }
    }

    #[test]
    fn a_microphone_not_yet_asked_for_still_opens() {
        let Setup {
            mut dictation, log, ..
        } = setup();
        dictation.microphones.access = MicrophoneAccess::NotAsked;
        hold(&mut dictation, &[pressed()]);
        assert_eq!(log.borrow().opened.len(), 1);
    }

    #[test]
    fn an_arm_without_a_press_opens_the_microphone_then() {
        let Setup {
            mut dictation, log, ..
        } = setup();
        hold(&mut dictation, &[armed(), HoldEvent::Released]);
        assert_eq!(log.borrow().opened.len(), 1);
        assert_eq!(dictation.take_jobs().len(), 1);
    }

    #[test]
    fn a_take_carries_the_settings_from_when_it_ended() {
        let Setup { mut dictation, .. } = setup();
        hold(&mut dictation, &[pressed(), armed()]);
        let mut config = Config::default();
        config.copy_to_clipboard = true;
        dictation.set_config(Arc::new(config));
        hold(&mut dictation, &[HoldEvent::Released]);
        assert!(dictation.take_jobs()[0].config.copy_to_clipboard);
    }

    #[test]
    fn microphone_changes_are_reported_in_design_md_s_words() {
        let Setup { mut dictation, .. } = setup();
        dictation.microphones.changes = vec![
            MicrophoneChange {
                from: Some(mic("MacBook Pro-mikrofon")),
                to: Some(mic("Yeti")),
            },
            MicrophoneChange {
                from: Some(mic("Yeti")),
                to: None,
            },
        ];
        dictation.poll();
        assert_eq!(
            dictation.take_lines(),
            [
                Line::MicrophoneChanged(Some("Yeti".into())),
                Line::MicrophoneChanged(None),
            ]
        );
    }

    #[test]
    fn the_pin_goes_first_and_a_new_order_reaches_the_microphones() {
        let Setup {
            mut dictation, log, ..
        } = setup();
        let mut config = Config::default();
        config.microphones = vec!["Yeti".into(), "MacBook Pro-mikrofon".into()];
        dictation.set_config(Arc::new(config.clone()));
        dictation.set_config(Arc::new(config.clone()));
        config.pinned_microphone = Some("MacBook Pro-mikrofon".into());
        dictation.set_config(Arc::new(config));
        assert_eq!(
            log.borrow().orders,
            [
                vec!["Yeti".to_owned(), "MacBook Pro-mikrofon".into()],
                vec!["MacBook Pro-mikrofon".to_owned(), "Yeti".into()],
            ],
            "an unchanged order is not sent again"
        );
    }

    #[test]
    fn the_order_matches_by_name_with_the_pin_ahead() {
        let mut config = Config::default();
        config.microphones = vec!["Yeti".into(), "AirPods".into()];
        config.pinned_microphone = Some("AirPods".into());
        let order = microphone_order(&config);
        let names: Vec<&str> = order.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(names, ["AirPods", "Yeti"]);

        // Matched by name, a microphone whose id the system changed is still found.
        let connected = skald_audio::Connected {
            microphones: vec![mic("MacBook Pro-mikrofon"), mic("Yeti")],
            default: Some(MicId::new("id-MacBook Pro-mikrofon")),
        };
        assert_eq!(
            skald_audio::choose(&order, &connected).map(|m| m.name.as_str()),
            Some("Yeti")
        );

        config.pinned_microphone = Some("  ".into());
        assert_eq!(microphone_order(&config).len(), 2);
    }
}
