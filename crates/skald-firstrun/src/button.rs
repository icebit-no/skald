//! A button as a step describes it: its words, what it asks of the app, and its weight.

use crate::action::Action;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ButtonSpec {
    pub label: &'static str,
    pub action: Action,
    /// The one thing the step is waiting for: gold.
    pub primary: bool,
}

impl ButtonSpec {
    pub fn primary(label: &'static str, action: Action) -> Self {
        Self {
            label,
            action,
            primary: true,
        }
    }

    pub fn secondary(label: &'static str, action: Action) -> Self {
        Self {
            label,
            action,
            primary: false,
        }
    }
}
