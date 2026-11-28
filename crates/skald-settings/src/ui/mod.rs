//! The window on GPUI: one view that draws the tab chosen from the model it was last given.

mod about;
mod controls;
mod general;
mod history;
mod view;
mod vocabulary;

use std::rc::Rc;

use gpui_kit::{
    AnyWindowHandle, App, AppContext, Bounds, Global, KeyBinding, TitlebarOptions, WeakEntity,
    WindowBounds, WindowOptions, px, size,
};

use crate::action::Action;
use crate::model::SettingsModel;
use view::SettingsView;

gpui_kit::actions!(skald_settings, [CloseWindow]);

type OnAction = Rc<dyn Fn(Action, &mut App)>;

/// The four tabs, in the order they stand.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Tab {
    #[default]
    General,
    /// Ordskatt, which the menu's «Ordskatt …» opens.
    Vocabulary,
    History,
    About,
}

impl Tab {
    pub const ALL: [Tab; 4] = [Tab::General, Tab::Vocabulary, Tab::History, Tab::About];

    pub fn label(self) -> &'static str {
        match self {
            Tab::General => "Generelt",
            Tab::Vocabulary => "Ordskatt",
            Tab::History => "Historikk",
            Tab::About => "Om",
        }
    }
}

/// The settings window, opened on demand and closed by the user. The app keeps one, shows it from the
/// menu's «Innstillinger …» and «Ordskatt …», hands it the model whenever the model changes, and receives
/// an [`Action`] for each change the user makes.
pub struct SettingsWindow {
    on_action: OnAction,
    open: Option<(AnyWindowHandle, WeakEntity<SettingsView>)>,
}

struct KeysBound;

impl Global for KeysBound {}

const WINDOW_SIZE: (f32, f32) = (640.0, 640.0);
const MIN_SIZE: (f32, f32) = (520.0, 420.0);

impl SettingsWindow {
    /// `on_action` runs once the window's own update is done, so it may hand the window a new model
    /// at once. Sets up gpui-kit's controls and their keys the first time.
    pub fn new(on_action: impl Fn(Action, &mut App) + 'static, cx: &mut App) -> Self {
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
            open: None,
        }
    }

    /// Opens the window on `tab`, or brings it to the front, on `tab` when one is given.
    pub fn show(&mut self, model: &SettingsModel, tab: Option<Tab>, cx: &mut App) {
        if let Some((handle, view)) = &self.open {
            let raised = handle
                .update(cx, |_, window, cx| skald_ui::bring_to_front(window, cx))
                .is_ok();
            if raised && let Some(view) = view.upgrade() {
                view.update(cx, |view, cx| view.show(model.clone(), tab, cx));
                return;
            }
        }
        let options = WindowOptions {
            titlebar: Some(TitlebarOptions {
                title: Some("Innstillinger".into()),
                ..TitlebarOptions::default()
            }),
            // The name a Wayland compositor knows Skald's windows by, as in a Hyprland window rule.
            app_id: Some(skald_ui::APP_ID.into()),
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                None,
                size(px(WINDOW_SIZE.0), px(WINDOW_SIZE.1)),
                cx,
            ))),
            window_min_size: Some(size(px(MIN_SIZE.0), px(MIN_SIZE.1))),
            ..WindowOptions::default()
        };
        let on_action = self.on_action.clone();
        let model = model.clone();
        let opened = gpui_kit::open_window(options, cx, move |window, cx| {
            cx.new(|cx| SettingsView::new(model, tab.unwrap_or_default(), on_action, window, cx))
        });
        match opened {
            Ok((handle, view)) => {
                // Skald runs without a Dock icon, so its window comes forward only when asked.
                let _ = handle.update(cx, |_, window, cx| skald_ui::bring_to_front(window, cx));
                self.open = Some((handle, view.downgrade()));
            }
            Err(error) => eprintln!("skald-settings: the window did not open: {error}"),
        }
    }

    /// Shows `model` if the window is open; a closed window is drawn from the model given to
    /// [`show`](Self::show).
    pub fn update(&mut self, model: &SettingsModel, cx: &mut App) {
        if let Some((_, view)) = &self.open
            && let Some(view) = view.upgrade()
        {
            view.update(cx, |view, cx| view.set_model(model.clone(), cx));
        }
    }
}
