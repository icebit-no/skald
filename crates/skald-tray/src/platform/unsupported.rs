//! The other systems have no item; [`crate::Tray::new`] says so.

use std::convert::Infallible;

use crate::{Action, TrayError, TrayModel};

pub(crate) struct StatusItem {
    never: Infallible,
}

impl StatusItem {
    pub(crate) fn new(
        _model: &TrayModel,
        _on_action: Box<dyn Fn(Action)>,
    ) -> Result<Self, TrayError> {
        Err(TrayError::Unsupported)
    }

    pub(crate) fn show(&mut self, _model: &TrayModel) {
        match self.never {}
    }
}
