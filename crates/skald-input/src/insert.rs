//! Inserting text where the cursor is: through the clipboard and a paste keystroke, never by synthetic
//! typing, and only once every modifier is up.

use std::fmt;
use std::time::{Duration, Instant};

/// How long the pasted text stays on the clipboard before the previous contents go back. The target app
/// reads the clipboard when it gets to the paste keystroke, which can take a while on a busy machine;
/// restoring sooner would paste the previous contents instead of the text. Nothing waits on the restore
/// but the next insertion, so the margin is generous.
pub(crate) const RESTORE_AFTER: Duration = Duration::from_secs(1);

/// How often the modifiers are checked while waiting for them to come up.
const KEYS_UP_POLL: Duration = Duration::from_millis(20);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InsertOptions {
    /// «Kopier også teksten til utklippstavlen»: leave the text on the clipboard instead of putting the
    /// previous contents back.
    pub keep_text_on_clipboard: bool,
    /// How long to wait for every modifier to come up before giving up. A paste sent while one is down
    /// becomes another shortcut, and the hold key itself may still be down when a take's text is ready,
    /// as when the next take starts before the previous one is inserted. The default outlasts the longest
    /// take.
    pub keys_up_within: Duration,
}

impl Default for InsertOptions {
    fn default() -> Self {
        Self {
            keep_text_on_clipboard: false,
            keys_up_within: Duration::from_secs(150),
        }
    }
}

/// How an insertion went, told by what the clipboard holds afterwards.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Inserted {
    /// The text was empty, so nothing was pasted and the clipboard is untouched.
    Nothing,
    /// The previous clipboard contents are back.
    PreviousRestored,
    /// The text stays on the clipboard, as the options asked.
    TextKept,
    /// The text stays on the clipboard because the previous contents could not be read.
    PreviousUnreadable,
    /// The text stays on the clipboard because the previous contents were marked concealed or transient,
    /// as a password manager marks a secret, and are not put back.
    PreviousMarkedPrivate,
    /// Something was copied after the text was pasted; that copy is left in place.
    CopiedMeanwhile,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InsertError {
    /// The app may not send keystrokes: macOS Accessibility is not granted. The clipboard is untouched.
    NoAccessibility,
    /// A modifier stayed down for the whole wait. The clipboard is untouched.
    KeysHeld,
    /// The clipboard refused the text.
    Clipboard,
    /// The paste keystroke could not be sent. Previous contents that were to be put back are back.
    Paste,
}

impl fmt::Display for InsertError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            InsertError::NoAccessibility => {
                "Accessibility is not granted, so the paste cannot be sent"
            }
            InsertError::KeysHeld => "a modifier stayed down, so the paste was not sent",
            InsertError::Clipboard => "the clipboard refused the text",
            InsertError::Paste => "the paste keystroke could not be sent",
        })
    }
}

impl std::error::Error for InsertError {}

/// Identifies one write to the clipboard; any later write gives a different stamp.
pub(crate) type ChangeStamp = isize;

/// What the clipboard held before the text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Previous<S> {
    /// Read in full, to be written back after the paste.
    Saved(S),
    Unreadable,
    /// Marked concealed or transient, as a password manager marks a secret. Its data is not read: put
    /// back, the secret would outlive the manager's own clearing, which acts only while the clipboard
    /// still holds what the manager put there.
    MarkedPrivate,
}

/// What the clipboard is to hold once the text is pasted.
enum AfterPaste<S> {
    Restore(S),
    LeaveText(Inserted),
}

/// What insertion needs from the desktop, so the sequence is tested without one.
pub(crate) trait Desktop {
    /// Everything the clipboard held, to be written back unchanged.
    type Snapshot;

    fn may_send_keys(&self) -> bool;
    fn modifiers_down(&self) -> bool;
    fn read_clipboard(&mut self) -> Previous<Self::Snapshot>;
    /// `transient` marks the text as on the clipboard only for the paste, so clipboard history tools skip
    /// it.
    fn write_text(&mut self, text: &str, transient: bool) -> Option<ChangeStamp>;
    fn write_snapshot(&mut self, snapshot: Self::Snapshot) -> Option<ChangeStamp>;
    fn change_stamp(&self) -> ChangeStamp;
    fn send_paste(&mut self) -> bool;
    fn now(&self) -> Instant;
    fn sleep(&mut self, duration: Duration);
}

/// The text as pasted: one trailing space, so consecutive takes do not run together. `None` when there
/// is nothing to paste.
pub(crate) fn pasted_text(text: &str) -> Option<String> {
    let text = text.trim_end();
    (!text.is_empty()).then(|| format!("{text} "))
}

pub(crate) fn insert_with<D: Desktop>(
    desktop: &mut D,
    text: &str,
    options: &InsertOptions,
) -> Result<Inserted, InsertError> {
    let Some(text) = pasted_text(text) else {
        return Ok(Inserted::Nothing);
    };
    if !desktop.may_send_keys() {
        return Err(InsertError::NoAccessibility);
    }
    wait_for_keys_up(desktop, options.keys_up_within)?;

    let after_paste = if options.keep_text_on_clipboard {
        AfterPaste::LeaveText(Inserted::TextKept)
    } else {
        match desktop.read_clipboard() {
            Previous::Saved(snapshot) => AfterPaste::Restore(snapshot),
            Previous::Unreadable => AfterPaste::LeaveText(Inserted::PreviousUnreadable),
            Previous::MarkedPrivate => AfterPaste::LeaveText(Inserted::PreviousMarkedPrivate),
        }
    };
    let restore = matches!(after_paste, AfterPaste::Restore(_));
    let stamp = desktop
        .write_text(&text, restore)
        .ok_or(InsertError::Clipboard)?;

    if !desktop.send_paste() {
        if let AfterPaste::Restore(snapshot) = after_paste {
            desktop.write_snapshot(snapshot);
        }
        return Err(InsertError::Paste);
    }

    let snapshot = match after_paste {
        AfterPaste::Restore(snapshot) => snapshot,
        AfterPaste::LeaveText(outcome) => return Ok(outcome),
    };
    desktop.sleep(RESTORE_AFTER);
    if desktop.change_stamp() != stamp {
        return Ok(Inserted::CopiedMeanwhile);
    }
    desktop
        .write_snapshot(snapshot)
        .ok_or(InsertError::Clipboard)?;
    Ok(Inserted::PreviousRestored)
}

fn wait_for_keys_up<D: Desktop>(desktop: &mut D, within: Duration) -> Result<(), InsertError> {
    let deadline = desktop.now() + within;
    while desktop.modifiers_down() {
        if desktop.now() >= deadline {
            return Err(InsertError::KeysHeld);
        }
        desktop.sleep(KEYS_UP_POLL);
    }
    Ok(())
}

#[cfg(target_os = "macos")]
pub use platform::Inserter;

#[cfg(target_os = "macos")]
mod platform {
    use super::{InsertError, InsertOptions, Inserted, insert_with};
    use crate::macos::desktop::MacDesktop;

    /// Inserts text where the cursor is. Each call blocks until the previous clipboard contents are
    /// back, so insertions never overlap.
    #[derive(Debug, Default)]
    pub struct Inserter {
        desktop: MacDesktop,
    }

    impl Inserter {
        pub fn new() -> Self {
            Self::default()
        }

        pub fn insert(
            &mut self,
            text: &str,
            options: &InsertOptions,
        ) -> Result<Inserted, InsertError> {
            insert_with(&mut self.desktop, text, options)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum Step {
        ReadClipboard,
        WriteText { text: String, transient: bool },
        WriteSnapshot(&'static str),
        SendPaste,
    }

    /// A desktop whose clock moves only when it sleeps, and which records what was done to it.
    struct FakeDesktop {
        start: Instant,
        elapsed: Duration,
        may_send_keys: bool,
        /// Modifiers stay down until this much time has passed.
        modifiers_down_until: Duration,
        clipboard: Previous<&'static str>,
        stamp: ChangeStamp,
        paste_works: bool,
        /// Someone copies something this long after the paste.
        copy_after_paste: Option<Duration>,
        pasted_at: Option<Duration>,
        steps: Vec<Step>,
        /// What the clipboard held each time the paste keystroke was sent.
        clipboard_at_paste: Vec<String>,
        current_text: Option<String>,
    }

    impl FakeDesktop {
        fn new() -> Self {
            Self {
                start: Instant::now(),
                elapsed: Duration::ZERO,
                may_send_keys: true,
                modifiers_down_until: Duration::ZERO,
                clipboard: Previous::Saved("previous"),
                stamp: 7,
                paste_works: true,
                copy_after_paste: None,
                pasted_at: None,
                steps: Vec::new(),
                clipboard_at_paste: Vec::new(),
                current_text: None,
            }
        }

        fn copy_happened(&mut self) {
            if let (Some(after), Some(pasted_at)) = (self.copy_after_paste, self.pasted_at)
                && self.elapsed >= pasted_at + after
            {
                self.copy_after_paste = None;
                self.stamp += 1;
                self.current_text = Some("copied meanwhile".into());
            }
        }
    }

    impl Desktop for FakeDesktop {
        type Snapshot = &'static str;

        fn may_send_keys(&self) -> bool {
            self.may_send_keys
        }

        fn modifiers_down(&self) -> bool {
            self.elapsed < self.modifiers_down_until
        }

        fn read_clipboard(&mut self) -> Previous<&'static str> {
            self.steps.push(Step::ReadClipboard);
            self.clipboard.clone()
        }

        fn write_text(&mut self, text: &str, transient: bool) -> Option<ChangeStamp> {
            self.steps.push(Step::WriteText {
                text: text.into(),
                transient,
            });
            self.stamp += 1;
            self.current_text = Some(text.into());
            Some(self.stamp)
        }

        fn write_snapshot(&mut self, snapshot: &'static str) -> Option<ChangeStamp> {
            self.steps.push(Step::WriteSnapshot(snapshot));
            self.stamp += 1;
            self.current_text = Some(snapshot.into());
            Some(self.stamp)
        }

        fn change_stamp(&self) -> ChangeStamp {
            self.stamp
        }

        fn send_paste(&mut self) -> bool {
            self.steps.push(Step::SendPaste);
            assert!(!self.modifiers_down(), "paste sent with a modifier down");
            self.clipboard_at_paste
                .push(self.current_text.clone().unwrap_or_default());
            self.pasted_at = Some(self.elapsed);
            self.paste_works
        }

        fn now(&self) -> Instant {
            self.start + self.elapsed
        }

        fn sleep(&mut self, duration: Duration) {
            self.elapsed += duration;
            self.copy_happened();
        }
    }

    fn written(text: &str, transient: bool) -> Step {
        Step::WriteText {
            text: text.into(),
            transient,
        }
    }

    #[test]
    fn the_text_is_pasted_and_the_previous_clipboard_restored_after_a_pause() {
        let mut desktop = FakeDesktop::new();

        let result = insert_with(
            &mut desktop,
            "Hei, dette er en prøve.",
            &InsertOptions::default(),
        );

        assert_eq!(result, Ok(Inserted::PreviousRestored));
        assert_eq!(
            desktop.steps,
            [
                Step::ReadClipboard,
                written("Hei, dette er en prøve. ", true),
                Step::SendPaste,
                Step::WriteSnapshot("previous"),
            ]
        );
        assert_eq!(desktop.clipboard_at_paste, ["Hei, dette er en prøve. "]);
        assert!(desktop.elapsed >= RESTORE_AFTER);
    }

    #[test]
    fn the_text_stays_on_the_clipboard_when_the_user_chose_to_copy_it() {
        let mut desktop = FakeDesktop::new();
        let options = InsertOptions {
            keep_text_on_clipboard: true,
            ..InsertOptions::default()
        };

        let result = insert_with(&mut desktop, "Hei", &options);

        assert_eq!(result, Ok(Inserted::TextKept));
        assert_eq!(desktop.steps, [written("Hei ", false), Step::SendPaste]);
        assert_eq!(desktop.elapsed, Duration::ZERO);
    }

    #[test]
    fn the_paste_waits_until_every_modifier_is_up() {
        let mut desktop = FakeDesktop::new();
        desktop.modifiers_down_until = Duration::from_millis(730);

        let result = insert_with(&mut desktop, "Hei", &InsertOptions::default());

        assert_eq!(result, Ok(Inserted::PreviousRestored));
        // The fake asserts on every paste that no modifier is down.
        assert_eq!(desktop.pasted_at, Some(Duration::from_millis(740)));
    }

    #[test]
    fn a_modifier_held_past_the_wait_leaves_the_clipboard_untouched() {
        let mut desktop = FakeDesktop::new();
        desktop.modifiers_down_until = Duration::from_secs(3600);
        let options = InsertOptions {
            keys_up_within: Duration::from_secs(2),
            ..InsertOptions::default()
        };

        let result = insert_with(&mut desktop, "Hei", &options);

        assert_eq!(result, Err(InsertError::KeysHeld));
        assert_eq!(desktop.steps, []);
        assert!(desktop.elapsed >= Duration::from_secs(2));
        assert!(desktop.elapsed < Duration::from_secs(3));
    }

    #[test]
    fn without_accessibility_nothing_is_touched() {
        let mut desktop = FakeDesktop::new();
        desktop.may_send_keys = false;

        let result = insert_with(&mut desktop, "Hei", &InsertOptions::default());

        assert_eq!(result, Err(InsertError::NoAccessibility));
        assert_eq!(desktop.steps, []);
    }

    #[test]
    fn a_copy_made_after_the_paste_is_not_overwritten() {
        let mut desktop = FakeDesktop::new();
        desktop.copy_after_paste = Some(Duration::from_millis(300));

        let result = insert_with(&mut desktop, "Hei", &InsertOptions::default());

        assert_eq!(result, Ok(Inserted::CopiedMeanwhile));
        assert_eq!(
            desktop.steps,
            [Step::ReadClipboard, written("Hei ", true), Step::SendPaste]
        );
        assert_eq!(desktop.current_text.as_deref(), Some("copied meanwhile"));
    }

    #[test]
    fn an_unreadable_clipboard_leaves_the_text_rather_than_clearing_it() {
        let mut desktop = FakeDesktop::new();
        desktop.clipboard = Previous::Unreadable;

        let result = insert_with(&mut desktop, "Hei", &InsertOptions::default());

        assert_eq!(result, Ok(Inserted::PreviousUnreadable));
        assert_eq!(
            desktop.steps,
            [Step::ReadClipboard, written("Hei ", false), Step::SendPaste]
        );
    }

    #[test]
    fn contents_marked_private_are_not_put_back() {
        let mut desktop = FakeDesktop::new();
        desktop.clipboard = Previous::MarkedPrivate;

        let result = insert_with(&mut desktop, "Hei", &InsertOptions::default());

        assert_eq!(result, Ok(Inserted::PreviousMarkedPrivate));
        assert_eq!(
            desktop.steps,
            [Step::ReadClipboard, written("Hei ", false), Step::SendPaste]
        );
        assert_eq!(desktop.current_text.as_deref(), Some("Hei "));
        assert_eq!(desktop.elapsed, Duration::ZERO);
    }

    #[test]
    fn a_failed_paste_leaves_the_text_when_the_previous_contents_were_marked_private() {
        let mut desktop = FakeDesktop::new();
        desktop.clipboard = Previous::MarkedPrivate;
        desktop.paste_works = false;

        let result = insert_with(&mut desktop, "Hei", &InsertOptions::default());

        assert_eq!(result, Err(InsertError::Paste));
        assert_eq!(
            desktop.steps,
            [Step::ReadClipboard, written("Hei ", false), Step::SendPaste]
        );
    }

    #[test]
    fn a_failed_paste_puts_the_previous_clipboard_back_at_once() {
        let mut desktop = FakeDesktop::new();
        desktop.paste_works = false;

        let result = insert_with(&mut desktop, "Hei", &InsertOptions::default());

        assert_eq!(result, Err(InsertError::Paste));
        assert_eq!(
            desktop.steps,
            [
                Step::ReadClipboard,
                written("Hei ", true),
                Step::SendPaste,
                Step::WriteSnapshot("previous"),
            ]
        );
        assert_eq!(desktop.elapsed, Duration::ZERO);
    }

    #[test]
    fn empty_text_touches_nothing() {
        for text in ["", "   ", "\n"] {
            let mut desktop = FakeDesktop::new();
            desktop.may_send_keys = false;

            assert_eq!(
                insert_with(&mut desktop, text, &InsertOptions::default()),
                Ok(Inserted::Nothing)
            );
            assert_eq!(desktop.steps, []);
        }
    }

    #[test]
    fn the_pasted_text_ends_in_exactly_one_space() {
        assert_eq!(pasted_text("Hei").as_deref(), Some("Hei "));
        assert_eq!(pasted_text("Hei.  ").as_deref(), Some("Hei. "));
        assert_eq!(pasted_text("Hei\n").as_deref(), Some("Hei "));
        assert_eq!(pasted_text(" Hei").as_deref(), Some(" Hei "));
        assert_eq!(pasted_text(" \t"), None);
    }
}
