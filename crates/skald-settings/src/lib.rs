//! The settings window, with the tabs Generelt, Ordskatt, Historikk and Om (docs/design.md, Settings).
//!
//! The window is a view over a [`SettingsModel`] the app hands it: the configuration, the state, the
//! microphones connected, what of each model is downloaded, a token counter and the version. It keeps no
//! setting of its own and touches no file. Each change the user makes goes to the app as an [`Action`];
//! a [`Change`] is applied with [`Change::apply`], after which the app writes the configuration file and
//! hands the window the model again. Two things the window does itself, since they need nothing of the
//! app: «Kopier» puts a text on the clipboard, and a link opens in the browser.
//!
//! What each tab shows and what an edit sends is decided in [`general`], [`vocabulary`], [`history`] and
//! [`about`], as data with no window in it, so it is tested without a screen. The window draws them with
//! GPUI and gpui-kit's headless controls, in the identity's colours, following the system's light or dark
//! mode.
//!
//! # Wiring it into the app
//!
//! ```ignore
//! let settings = SettingsWindow::new(move |action, cx| app.act(action, cx), cx);
//! // «Innstillinger …» and «Ordskatt …» in the menu:
//! settings.show(&model, None, cx);
//! settings.show(&model, Some(Tab::Vocabulary), cx);
//! // Whenever the configuration, the state, the microphones or a download changes:
//! settings.update(&model, cx);
//! ```
//!
//! [`SettingsWindow::new`] sets gpui-kit up through skald-ui the first time a window asks, which binds the
//! text fields' keys for the whole app. The window shows a change at once, and the next model the app
//! hands it is the truth.

pub mod about;
mod action;
pub mod general;
pub mod history;
mod model;
mod ui;
pub mod vocabulary;
mod words;

pub use action::{Action, Change};
pub use model::{Compute, Microphones, ModelState, SettingsModel, TokenCounter};
pub use ui::{SettingsWindow, Tab};
