//! Windows (the notification area) has no item yet; it will draw [`crate::TrayModel::menu`] here, behind
//! its own `cfg`.

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
