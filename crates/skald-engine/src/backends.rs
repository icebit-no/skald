//! The compute backends transcribe.cpp computes on. On Windows they are modules loaded with the first
//! model: the Vulkan one is skipped when the machine has no Vulkan driver, and the CPU carries on.
//! Elsewhere they are built in, and loading them does nothing.

use std::sync::OnceLock;

use transcribe_cpp::DeviceType;

/// Loads the backend modules, once, before the first model; every later call gives the first answer,
/// since transcribe.cpp cannot load them again in the same process.
pub(crate) fn load() -> Result<(), String> {
    static LOADED: OnceLock<Result<(), String>> = OnceLock::new();
    LOADED
        .get_or_init(|| {
            transcribe_cpp::init_backends_default()
                .map_err(|error| format!("the compute backends did not load: {error}"))
        })
        .clone()
}

/// Whether there is a GPU to compute on: Metal on a Mac, a GPU with a Vulkan driver on Windows. Without
/// one the model runs on the CPU, several times slower.
pub fn gpu_available() -> bool {
    load().is_ok()
        && transcribe_cpp::devices()
            .iter()
            .any(|device| matches!(device.device_type, DeviceType::Gpu | DeviceType::Igpu))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_backends_load_and_the_cpu_is_among_the_devices() {
        assert_eq!(load(), Ok(()));
        assert_eq!(load(), Ok(()), "a second call gives the first answer");
        assert!(
            transcribe_cpp::devices()
                .iter()
                .any(|device| device.device_type == DeviceType::Cpu),
            "{:?}",
            transcribe_cpp::devices()
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn a_mac_has_its_gpu() {
        assert!(gpu_available());
    }
}
