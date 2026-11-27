//! Listens for the hold key on a thread of its own and reports what each hold means for the take.

use std::fmt;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};

use handy_keys::{KeyEvent, KeyboardListener};

use crate::hold::{HoldEvent, HoldMachine};
use crate::hotkey::{HoldKey, KeyChange, classify};
use crate::macos::access::{AccessWatch, accessibility_granted};
use crate::macos::keyboard::sample;

/// How often the thread wakes when no key arrives: the longest a revoked permission keeps the event tap in
/// place, and the longest the thread outlives its listener.
const WAKE_EVERY: Duration = Duration::from_millis(500);

/// A permission change is checked again this much later, in case the window server had not taken it in at
/// the first check.
const RECHECK_AFTER: Duration = Duration::from_millis(500);

/// How often starting is tried again while the listener cannot run.
const RETRY_EVERY: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ListenerEvent {
    /// The hold key is listened for.
    Ready,
    /// The hold key is not listened for, and why. The listener keeps trying and reports
    /// [`ListenerEvent::Ready`] once it can listen again.
    Unavailable(Unavailable),
    Hold(HoldEvent),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unavailable {
    /// macOS Accessibility is not granted, or was revoked.
    NoAccessibility,
    /// The key listener could not start or stopped, for the reason given.
    ListenerFailed(String),
}

impl fmt::Display for Unavailable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Unavailable::NoAccessibility => f.write_str("Accessibility is not granted"),
            Unavailable::ListenerFailed(reason) => write!(f, "the key listener failed: {reason}"),
        }
    }
}

/// Listens for the hold key until dropped.
#[derive(Debug)]
pub struct HoldListener {
    events: Receiver<ListenerEvent>,
    /// Dropping it tells the thread to stop.
    _stop: Sender<()>,
}

impl HoldListener {
    pub fn start(hold_key: HoldKey) -> std::io::Result<Self> {
        let (events_tx, events) = mpsc::channel();
        let (stop, stop_rx) = mpsc::channel();
        thread::Builder::new()
            .name("skald-hold-key".into())
            .spawn(move || Driver::new(hold_key, events_tx, stop_rx).run())?;
        Ok(Self {
            events,
            _stop: stop,
        })
    }

    pub fn events(&self) -> &Receiver<ListenerEvent> {
        &self.events
    }
}

struct Driver {
    hold_key: HoldKey,
    events: Sender<ListenerEvent>,
    stop: Receiver<()>,
    machine: HoldMachine,
    keyboard: Option<KeyboardListener>,
    access: AccessWatch,
    /// Pending permission checks, earliest first.
    access_checks: Vec<Instant>,
    retry_at: Instant,
    /// The unavailability last reported, so each is reported once.
    reported: Option<Unavailable>,
}

impl Driver {
    fn new(hold_key: HoldKey, events: Sender<ListenerEvent>, stop: Receiver<()>) -> Self {
        Self {
            hold_key,
            events,
            stop,
            machine: HoldMachine::new(),
            keyboard: None,
            access: AccessWatch::new(),
            access_checks: Vec::new(),
            retry_at: Instant::now(),
            reported: None,
        }
    }

    fn run(mut self) {
        loop {
            if matches!(self.stop.try_recv(), Err(TryRecvError::Disconnected)) {
                return;
            }
            let now = Instant::now();
            if self.access.changed() {
                self.access_checks = vec![now, now + RECHECK_AFTER];
            }
            let check_due = self.take_due_checks(now);
            if self.keyboard.is_some() {
                if check_due && !accessibility_granted() {
                    self.lose(Unavailable::NoAccessibility);
                }
            } else if check_due || now >= self.retry_at {
                self.try_start();
            }

            let timeout = self.next_wake(now).saturating_duration_since(now);
            match self
                .keyboard
                .as_ref()
                .map(|keyboard| keyboard.recv_timeout(timeout))
            {
                Some(Ok(event)) => self.on_key_event(&event),
                Some(Err(handy_keys::Error::Timeout)) => {}
                Some(Err(error)) => {
                    let reason = if accessibility_granted() {
                        Unavailable::ListenerFailed(error.to_string())
                    } else {
                        Unavailable::NoAccessibility
                    };
                    self.lose(reason);
                }
                None => {
                    if let Err(RecvTimeoutError::Disconnected) = self.stop.recv_timeout(timeout) {
                        return;
                    }
                }
            }
            self.tick_if_due();
        }
    }

    fn take_due_checks(&mut self, now: Instant) -> bool {
        let due = self.access_checks.iter().filter(|at| **at <= now).count();
        self.access_checks.drain(..due);
        due > 0
    }

    fn next_wake(&self, now: Instant) -> Instant {
        let mut wake = now + WAKE_EVERY;
        if let Some(tick) = self.machine.next_tick() {
            wake = wake.min(tick);
        }
        if let Some(check) = self.access_checks.first() {
            wake = wake.min(*check);
        }
        if self.keyboard.is_none() {
            wake = wake.min(self.retry_at);
        }
        wake
    }

    fn try_start(&mut self) {
        self.retry_at = Instant::now() + RETRY_EVERY;
        if !accessibility_granted() {
            self.report(Unavailable::NoAccessibility);
            return;
        }
        match KeyboardListener::new() {
            Ok(keyboard) => {
                self.keyboard = Some(keyboard);
                self.reported = None;
                self.send(ListenerEvent::Ready);
            }
            Err(handy_keys::Error::AccessibilityNotGranted) => {
                self.report(Unavailable::NoAccessibility);
            }
            Err(error) => self.report(Unavailable::ListenerFailed(error.to_string())),
        }
    }

    /// Takes the event tap down, which puts it out of the input path, and cancels a running take.
    fn lose(&mut self, reason: Unavailable) {
        self.keyboard = None;
        self.retry_at = Instant::now() + RETRY_EVERY;
        if let Some(event) = self.machine.reset() {
            self.send(ListenerEvent::Hold(event));
        }
        self.report(reason);
    }

    fn report(&mut self, reason: Unavailable) {
        if self.reported.as_ref() != Some(&reason) {
            self.reported = Some(reason.clone());
            self.send(ListenerEvent::Unavailable(reason));
        }
    }

    fn on_key_event(&mut self, event: &KeyEvent) {
        let now = Instant::now();
        let hold_event = match classify(event, self.hold_key) {
            KeyChange::HoldKey => self.machine.hold_key_changed(now, sample(self.hold_key)),
            KeyChange::OtherKeyPressed => self.machine.other_key_pressed(now),
            KeyChange::Ignored => None,
        };
        if let Some(hold_event) = hold_event {
            self.send(ListenerEvent::Hold(hold_event));
        }
    }

    fn tick_if_due(&mut self) {
        let now = Instant::now();
        if self.machine.next_tick().is_some_and(|due| due <= now)
            && let Some(event) = self.machine.tick(now, sample(self.hold_key))
        {
            self.send(ListenerEvent::Hold(event));
        }
    }

    /// A send fails only once the listener is dropped, which the stop channel reports too.
    fn send(&self, event: ListenerEvent) {
        let _ = self.events.send(event);
    }
}
