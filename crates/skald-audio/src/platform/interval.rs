use std::time::Duration;

use crate::CaptureError;

/// Sends no notices; the watcher lists the microphones again every [`Self::POLL`].
pub(crate) struct DeviceChanges;

impl DeviceChanges {
    pub(crate) const POLL: Option<Duration> = Some(Duration::from_secs(2));

    pub(crate) fn watch(
        _on_change: impl Fn() + Send + Sync + 'static,
    ) -> Result<Self, CaptureError> {
        Ok(Self)
    }
}
