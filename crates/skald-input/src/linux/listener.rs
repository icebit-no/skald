//! The hold key on Linux. A Wayland compositor gives the keyboard to no other program, so the desktop's own
//! key binding reports the hold key: it runs `skald start` as the key goes down and `skald stop` as it comes
//! up, and the running Skald hands each to [`press_hold_key`]. The hands-free chord, Ctrl+Super+X, is a
//! binding of its own that runs `skald toggle` ([`toggle_hands_free`]). A desktop with the GlobalShortcuts
//! portal can report both there instead (`portal`). The presses go through the same rules as a key heard from
//! the keyboard ([`HoldMachine`]): a take arms once the key has been down for 200 ms and ends when it comes
//! up. A press reported twice, as by two bindings for the same key, counts once.
//!
//! A binding reports the hold key and nothing else. A shortcut made with it, as right Ctrl+C, does not
//! cancel the take; held past arming it records, and the silence gate keeps a take without speech from
//! becoming text. A release the binding never reports leaves the take running until the 120-second cap.

use std::io;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Mutex, PoisonError};
use std::thread;
use std::time::Instant;

use crate::hold::{HoldMachine, KeySample};
use crate::hotkey::HoldKey;
use crate::listening::{ListenerEvent, PressRefused};

/// The running listener, which [`press_hold_key`] reaches from the socket's thread. The key is the
/// session's, and one Skald takes dictation at a time, so there is at most one listener to reach.
static LISTENING: Mutex<Option<Listening>> = Mutex::new(None);

static NEXT_LISTENER: AtomicU64 = AtomicU64::new(1);

struct Listening {
    listener: u64,
    commands: Sender<Command>,
}

/// What the app and the key binding tell the listener's thread.
#[derive(Debug)]
enum Command {
    HoldKey { down: bool },
    HandsFreeChord,
    HandsFreeEnded,
}

/// Takes the hold key from the desktop's key binding until dropped.
#[derive(Debug)]
pub struct HoldListener {
    events: Receiver<ListenerEvent>,
    /// Dropping it, with the copy [`press_hold_key`] reaches, tells the thread to stop.
    commands: Sender<Command>,
    id: u64,
}

impl HoldListener {
    /// The binding decides which key it reports, so `hold_key` names it only for what Skald shows.
    pub fn start(_hold_key: HoldKey) -> io::Result<Self> {
        let (events_tx, events) = mpsc::channel();
        let (commands, commands_rx) = mpsc::channel();
        thread::Builder::new()
            .name("skald-hold-key".into())
            .spawn(move || Driver::new(events_tx, commands_rx).run())?;
        super::portal::start();
        let id = NEXT_LISTENER.fetch_add(1, Ordering::Relaxed);
        *listening() = Some(Listening {
            listener: id,
            commands: commands.clone(),
        });
        Ok(Self {
            events,
            commands,
            id,
        })
    }

    pub fn events(&self) -> &Receiver<ListenerEvent> {
        &self.events
    }

    /// The hands-free take ended without the chord. The next press of the chord then starts a new one.
    pub fn hands_free_ended(&self) {
        // Fails only once the thread has stopped, and then there is no take to end.
        let _ = self.commands.send(Command::HandsFreeEnded);
    }
}

impl Drop for HoldListener {
    fn drop(&mut self) {
        let mut listening = listening();
        if listening
            .as_ref()
            .is_some_and(|running| running.listener == self.id)
        {
            *listening = None;
        }
    }
}

/// Presses the hold key (`down`) or releases it, as the desktop's key binding reports it.
pub fn press_hold_key(down: bool) -> Result<(), PressRefused> {
    send(Command::HoldKey { down })
}

/// Presses the hands-free chord once, as its key binding reports it: starts a hands-free take, or stops the
/// one running.
pub fn toggle_hands_free() -> Result<(), PressRefused> {
    send(Command::HandsFreeChord)
}

fn send(command: Command) -> Result<(), PressRefused> {
    let listening = listening();
    let running = listening.as_ref().ok_or(PressRefused::NotListening)?;
    running
        .commands
        .send(command)
        .map_err(|_| PressRefused::NotListening)
}

fn listening() -> std::sync::MutexGuard<'static, Option<Listening>> {
    LISTENING.lock().unwrap_or_else(PoisonError::into_inner)
}

struct Driver {
    events: Sender<ListenerEvent>,
    commands: Receiver<Command>,
    machine: HoldMachine,
    /// The hold key as the binding last reported it.
    down: bool,
}

impl Driver {
    fn new(events: Sender<ListenerEvent>, commands: Receiver<Command>) -> Self {
        Self {
            events,
            commands,
            machine: HoldMachine::new(),
            down: false,
        }
    }

    fn run(mut self) {
        // A binding needs no permission, so the key is listened for from the start.
        self.send(ListenerEvent::Ready);
        loop {
            let command = match self.machine.next_tick() {
                Some(due) => {
                    match self
                        .commands
                        .recv_timeout(due.saturating_duration_since(Instant::now()))
                    {
                        Ok(command) => Some(command),
                        Err(RecvTimeoutError::Timeout) => None,
                        Err(RecvTimeoutError::Disconnected) => return,
                    }
                }
                None => match self.commands.recv() {
                    Ok(command) => Some(command),
                    Err(_) => return,
                },
            };
            if let Some(command) = command {
                self.on_command(command);
            }
            self.tick_if_due();
        }
    }

    fn on_command(&mut self, command: Command) {
        match command {
            Command::HoldKey { down } => {
                self.down = down;
                if let Some(event) = self.machine.hold_key_changed(Instant::now(), self.sample()) {
                    self.send(ListenerEvent::Hold(event));
                }
            }
            Command::HandsFreeChord => {
                if let Some(event) = self.machine.hands_free_pressed(Instant::now()) {
                    self.send(ListenerEvent::Hold(event));
                }
                // A binding reports a press and no release, so the chord is up again at once.
                self.machine.hands_free_released();
            }
            Command::HandsFreeEnded => self.machine.hands_free_ended(),
        }
    }

    /// Arms a press held long enough. The watchdog finds the key as the binding last reported it.
    fn tick_if_due(&mut self) {
        let now = Instant::now();
        if self.machine.next_tick().is_some_and(|due| due <= now)
            && let Some(event) = self.machine.tick(now, self.sample())
        {
            self.send(ListenerEvent::Hold(event));
        }
    }

    fn sample(&self) -> KeySample {
        KeySample {
            hold_key_down: self.down,
            other_modifiers_down: false,
        }
    }

    /// A send fails only once the listener is dropped, which the command channel reports too.
    fn send(&self, event: ListenerEvent) {
        let _ = self.events.send(event);
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::hold::{ARM_AFTER, HoldEvent};

    /// Long enough for the thread to answer, short enough to fail a test that waits for nothing.
    const ANSWER_WITHIN: Duration = Duration::from_secs(2);

    fn next(listener: &HoldListener) -> ListenerEvent {
        listener.events().recv_timeout(ANSWER_WITHIN).unwrap()
    }

    fn quiet(listener: &HoldListener, for_: Duration) {
        assert_eq!(
            listener.events().recv_timeout(for_),
            Err(RecvTimeoutError::Timeout)
        );
    }

    // One test, since every listener in the process shares the one [`press_hold_key`] reaches.
    #[test]
    fn a_binding_presses_and_releases_the_hold_key_under_the_hold_rules() {
        assert_eq!(press_hold_key(true), Err(PressRefused::NotListening));

        let listener = HoldListener::start(HoldKey::RightControl).unwrap();
        assert_eq!(next(&listener), ListenerEvent::Ready);

        // Held past arming, released: a take.
        press_hold_key(true).unwrap();
        let pressed = Instant::now();
        assert!(matches!(
            next(&listener),
            ListenerEvent::Hold(HoldEvent::Pressed { .. })
        ));
        // A second binding for the same key reports the press again.
        press_hold_key(true).unwrap();
        assert!(matches!(
            next(&listener),
            ListenerEvent::Hold(HoldEvent::Armed { .. })
        ));
        assert!(pressed.elapsed() >= ARM_AFTER);
        press_hold_key(false).unwrap();
        assert_eq!(next(&listener), ListenerEvent::Hold(HoldEvent::Released));
        press_hold_key(false).unwrap();
        quiet(&listener, ARM_AFTER);

        // Released before arming: no take.
        press_hold_key(true).unwrap();
        assert!(matches!(
            next(&listener),
            ListenerEvent::Hold(HoldEvent::Pressed { .. })
        ));
        press_hold_key(false).unwrap();
        assert_eq!(next(&listener), ListenerEvent::Hold(HoldEvent::NotArmed));
        quiet(&listener, ARM_AFTER * 2);

        // The hands-free chord starts a take, the hold key does nothing meanwhile, and the chord stops it.
        toggle_hands_free().unwrap();
        assert!(matches!(
            next(&listener),
            ListenerEvent::Hold(HoldEvent::HandsFreeStarted { .. })
        ));
        press_hold_key(true).unwrap();
        press_hold_key(false).unwrap();
        quiet(&listener, ARM_AFTER * 2);
        toggle_hands_free().unwrap();
        assert_eq!(
            next(&listener),
            ListenerEvent::Hold(HoldEvent::HandsFreeStopped)
        );
        // A take that ended at the cap leaves the next chord to start a new one.
        toggle_hands_free().unwrap();
        assert!(matches!(
            next(&listener),
            ListenerEvent::Hold(HoldEvent::HandsFreeStarted { .. })
        ));
        listener.hands_free_ended();
        toggle_hands_free().unwrap();
        assert!(matches!(
            next(&listener),
            ListenerEvent::Hold(HoldEvent::HandsFreeStarted { .. })
        ));
        toggle_hands_free().unwrap();
        assert_eq!(
            next(&listener),
            ListenerEvent::Hold(HoldEvent::HandsFreeStopped)
        );

        // A listener started in its place takes the presses; dropping the old one leaves the new one.
        let replacement = HoldListener::start(HoldKey::RightControl).unwrap();
        assert_eq!(next(&replacement), ListenerEvent::Ready);
        drop(listener);
        press_hold_key(true).unwrap();
        assert!(matches!(
            next(&replacement),
            ListenerEvent::Hold(HoldEvent::Pressed { .. })
        ));

        drop(replacement);
        assert_eq!(press_hold_key(false), Err(PressRefused::NotListening));
        assert_eq!(toggle_hands_free(), Err(PressRefused::NotListening));
    }
}
