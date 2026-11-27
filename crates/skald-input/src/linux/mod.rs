//! Linux: the hold key from the desktop's key binding, insertion through Wayland's clipboard and a virtual
//! keyboard, and no permission to ask for.

pub(crate) mod access;
mod clipboard;
mod desktop;
pub(crate) mod insert;
pub(crate) mod listener;
mod portal;
