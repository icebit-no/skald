//! Insertion on a Wayland desktop: the text on the clipboard, Ctrl+V once the keys are up, and the previous
//! contents put back (docs/design.md, Insertion).

use super::desktop::WaylandDesktop;
use crate::insert::{InsertError, InsertOptions, Inserted, insert_with};

/// Inserts text where the cursor is. Each call blocks until the previous clipboard contents are back, so
/// insertions never overlap.
#[derive(Debug, Default)]
pub struct Inserter {
    desktop: WaylandDesktop,
}

impl Inserter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, text: &str, options: &InsertOptions) -> Result<Inserted, InsertError> {
        insert_with(&mut self.desktop, text, options)
    }
}
