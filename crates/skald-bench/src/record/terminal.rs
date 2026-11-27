//! The terminal the recorder runs in: raw mode, so a key acts without Enter after it, and key releases
//! where the terminal reports them.
//!
//! A terminal sends a key when it goes down, and again while it is held, but not when it comes up.
//! Terminals that speak the kitty keyboard protocol (Ghostty, kitty, WezTerm and others) report the
//! release when asked to, and in them a take lasts while Enter is held. In the others, macOS's own
//! Terminal among them, a take lasts from one press of Enter to the next.

use std::io::{self, IsTerminal, Write};
use std::time::Duration;

use crossterm::event::{
    self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, KeyboardEnhancementFlags,
    PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
};
use crossterm::{execute, terminal};

use super::session::{Key, Keys, TakeEnd};

/// Raw mode while it lives, and the terminal as it was once it is dropped, a panic included.
pub(super) struct Terminal {
    take_end: TakeEnd,
}

impl Terminal {
    pub(super) fn open() -> Result<Self, String> {
        terminal::enable_raw_mode()
            .map_err(|e| format!("Kunne ikke ta over tastene i terminalen: {e}"))?;
        // Asking writes to standard output, which may be a file when the summary is.
        let releases = io::stdout().is_terminal()
            && terminal::supports_keyboard_enhancement().unwrap_or(false);
        // Enter's release is reported only when every key comes as an escape code, so that a shell
        // left in this mode by a crash can still run `reset`.
        let reported = releases
            && execute!(
                io::stderr(),
                PushKeyboardEnhancementFlags(
                    KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                        | KeyboardEnhancementFlags::REPORT_EVENT_TYPES
                        | KeyboardEnhancementFlags::REPORT_ALL_KEYS_AS_ESCAPE_CODES
                )
            )
            .is_ok();
        Ok(Self {
            take_end: if reported {
                TakeEnd::Release
            } else {
                TakeEnd::SecondPress
            },
        })
    }

    pub(super) fn take_end(&self) -> TakeEnd {
        self.take_end
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        if self.take_end == TakeEnd::Release {
            let _ = execute!(io::stderr(), PopKeyboardEnhancementFlags);
        }
        let _ = terminal::disable_raw_mode();
        let _ = io::stderr().flush();
    }
}

impl Keys for Terminal {
    fn next(&mut self, timeout: Duration) -> io::Result<Option<Key>> {
        if !event::poll(timeout)? {
            return Ok(None);
        }
        Ok(match event::read()? {
            Event::Key(event) => key(event),
            _ => None,
        })
    }
}

/// The recorder's meaning of a key event. Ctrl+C quits, since raw mode keeps it from stopping the
/// program; a key held down repeating means nothing.
fn key(event: KeyEvent) -> Option<Key> {
    match (event.kind, event.code) {
        (KeyEventKind::Release, KeyCode::Enter) => Some(Key::EnterReleased),
        (KeyEventKind::Press, KeyCode::Enter) => Some(Key::Enter),
        (KeyEventKind::Press, KeyCode::Char('c')) if event.modifiers == KeyModifiers::CONTROL => {
            Some(Key::Quit)
        }
        (KeyEventKind::Press, _) if !event.modifiers.difference(KeyModifiers::SHIFT).is_empty() => {
            None
        }
        (KeyEventKind::Press, KeyCode::Char('n' | 'N')) => Some(Key::Redo),
        (KeyEventKind::Press, KeyCode::Char('h' | 'H')) => Some(Key::Skip),
        (KeyEventKind::Press, KeyCode::Char('q' | 'Q') | KeyCode::Esc) => Some(Key::Quit),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::KeyEventState;

    use super::*;

    fn event(code: KeyCode, modifiers: KeyModifiers, kind: KeyEventKind) -> KeyEvent {
        KeyEvent {
            code,
            modifiers,
            kind,
            state: KeyEventState::NONE,
        }
    }

    fn pressed(code: KeyCode) -> Option<Key> {
        key(event(code, KeyModifiers::NONE, KeyEventKind::Press))
    }

    #[test]
    fn keys_mean_what_the_screen_says() {
        assert_eq!(pressed(KeyCode::Enter), Some(Key::Enter));
        assert_eq!(
            key(event(
                KeyCode::Enter,
                KeyModifiers::NONE,
                KeyEventKind::Release
            )),
            Some(Key::EnterReleased)
        );
        assert_eq!(
            key(event(
                KeyCode::Enter,
                KeyModifiers::NONE,
                KeyEventKind::Repeat
            )),
            None
        );
        assert_eq!(pressed(KeyCode::Char('n')), Some(Key::Redo));
        assert_eq!(
            key(event(
                KeyCode::Char('N'),
                KeyModifiers::SHIFT,
                KeyEventKind::Press
            )),
            Some(Key::Redo)
        );
        assert_eq!(pressed(KeyCode::Char('h')), Some(Key::Skip));
        assert_eq!(pressed(KeyCode::Char('q')), Some(Key::Quit));
        assert_eq!(pressed(KeyCode::Esc), Some(Key::Quit));
        assert_eq!(
            key(event(
                KeyCode::Char('c'),
                KeyModifiers::CONTROL,
                KeyEventKind::Press
            )),
            Some(Key::Quit)
        );
    }

    #[test]
    fn other_keys_and_shortcuts_mean_nothing() {
        assert_eq!(pressed(KeyCode::Char('x')), None);
        assert_eq!(pressed(KeyCode::Char(' ')), None);
        assert_eq!(
            key(event(
                KeyCode::Char('n'),
                KeyModifiers::SUPER,
                KeyEventKind::Press
            )),
            None
        );
        assert_eq!(
            key(event(
                KeyCode::Char('q'),
                KeyModifiers::NONE,
                KeyEventKind::Release
            )),
            None
        );
    }
}
