//! The permission macOS calls Accessibility, which Windows does not have: a low-level keyboard hook and
//! the paste keystroke need no grant, so it always reads as granted, nothing changes while the listener
//! runs, and there is nothing to ask for or open.

#[derive(Debug)]
pub(crate) struct AccessWatch;

impl AccessWatch {
    pub(crate) fn new() -> Self {
        Self
    }

    pub(crate) fn changed(&self) -> bool {
        false
    }
}

pub fn accessibility_granted() -> bool {
    true
}

pub fn request_accessibility() -> bool {
    true
}

pub fn open_accessibility_settings() -> std::io::Result<()> {
    Ok(())
}
