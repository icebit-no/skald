//! What the listener asks of a permission to listen. Windows asks none: a low-level keyboard hook needs
//! no grant, so the listener may always listen and nothing changes while it runs.

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

pub(crate) fn accessibility_granted() -> bool {
    true
}
