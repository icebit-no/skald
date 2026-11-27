//! Insertion on Linux, which Skald does not do: every text is refused as a paste that could not be sent,
//! so the app reports it as not inserted and keeps it in the history.

use crate::insert::{InsertError, InsertOptions, Inserted, pasted_text};

#[derive(Debug, Default)]
pub struct Inserter;

impl Inserter {
    pub fn new() -> Self {
        Self
    }

    pub fn insert(
        &mut self,
        text: &str,
        _options: &InsertOptions,
    ) -> Result<Inserted, InsertError> {
        match pasted_text(text) {
            None => Ok(Inserted::Nothing),
            Some(_) => Err(InsertError::Paste),
        }
    }
}
