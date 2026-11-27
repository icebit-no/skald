//! Keeps the active microphone up to date while microphones come and go.

use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::capture::{CaptureError, connected_microphones};
use crate::microphone::{Connected, Microphone, MicrophoneChange, Selector};
use crate::platform::DeviceChanges;

/// A device that has just appeared may not list its input yet, so the list is read once more this
/// long after a change.
const SETTLE: Duration = Duration::from_secs(1);

enum Signal {
    DevicesChanged,
    SetOrder(Vec<Microphone>),
    Stop,
}

/// Follows the connected microphones on a thread of its own and reports each change of the
/// microphone a take would use. Watching opens no microphone.
pub struct MicrophoneWatcher {
    _changes: DeviceChanges,
    signals: Sender<Signal>,
    active: Arc<Mutex<Option<Microphone>>>,
    thread: Option<JoinHandle<()>>,
}

impl MicrophoneWatcher {
    /// Starts watching with the user's order, most wanted first. Changes arrive on the receiver.
    pub fn start(
        order: Vec<Microphone>,
    ) -> Result<(Self, Receiver<MicrophoneChange>), CaptureError> {
        let selector = Selector::new(order, connected_microphones()?);
        let active = Arc::new(Mutex::new(selector.active().cloned()));
        let (signals, inbox) = mpsc::channel();
        let (reports, changes) = mpsc::channel();

        let notices = signals.clone();
        let device_changes = DeviceChanges::watch(move || {
            // The watcher may already have stopped; then no one needs to know.
            let _ = notices.send(Signal::DevicesChanged);
        })?;
        let shared = active.clone();
        let thread = thread::Builder::new()
            .name("skald-audio-watch".into())
            .spawn(move || {
                let list = || connected_microphones().ok();
                let timing = Timing {
                    poll: DeviceChanges::POLL,
                    settle: SETTLE,
                };
                run(selector, list, timing, &inbox, &shared, &reports);
            })
            .map_err(|e| CaptureError::System(format!("cannot start the watcher: {e}")))?;

        let watcher = Self {
            _changes: device_changes,
            signals,
            active,
            thread: Some(thread),
        };
        Ok((watcher, changes))
    }

    /// The microphone a take started now would use.
    pub fn active(&self) -> Option<Microphone> {
        self.active
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Replaces the order; the choice moves at once if the new order calls for it.
    pub fn set_order(&self, order: Vec<Microphone>) {
        let _ = self.signals.send(Signal::SetOrder(order));
    }
}

impl Drop for MicrophoneWatcher {
    fn drop(&mut self) {
        let _ = self.signals.send(Signal::Stop);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

struct Timing {
    /// How often to list the microphones without a notice, where the system sends none.
    poll: Option<Duration>,
    settle: Duration,
}

/// Answers each signal: a notice lists the microphones now and once more after the settle time; a
/// new order chooses again from the last list. A list that fails mid-change gives `None` and is
/// retried on the next notice or look.
fn run(
    mut selector: Selector,
    mut list: impl FnMut() -> Option<Connected>,
    timing: Timing,
    inbox: &Receiver<Signal>,
    active: &Mutex<Option<Microphone>>,
    reports: &Sender<MicrophoneChange>,
) {
    let mut look_again_at: Option<Instant> = None;
    loop {
        let wait = look_again_at
            .map(|at| at.saturating_duration_since(Instant::now()))
            .or(timing.poll);
        let received = match wait {
            Some(wait) => inbox.recv_timeout(wait),
            None => inbox.recv().map_err(|_| RecvTimeoutError::Disconnected),
        };
        let change = match received {
            Ok(Signal::Stop) | Err(RecvTimeoutError::Disconnected) => return,
            Ok(Signal::SetOrder(order)) => selector.set_order(order),
            Ok(Signal::DevicesChanged) => {
                look_again_at = Some(Instant::now() + timing.settle);
                list().and_then(|connected| selector.update(connected))
            }
            Err(RecvTimeoutError::Timeout) => {
                look_again_at = None;
                list().and_then(|connected| selector.update(connected))
            }
        };
        *active.lock().unwrap_or_else(PoisonError::into_inner) = selector.active().cloned();
        if let Some(change) = change {
            let _ = reports.send(change);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MicId;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn mic(id: &str) -> Microphone {
        Microphone {
            id: MicId::new(id),
            name: id.to_owned(),
        }
    }

    fn connected(ids: &[&str]) -> Connected {
        Connected {
            microphones: ids.iter().map(|id| mic(id)).collect(),
            default: None,
        }
    }

    struct Running {
        signals: Sender<Signal>,
        changes: Receiver<MicrophoneChange>,
        active: Arc<Mutex<Option<Microphone>>>,
        system: Arc<Mutex<Connected>>,
        looks: Arc<AtomicUsize>,
        thread: JoinHandle<()>,
    }

    /// The loop over a fake system whose microphones the test sets, with short timings.
    fn running(order: &[&str], now: &[&str], poll: Option<Duration>) -> Running {
        let system = Arc::new(Mutex::new(connected(now)));
        let order = order.iter().map(|id| mic(id)).collect();
        let selector = Selector::new(order, connected(now));
        let active = Arc::new(Mutex::new(selector.active().cloned()));
        let (signals, inbox) = mpsc::channel();
        let (reports, changes) = mpsc::channel();
        let looks = Arc::new(AtomicUsize::new(0));
        let (seen, shared, counted) = (system.clone(), active.clone(), looks.clone());
        let thread = thread::spawn(move || {
            let list = || {
                let now = seen.lock().unwrap().clone();
                counted.fetch_add(1, Ordering::SeqCst);
                Some(now)
            };
            let timing = Timing {
                poll,
                settle: Duration::from_millis(20),
            };
            run(selector, list, timing, &inbox, &shared, &reports);
        });
        Running {
            signals,
            changes,
            active,
            system,
            looks,
            thread,
        }
    }

    const WAIT: Duration = Duration::from_secs(5);

    #[test]
    fn a_notice_reports_a_microphone_plugged_in() {
        let run = running(&["usb", "builtin"], &["builtin"], None);
        *run.system.lock().unwrap() = connected(&["builtin", "usb"]);
        run.signals.send(Signal::DevicesChanged).unwrap();
        let change = run.changes.recv_timeout(WAIT).unwrap();
        assert_eq!(change.from, Some(mic("builtin")));
        assert_eq!(change.to, Some(mic("usb")));
        assert_eq!(*run.active.lock().unwrap(), Some(mic("usb")));
        run.signals.send(Signal::Stop).unwrap();
        run.thread.join().unwrap();
    }

    #[test]
    fn a_microphone_that_lists_late_is_found_on_the_second_look() {
        let run = running(&["usb", "builtin"], &["builtin"], None);
        // The notice comes before the device lists its input.
        run.signals.send(Signal::DevicesChanged).unwrap();
        while run.looks.load(Ordering::SeqCst) == 0 {
            thread::yield_now();
        }
        *run.system.lock().unwrap() = connected(&["builtin", "usb"]);
        let change = run.changes.recv_timeout(WAIT).unwrap();
        assert_eq!(change.to, Some(mic("usb")));
        run.signals.send(Signal::Stop).unwrap();
        run.thread.join().unwrap();
    }

    #[test]
    fn without_notices_the_list_is_read_at_an_interval() {
        let run = running(
            &["usb", "builtin"],
            &["usb", "builtin"],
            Some(Duration::from_millis(10)),
        );
        *run.system.lock().unwrap() = connected(&["builtin"]);
        let change = run.changes.recv_timeout(WAIT).unwrap();
        assert_eq!(change.to, Some(mic("builtin")));
        run.signals.send(Signal::Stop).unwrap();
        run.thread.join().unwrap();
    }

    #[test]
    fn a_new_order_is_reported_without_a_notice() {
        let run = running(&["usb", "builtin"], &["builtin", "usb"], None);
        run.signals
            .send(Signal::SetOrder(vec![mic("builtin"), mic("usb")]))
            .unwrap();
        let change = run.changes.recv_timeout(WAIT).unwrap();
        assert_eq!(change.to, Some(mic("builtin")));
        run.signals.send(Signal::Stop).unwrap();
        run.thread.join().unwrap();
    }

    #[test]
    fn the_loop_ends_when_the_watcher_is_dropped() {
        let run = running(&[], &["builtin"], None);
        drop(run.signals);
        run.thread.join().unwrap();
        assert!(run.changes.try_recv().is_err());
    }
}
