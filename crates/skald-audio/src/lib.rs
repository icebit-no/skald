//! Microphone capture: the device chosen by the priority list, resampled to 16 kHz mono, with level
//! readings for the meter.

mod capture;
pub mod filter;
mod level;
mod microphone;
mod platform;
mod resample;
mod take;
mod watch;

pub use capture::{CaptureError, Take, connected_microphones};
pub use level::{LEVEL_FLOOR_DBFS, LEVEL_INTERVAL};
pub use microphone::{Connected, MicId, Microphone, MicrophoneChange, choose};
pub use take::{Ending, Recording, TAKE_CAP, TakeEvent};
pub use watch::MicrophoneWatcher;

/// The rate of every take's samples, the rate the models take.
pub const SAMPLE_RATE: u32 = 16_000;
