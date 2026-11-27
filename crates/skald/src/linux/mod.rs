//! What the app asks of the system on Linux, where macOS has its own modules: start at login, the
//! permissions the first-run window shows, and the start and end sounds.

pub(crate) mod login;
pub(crate) mod permissions;
pub(crate) mod sounds;
