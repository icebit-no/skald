//! The first-run window: download the model, grant the permissions, try a dictation (docs/design.md, First
//! run).
//!
//! The window is a view over a [`FirstRunModel`] the app hands it, and each thing the user asks for goes
//! back to the app as an [`Action`]. It touches no file and no network: the app runs the download, asks
//! the system for the permissions, opens its settings and runs the test take, and hands the window the
//! model again whenever any of it moves on. The permissions the window reads itself, through the
//! [`PermissionChecks`] the app passes in, when it opens and each time it comes to the front. What each
//! step shows is decided in [`download`], [`permissions`], [`test_step`] and [`steps`], as data with no
//! window in it, so it is tested without a screen. The window draws them with GPUI in skald-ui's colours,
//! following the system's light or dark mode.
//!
//! # Wiring it into the app
//!
//! ```ignore
//! let card = ModelCard::of(skald_models::find(&config.model)?)?;
//! let mut model = FirstRunModel::new(card, skald_model.status(&models_dir)?);
//! model.hold_key = config.hold_key.clone();
//! let checks = PermissionChecks {
//!     microphone: Box::new(|| /* skald_audio::microphone_access(), value for value */),
//!     accessibility: Box::new(|| /* skald-input's Accessibility check */),
//! };
//! let mut window = FirstRunWindow::new(checks, move |action, cx| app.first_run(action, cx), cx);
//! window.show(&model, cx);
//!
//! // Action::StartDownload: model.download.start(card.size), then on the download's thread
//! // Downloader::download(…, |event| send(event)), and on the main thread for each event:
//! model.download.event(event, Instant::now());
//! window.update(&model, cx);
//! // and when it returns:
//! model.download.finish(&result);
//! window.update(&model, cx);
//! // Action::CancelDownload: cancel.cancel(); model.download.stopping(); window.update(&model, cx).
//! ```
//!
//! The test take is inserted where the cursor is, as every take is: into the window's text field, which
//! has the focus. The window reads that paste on the main thread while the insertion waits to put the
//! clipboard back, so the insertion has to run on another thread. [`Action::Done`] closes the window;
//! [`Action::Closed`] says the user closed it before they were done.

mod action;
mod button;
pub mod download;
mod format;
mod model;
pub mod permissions;
pub mod steps;
pub mod test_step;
mod ui;

pub use action::{Action, Permission};
pub use button::ButtonSpec;
pub use download::{Download, DownloadError, Retry, Throughput};
pub use model::{
    Access, FirstRunModel, ModelCard, NO_VULKAN, PermissionChecks, Permissions, Platform,
    TestDictation,
};
pub use steps::Step;
pub use ui::FirstRunWindow;
