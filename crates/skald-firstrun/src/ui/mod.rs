//! The window on GPUI: one view that draws the step chosen from the model it was last given.

mod controls;
mod view;

use std::rc::Rc;

use gpui_kit::{
    AnyWindowHandle, App, AppContext, Bounds, Entity, Global, KeyBinding, TitlebarOptions,
    WeakEntity, WindowBounds, WindowOptions, px, size,
};

use crate::action::Action;
use crate::model::{FirstRunModel, PermissionChecks};
use view::FirstRunView;

gpui_kit::actions!(skald_firstrun, [CloseWindow]);

type OnAction = Rc<dyn Fn(Action, &mut App)>;

/// The first-run window. The app keeps one, shows it when the model or a permission is missing, hands it
/// the model whenever the model changes, and receives an [`Action`] for each thing the user asks for.
pub struct FirstRunWindow {
    on_action: OnAction,
    checks: Rc<PermissionChecks>,
    open: Option<(AnyWindowHandle, WeakEntity<FirstRunView>)>,
}

struct KeysBound;

impl Global for KeysBound {}

const WINDOW_SIZE: (f32, f32) = (620.0, 620.0);
const MIN_SIZE: (f32, f32) = (540.0, 540.0);

impl FirstRunWindow {
    /// `on_action` runs once the window's own update is done, so it may hand the window a new model at
    /// once. Sets up gpui-kit through skald-ui the first time a window asks.
    pub fn new(
        checks: PermissionChecks,
        on_action: impl Fn(Action, &mut App) + 'static,
        cx: &mut App,
    ) -> Self {
        skald_ui::init(cx);
        if !cx.has_global::<KeysBound>() {
            cx.bind_keys([KeyBinding::new(
                "cmd-w",
                CloseWindow,
                Some(view::KEY_CONTEXT),
            )]);
            cx.set_global(KeysBound);
        }
        Self {
            on_action: Rc::new(on_action),
            checks: Rc::new(checks),
            open: None,
        }
    }

    /// Opens the window on the first step not done, or brings it to the front.
    pub fn show(&mut self, model: &FirstRunModel, cx: &mut App) {
        if self.update_view(model, cx) {
            if let Some((handle, _)) = &self.open {
                let _ = handle.update(cx, |_, window, _| window.activate_window());
            }
            cx.activate(true);
            return;
        }
        let options = WindowOptions {
            titlebar: Some(TitlebarOptions {
                title: Some("Kom i gang med Skald".into()),
                ..TitlebarOptions::default()
            }),
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                None,
                size(px(WINDOW_SIZE.0), px(WINDOW_SIZE.1)),
                cx,
            ))),
            window_min_size: Some(size(px(MIN_SIZE.0), px(MIN_SIZE.1))),
            is_minimizable: false,
            ..WindowOptions::default()
        };
        let on_action = self.on_action.clone();
        let checks = self.checks.clone();
        let model = model.clone();
        let opened = cx.open_window(options, move |window, cx| {
            cx.new(|cx| FirstRunView::new(model, checks, on_action, window, cx))
        });
        match opened {
            Ok(handle) => {
                let view = handle.update(cx, |_, _, cx| cx.entity().downgrade()).ok();
                if let Some(view) = view {
                    self.open = Some((handle.into(), view));
                }
                // Skald runs without a Dock icon, so its window comes forward only when asked.
                cx.activate(true);
            }
            Err(error) => eprintln!("skald-firstrun: the window did not open: {error}"),
        }
    }

    /// Shows `model` if the window is open.
    pub fn update(&mut self, model: &FirstRunModel, cx: &mut App) {
        self.update_view(model, cx);
    }

    /// Reads the permissions again now, as when the system's prompt was answered while the window kept
    /// the front.
    pub fn check_permissions(&mut self, cx: &mut App) {
        if let Some((handle, view)) = self.window() {
            let _ = handle.update(cx, |_, window, cx| {
                view.update(cx, |view, cx| view.check_permissions(window, cx))
            });
        }
    }

    /// Closes the window without an action, as when the app found the model and the permissions in place
    /// some other way.
    pub fn close(&mut self, cx: &mut App) {
        if let Some((handle, _)) = self.open.take() {
            let _ = handle.update(cx, |_, window, _| window.remove_window());
        }
    }

    pub fn is_open(&self) -> bool {
        self.window().is_some()
    }

    /// Hands the open window the model; `false` when no window is open.
    fn update_view(&mut self, model: &FirstRunModel, cx: &mut App) -> bool {
        let Some((handle, view)) = self.window() else {
            return false;
        };
        handle
            .update(cx, |_, window, cx| {
                view.update(cx, |view, cx| view.set_model(model.clone(), window, cx))
            })
            .is_ok()
    }

    fn window(&self) -> Option<(AnyWindowHandle, Entity<FirstRunView>)> {
        let (handle, view) = self.open.as_ref()?;
        Some((*handle, view.upgrade()?))
    }
}
