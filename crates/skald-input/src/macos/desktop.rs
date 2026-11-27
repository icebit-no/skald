use std::time::{Duration, Instant};

use objc2_app_kit::NSPasteboard;
use objc2_core_graphics::CGPreflightPostEventAccess;

use super::{keyboard, pasteboard};
use crate::insert::{ChangeStamp, Desktop, Previous};

#[derive(Debug, Default)]
pub(crate) struct MacDesktop;

impl Desktop for MacDesktop {
    type Snapshot = pasteboard::Snapshot;

    fn may_send_keys(&self) -> bool {
        CGPreflightPostEventAccess()
    }

    fn modifiers_down(&self) -> bool {
        keyboard::modifiers_down()
    }

    fn read_clipboard(&mut self) -> Previous<pasteboard::Snapshot> {
        pasteboard::read(&NSPasteboard::generalPasteboard())
    }

    fn write_text(&mut self, text: &str, transient: bool) -> Option<ChangeStamp> {
        pasteboard::write_text(&NSPasteboard::generalPasteboard(), text, transient)
    }

    fn write_snapshot(&mut self, snapshot: pasteboard::Snapshot) -> Option<ChangeStamp> {
        pasteboard::write_snapshot(&NSPasteboard::generalPasteboard(), snapshot)
    }

    fn change_stamp(&self) -> ChangeStamp {
        NSPasteboard::generalPasteboard().changeCount()
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
