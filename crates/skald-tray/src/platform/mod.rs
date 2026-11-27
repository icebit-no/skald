//! The item each platform draws from the same model and menu entries.

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub(crate) use macos::StatusItem;

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub(crate) use windows::StatusItem;

#[cfg(not(any(target_os = "macos", windows)))]
mod unsupported;
#[cfg(not(any(target_os = "macos", windows)))]
pub(crate) use unsupported::StatusItem;
