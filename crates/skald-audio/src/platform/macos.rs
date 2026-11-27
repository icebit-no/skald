use std::ffi::c_void;
use std::ptr::NonNull;
use std::time::Duration;

use objc2_core_audio::{
    AudioObjectAddPropertyListener, AudioObjectID, AudioObjectPropertyAddress,
    AudioObjectPropertySelector, AudioObjectRemovePropertyListener,
    kAudioHardwarePropertyDefaultInputDevice, kAudioHardwarePropertyDevices,
    kAudioObjectPropertyElementMain, kAudioObjectPropertyScopeGlobal, kAudioObjectSystemObject,
};

use crate::CaptureError;

type Callback = Box<dyn Fn() + Send + Sync>;

const WATCHED: [AudioObjectPropertySelector; 2] = [
    kAudioHardwarePropertyDevices,
    kAudioHardwarePropertyDefaultInputDevice,
];

const SYSTEM: AudioObjectID = kAudioObjectSystemObject as AudioObjectID;

/// Calls back on a Core Audio thread when a device is added or removed, or the default input
/// changes. Listening opens no device.
pub(crate) struct DeviceChanges {
    // Boxed twice so the listener holds a thin pointer that stays put until it is removed.
    callback: Box<Callback>,
    registered: Vec<AudioObjectPropertyAddress>,
}

impl DeviceChanges {
    pub(crate) const POLL: Option<Duration> = None;

    pub(crate) fn watch(
        on_change: impl Fn() + Send + Sync + 'static,
    ) -> Result<Self, CaptureError> {
        let mut changes = Self {
            callback: Box::new(Box::new(on_change)),
            registered: Vec::with_capacity(WATCHED.len()),
        };
        for selector in WATCHED {
            let address = AudioObjectPropertyAddress {
                mSelector: selector,
                mScope: kAudioObjectPropertyScopeGlobal,
                mElement: kAudioObjectPropertyElementMain,
            };
            // SAFETY: the address is only read during the call. The client data points at the
            // callback, which lives until drop removes the listener.
            let status = unsafe {
                AudioObjectAddPropertyListener(
                    SYSTEM,
                    NonNull::from(&address),
                    Some(changed),
                    changes.client_data(),
                )
            };
            if status != 0 {
                // Dropping `changes` removes the listeners registered so far.
                return Err(CaptureError::System(format!(
                    "cannot listen for microphone changes (OSStatus {status})"
                )));
            }
            changes.registered.push(address);
        }
        Ok(changes)
    }

    fn client_data(&self) -> *mut c_void {
        let callback: *const Callback = &*self.callback;
        callback.cast_mut().cast()
    }
}

impl Drop for DeviceChanges {
    fn drop(&mut self) {
        for address in &self.registered {
            // SAFETY: the same address, listener and client data as registered.
            unsafe {
                AudioObjectRemovePropertyListener(
                    SYSTEM,
                    NonNull::from(address),
                    Some(changed),
                    self.client_data(),
                );
            }
        }
    }
}

unsafe extern "C-unwind" fn changed(
    _object: AudioObjectID,
    _count: u32,
    _addresses: NonNull<AudioObjectPropertyAddress>,
    client_data: *mut c_void,
) -> i32 {
    // SAFETY: the client data is the callback registered with this listener, alive until it is
    // removed.
    let callback = unsafe { &*client_data.cast::<Callback>() };
    callback();
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listening_registers_and_removes_without_opening_a_device() {
        let changes = DeviceChanges::watch(|| {}).expect("Core Audio takes the listener");
        assert_eq!(changes.registered.len(), WATCHED.len());
        drop(changes);
    }
}
