//! What the app asks of the system on Windows, where macOS has its own modules: start at login, the
//! permissions the first-run window shows, and the console's Ctrl-C.

pub(crate) mod console;
pub(crate) mod login;
pub(crate) mod permissions;
