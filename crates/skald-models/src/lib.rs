//! The speech models Skald offers and getting one onto the machine: the registry with each model's licence and
//! credit, the download of a pinned file with progress, resume and a SHA-256 check, and what of each model is
//! in the models folder.
//!
//! The models folder holds a folder per model, named by its id, with the model's file in it
//! (`nb-whisper-large/ggml-model.bin`), and Silero VAD's file at its top.
//!
//! The network is used only to fetch a model's file from its pinned URL (CLAUDE.md); nothing else is
//! contacted.

mod download;
mod free_space;
mod registry;
mod status;

use std::path::{Path, PathBuf};

pub use download::{Cancel, Downloader, Error, Event, verify};
pub use registry::{Availability, MODELS, Model, ModelFile, find};
pub use status::{Status, status};

/// Silero VAD's file at the top of the models folder. It ships with the app and is never downloaded
/// (docs/design.md, First run).
pub const VAD_FILE: &str = "silero-vad-v6.2-F32.gguf";

pub fn vad_path(models_dir: &Path) -> PathBuf {
    models_dir.join(VAD_FILE)
}
