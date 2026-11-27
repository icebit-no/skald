//! What insertion needs from a Wayland desktop: the clipboard through data-control, and Ctrl+V through
//! wtype, which sends it from a virtual keyboard.

use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use super::clipboard::{Clipboard, Snapshot};
use crate::insert::{ChangeStamp, Desktop, Previous};

/// Connects to the clipboard when first needed, and again after a failure, so a Skald started before the
/// compositor was ready inserts once it is.
#[derive(Default)]
pub(crate) struct WaylandDesktop {
    clipboard: Option<Clipboard>,
}

impl std::fmt::Debug for WaylandDesktop {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WaylandDesktop")
            .field("connected", &self.clipboard.is_some())
            .finish()
    }
}

impl WaylandDesktop {
    /// Runs `with` on the clipboard, connecting first when needed, and drops a connection that failed so
    /// the next call makes a new one.
    fn with_clipboard<T>(&mut self, with: impl FnOnce(&Clipboard) -> Option<T>) -> Option<T> {
        if self.clipboard.is_none() {
            self.clipboard = Clipboard::connect().ok();
        }
        let answer = with(self.clipboard.as_ref()?);
        if answer.is_none() {
            self.clipboard = None;
        }
        answer
    }
}

impl Desktop for WaylandDesktop {
    type Snapshot = Snapshot;

    /// Wayland asks no permission of a virtual keyboard.
    fn may_send_keys(&self) -> bool {
        true
    }

    /// A Wayland program cannot see the keyboard, so the modifiers read as up. The hold key is up by then:
    /// its release is what ended the take.
    fn modifiers_down(&self) -> bool {
        false
    }

    fn read_clipboard(&mut self) -> Previous<Snapshot> {
        self.with_clipboard(|clipboard| Some(clipboard.read()))
            .unwrap_or(Previous::Unreadable)
    }

    fn write_text(&mut self, text: &str, transient: bool) -> Option<ChangeStamp> {
        self.with_clipboard(|clipboard| clipboard.write(Snapshot::text(text), transient))
    }

    fn write_snapshot(&mut self, snapshot: Snapshot) -> Option<ChangeStamp> {
        self.with_clipboard(|clipboard| clipboard.write(snapshot, false))
    }

    /// A stamp that cannot be read reads as a change, so the previous contents are not written over
    /// something unknown.
    fn change_stamp(&self) -> ChangeStamp {
        self.clipboard
            .as_ref()
            .and_then(Clipboard::stamp)
            .unwrap_or(ChangeStamp::MIN)
    }

    fn send_paste(&mut self) -> bool {
        Command::new("wtype")
            .args(["-M", "ctrl", "v", "-m", "ctrl"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
    }

    fn now(&self) -> Instant {
        Instant::now()
    }

    fn sleep(&mut self, duration: Duration) {
        thread::sleep(duration);
    }
}
