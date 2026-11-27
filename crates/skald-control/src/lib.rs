//! The `skald` command line and the local socket it talks to the running Skald over (docs/design.md,
//! Command line).
//!
//! - **The app** binds a [`Server`] at [`Paths::socket`](skald_core::paths::Paths::socket) and serves it
//!   with its own [`Skald`]. Only the user can open the socket.
//! - **The binary** calls [`run`] when it is given a command. The command goes over the socket when
//!   Skald is running, so Skald applies a change at once; otherwise it is carried out here, on the
//!   configuration file and the state file, with the binary's [`Machine`] for what the files cannot say.
//!
//! Either way the command is carried out by the same code on top of [`Skald`], so it answers the same,
//! and `--json` prints the same object, whether Skald runs or not.

mod cli;
mod commands;
mod offline;
pub mod protocol;
mod skald;
#[cfg(unix)]
mod socket;

pub use cli::{run, run_with};
pub use protocol::{Error, ErrorCode, Request};
pub use skald::{Machine, Microphones, Model, Skald, Transcribed};
#[cfg(unix)]
pub use socket::{BindError, Client, Server, Serving};
