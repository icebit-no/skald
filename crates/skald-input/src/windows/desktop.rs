use std::time::{Duration, Instant};

use super::{clipboard, elevation, keyboard};
use crate::insert::{ChangeStamp, Desktop, Previous};

#[derive(Debug, Default)]
pub(crate) struct WindowsDesktop;

impl Desktop for WindowsDesktop {
    type Snapshot = clipboard::Snapshot;

    /// Windows asks no permission to send keystrokes.
    fn may_send_keys(&self) -> bool {
        true
    }

    fn front_window_refuses_keys(&self) -> bool {
        elevation::foreground_window_elevated()
    }

    fn modifiers_down(&self) -> bool {
        keyboard::modifiers_down()
    }

    fn read_clipboard(&mut self) -> Previous<clipboard::Snapshot> {
        clipboard::read()
    }

    fn write_text(&mut self, text: &str, transient: bool) -> Option<ChangeStamp> {
        clipboard::write_text(text, transient)
    }

    fn write_snapshot(&mut self, snapshot: clipboard::Snapshot) -> Option<ChangeStamp> {
        clipboard::write_snapshot(snapshot)
    }

    fn change_stamp(&self) -> ChangeStamp {
        clipboard::stamp()
    }

    fn send_paste(&mut self) -> bool {
        keyboard::send_paste()
    }

    fn now(&self) -> Instant {
        Instant::now()
    }

    fn sleep(&mut self, duration: Duration) {
        std::thread::sleep(duration);
    }
}
