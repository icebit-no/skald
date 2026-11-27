//! Listens for the hold key and the hands-free chord on a thread of its own and reports what each means
//! for the take. The chord is kept from the app in front, so it reaches Skald alone.

use std::collections::HashSet;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use handy_keys::{KeyEvent, KeyboardListener};

use crate::hold::HoldMachine;
use crate::hotkey::{HANDS_FREE, HoldKey, KeyChange, classify};
use crate::listening::{ListenerEvent, Unavailable};
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

/// What the app tells the listener's thread.
#[derive(Debug)]
enum Command {
    HandsFreeEnded,
}

/// Listens for the hold key and the hands-free chord until dropped.
#[derive(Debug)]
pub struct HoldListener {
    events: Receiver<ListenerEvent>,
    /// Dropping it tells the thread to stop.
    commands: Sender<Command>,
}

impl HoldListener {
    pub fn start(hold_key: HoldKey) -> std::io::Result<Self> {
        let (events_tx, events) = mpsc::channel();
        let (commands, commands_rx) = mpsc::channel();
        thread::Builder::new()
            .name("skald-hold-key".into())
            .spawn(move || Driver::new(hold_key, events_tx, commands_rx).run())?;
        Ok(Self { events, commands })
    }

    pub fn events(&self) -> &Receiver<ListenerEvent> {
        &self.events
    }

    /// The hands-free take ended without the chord: at the cap, when its microphone went away, or because
    /// it could not start. The next press of the chord then starts a new one.
    pub fn hands_free_ended(&self) {
        // Fails only once the thread has stopped, and then there is no take to end.
        let _ = self.commands.send(Command::HandsFreeEnded);
    }
}

struct Driver {
    hold_key: HoldKey,
    events: Sender<ListenerEvent>,
    commands: Receiver<Command>,
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
    fn new(hold_key: HoldKey, events: Sender<ListenerEvent>, commands: Receiver<Command>) -> Self {
        Self {
            hold_key,
            events,
            commands,
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
            if !self.take_commands() {
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
                Some(Ok(event)) => {
                    // A take the app ended is known before a press that may start the next one.
                    if !self.take_commands() {
                        return;
                    }
                    self.on_key_event(&event);
                }
                Some(Err(handy_keys::Error::Timeout)) => {}
                Some(Err(error)) => {
                    let reason = if accessibility_granted() {
                        Unavailable::ListenerFailed(error.to_string())
                    } else {
                        Unavailable::NoAccessibility
                    };
                    self.lose(reason);
                }
                None => match self.commands.recv_timeout(timeout) {
                    Ok(command) => self.on_command(command),
                    Err(RecvTimeoutError::Disconnected) => return,
                    Err(RecvTimeoutError::Timeout) => {}
                },
            }
            self.tick_if_due();
        }
    }

    /// Carries out what the app has said; false once the listener is dropped.
    fn take_commands(&mut self) -> bool {
        loop {
            match self.commands.try_recv() {
                Ok(command) => self.on_command(command),
                Err(TryRecvError::Empty) => return true,
                Err(TryRecvError::Disconnected) => return false,
            }
        }
    }

    fn on_command(&mut self, command: Command) {
        match command {
            Command::HandsFreeEnded => self.machine.hands_free_ended(),
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
        let chord: HashSet<_> = HANDS_FREE.iter().map(|chord| chord.hotkey()).collect();
        match KeyboardListener::new_with_blocking(Arc::new(Mutex::new(chord))) {
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
        let hold_event = match classify(event, self.hold_key, HANDS_FREE) {
            KeyChange::HoldKey => self.machine.hold_key_changed(now, sample(self.hold_key)),
            KeyChange::HandsFreePressed => self.machine.hands_free_pressed(now),
            KeyChange::HandsFreeReleased => {
                self.machine.hands_free_released();
                None
            }
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

    /// A send fails only once the listener is dropped, which the command channel reports too.
    fn send(&self, event: ListenerEvent) {
        let _ = self.events.send(event);
    }
}
