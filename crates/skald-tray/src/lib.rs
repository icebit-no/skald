//! Skald's item in the menu bar (docs/design.md, Menu): ᚨ as a template icon, with a red dot while a
//! take records and a gold dot while it is transcribed, and the menu under it. The app hands the item a
//! [`TrayModel`] whenever its state changes and receives an [`Action`] for each entry chosen.
//!
//! What the menu holds is decided in [`menu`], as data with no platform in it, so it is tested on any
//! machine. The item that draws it is AppKit's `NSStatusItem` on macOS. Windows's notification area and
//! a StatusNotifierItem on Linux, which Waybar shows, come with those platforms (docs/plan.md); until
//! then [`Tray::new`] says [`TrayError::Unsupported`] there.
//!
//! # Wiring it into the app
//!
//! The item lives on the main thread, in the run loop that runs it, which is GPUI's once the app runs
//! GPUI. This crate does not depend on GPUI: it needs only to be made on the main thread after the
//! application has launched, and to be kept there. [`Tray`] is not `Send`, so the compiler holds it to
//! that thread. In the app:
//!
//! 1. In the closure GPUI's `Application::run` calls once the application has launched, make the
//!    item with the model as it stands and a handler that sends each action on a channel, such as
//!    `async_channel`, which GPUI already depends on.
//! 2. Keep the [`Tray`] in the entity that owns the app's state, and call [`Tray::update`] from that
//!    entity whenever the state changes. An unchanged model costs a comparison.
//! 3. Receive the actions in a task the entity spawns, and act on them there with GPUI's context.
//!
//! ```ignore
//! Application::new().run(move |cx| {
//!     let (actions, chosen) = async_channel::unbounded();
//!     let tray = skald_tray::Tray::new(&model, move |action| {
//!         let _ = actions.try_send(action);
//!     })?;
//!     let app = cx.new(|cx| {
//!         cx.spawn(async move |this, cx| {
//!             while let Ok(action) = chosen.recv().await {
//!                 if this.update(cx, |app: &mut App, cx| app.act(action, cx)).is_err() {
//!                     break;
//!                 }
//!             }
//!         })
//!         .detach();
//!         App { tray, .. }
//!     });
//! });
//! ```
//!
//! The handler runs from AppKit's dispatch of the chosen entry, outside any GPUI update, which is why
//! it passes the action on rather than acting on it.
//! On macOS the menu is built when it opens, from the model last given, so an update while it is open
//! shows the next time.

pub mod menu;
mod model;
mod platform;

use std::fmt;
use std::marker::PhantomData;

pub use model::{Action, Activity, Microphone, TrayModel};

/// The item in the menu bar, there until it is dropped.
pub struct Tray {
    item: platform::StatusItem,
    /// Holds the item to the thread that made it on every platform.
    _main_thread: PhantomData<*const ()>,
}

impl Tray {
    /// Puts the item in the menu bar showing `model`; each entry chosen calls `on_action` on the main
    /// thread. Call it on the main thread once the application has launched.
    pub fn new(model: &TrayModel, on_action: impl Fn(Action) + 'static) -> Result<Self, TrayError> {
        Ok(Self {
            item: platform::StatusItem::new(model, Box::new(on_action))?,
            _main_thread: PhantomData,
        })
    }

    /// Shows `model`. Only what differs from the model shown is touched.
    pub fn update(&mut self, model: &TrayModel) {
        self.item.show(model);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayError {
    /// The item is made on the main thread only, where the application's run loop runs.
    NotMainThread,
    /// No menu bar item on this platform yet.
    Unsupported,
}

impl fmt::Display for TrayError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::NotMainThread => "the menu bar item can only be made on the main thread",
            Self::Unsupported => "there is no menu bar item on this platform yet",
        })
    }
}

impl std::error::Error for TrayError {}
