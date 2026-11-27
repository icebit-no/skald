//! Notice that the system's microphones changed.
//!
//! macOS tells the watcher when a device comes, goes or becomes the default input. Windows and Linux
//! are asked again at an interval until they get notifications of their own in M4 and M5.

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub(crate) use macos::DeviceChanges;

#[cfg(not(target_os = "macos"))]
mod interval;
#[cfg(not(target_os = "macos"))]
pub(crate) use interval::DeviceChanges;
