//! The item each platform draws from the same model and menu entries.

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub(crate) use macos::StatusItem;

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub(crate) use windows::StatusItem;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub(crate) use linux::StatusItem;

#[cfg(not(any(target_os = "macos", windows, target_os = "linux")))]
mod unsupported;
#[cfg(not(any(target_os = "macos", windows, target_os = "linux")))]
pub(crate) use unsupported::StatusItem;
