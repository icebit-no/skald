//! The window's one view: the tab bar, and the tab it shows drawn from the model.

use std::time::{SystemTime, UNIX_EPOCH};

use gpui_kit::base::Button;
use gpui_kit::base::input::{InputEvent, InputState};
use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, App, ClickEvent, Context, Entity, FocusHandle, Focusable, FontWeight, Render,
    SharedString, Subscription, Window, div, px,
};

use super::controls::{self, OnClick, TEXT_SIZE};
use super::palette::Palette;
use super::{CloseWindow, OnAction, Tab};
use crate::action::{Action, Change};
use crate::history::{HistoryTab, Pick};
use crate::model::SettingsModel;
use crate::vocabulary::VocabularyTab;

pub(super) const KEY_CONTEXT: &str = "SkaldSettings";

/// Which correction the two fields under the list are editing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Editing {
    New,
    Rule(usize),
}

pub(crate) struct SettingsView {
    pub(super) model: SettingsModel,
    pub(super) tab: Tab,
    on_action: OnAction,
    focus: FocusHandle,
    styled_dark: Option<bool>,

    pub(super) sentence: Entity<InputState>,
    pub(super) new_word: Entity<InputState>,
    pub(super) correction_from: Entity<InputState>,
    pub(super) correction_to: Entity<InputState>,
    pub(super) editing: Option<Editing>,
    pub(super) try_text: Entity<InputState>,
    pub(super) vocabulary_message: Option<String>,

    pub(super) pick: Option<Pick>,
    pub(super) replacement: Entity<InputState>,
    pub(super) correct_next_time: bool,
    pub(super) add_to_vocabulary: bool,
    pub(super) history_message: Option<String>,
    /// The entry last copied, by when it ended, so its button says so.
    pub(super) copied: Option<i64>,

    _subscriptions: Vec<Subscription>,
}

impl SettingsView {
    pub(super) fn new(
        model: SettingsModel,
        tab: Tab,
        on_action: OnAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let input = |placeholder: &'static str, window: &mut Window, cx: &mut Context<Self>| {
            cx.new(|cx| InputState::new(window, cx).placeholder(placeholder))
        };
        let sentence = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Vi merget pull-requesten i går.")
                .default_value(model.config.vocabulary.sentence.clone())
        });
        let new_word = input("Et ord eller navn", window, cx);
        let correction_from = input("Slik modellen skriver det", window, cx);
        let correction_to = input("Slik det skal stå", window, cx);
        let try_text = input("Skriv eller lim inn en tekst", window, cx);
        let replacement = input("Riktig ord", window, cx);

        let subscriptions = vec![
            cx.subscribe_in(&sentence, window, Self::on_sentence),
            cx.subscribe_in(&new_word, window, Self::on_new_word),
            cx.subscribe_in(&correction_to, window, Self::on_correction_to),
            cx.subscribe_in(&try_text, window, |_, _, event, _, cx| {
                if matches!(event, InputEvent::Change) {
                    cx.notify();
                }
            }),
            cx.subscribe_in(&replacement, window, Self::on_replacement),
            cx.observe_window_appearance(window, |_, _, cx| cx.notify()),
        ];

        // The window's keys, ⌘W among them, reach the view only once something in it has the focus.
        let focus = cx.focus_handle();
        window.focus(&focus, cx);

        Self {
            model,
            tab,
            on_action,
            focus,
            styled_dark: None,
            sentence,
            new_word,
            correction_from,
            correction_to,
            editing: None,
            try_text,
            vocabulary_message: None,
            pick: None,
            replacement,
            correct_next_time: true,
            add_to_vocabulary: false,
            history_message: None,
            copied: None,
            _subscriptions: subscriptions,
        }
    }

    pub(super) fn show(&mut self, model: SettingsModel, tab: Option<Tab>, cx: &mut Context<Self>) {
        if let Some(tab) = tab {
            self.tab = tab;
        }
        self.set_model(model, cx);
    }

    pub(super) fn set_model(&mut self, model: SettingsModel, cx: &mut Context<Self>) {
        // A pick names an entry by its place; a new take moves every entry down.
        if self.model.state.history != model.state.history {
            self.pick = None;
        }
        self.model = model;
        cx.notify();
    }

    /// Sends the action to the app once this update is done. A change shows at once; the model the app
    /// hands back is the truth from then on.
    pub(super) fn send(&mut self, action: Action, cx: &mut Context<Self>) {
        if let Action::Change(change) = &action {
            change.apply(&mut self.model.config);
        }
        let on_action = self.on_action.clone();
        cx.defer(move |cx| on_action(action, cx));
        cx.notify();
    }

    pub(super) fn change(&mut self, change: Option<Change>, cx: &mut Context<Self>) {
        if let Some(change) = change {
            self.send(Action::Change(change), cx);
        }
    }

    /// A click handler that runs `act` on the view.
    pub(super) fn on_click(
        &self,
        cx: &Context<Self>,
        act: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) -> OnClick {
        Box::new(cx.listener(move |view, _: &ClickEvent, window, cx| act(view, window, cx)))
    }

    pub(super) fn value(input: &Entity<InputState>, cx: &App) -> String {
        input.read(cx).value().to_string()
    }

    pub(super) fn set_value(
        input: &Entity<InputState>,
        value: impl Into<SharedString>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let value = value.into();
        input.update(cx, |input, cx| input.set_value(value, window, cx));
    }

    pub(super) fn now_ms() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |since| since.as_millis().try_into().unwrap_or(i64::MAX))
    }

    fn on_sentence(
        &mut self,
        input: &Entity<InputState>,
        event: &InputEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if matches!(event, InputEvent::PressEnter { .. } | InputEvent::Blur) {
            let typed = Self::value(input, cx);
            let change = VocabularyTab::of(&self.model).set_sentence(&typed);
            self.change(change, cx);
        }
    }

    fn on_new_word(
        &mut self,
        _: &Entity<InputState>,
        event: &InputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if matches!(event, InputEvent::PressEnter { .. }) {
            self.add_word(window, cx);
        }
    }

    pub(super) fn add_word(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let typed = Self::value(&self.new_word, cx);
        match VocabularyTab::of(&self.model).add_word(&typed) {
            Ok(change) => {
                self.vocabulary_message = None;
                Self::set_value(&self.new_word, "", window, cx);
                self.change(change, cx);
            }
            Err(message) => {
                self.vocabulary_message = Some(message);
                cx.notify();
            }
        }
    }

    fn on_correction_to(
        &mut self,
        _: &Entity<InputState>,
        event: &InputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if matches!(event, InputEvent::PressEnter { .. }) {
            self.save_correction(window, cx);
        }
    }

    pub(super) fn edit_correction(
        &mut self,
        editing: Editing,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (from, to) = match editing {
            Editing::New => (String::new(), String::new()),
            Editing::Rule(index) => VocabularyTab::of(&self.model)
                .corrections
                .get(index)
                .map(|row| (row.from.clone(), row.to.clone()))
                .unwrap_or_default(),
        };
        Self::set_value(&self.correction_from, from, window, cx);
        Self::set_value(&self.correction_to, to, window, cx);
        self.editing = Some(editing);
        self.vocabulary_message = None;
        self.correction_from
            .update(cx, |input, cx| input.focus(window, cx));
        cx.notify();
    }

    pub(super) fn save_correction(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(editing) = self.editing else {
            return;
        };
        let from = Self::value(&self.correction_from, cx);
        let to = Self::value(&self.correction_to, cx);
        let tab = VocabularyTab::of(&self.model);
        let result = match editing {
            Editing::New => tab.add_correction(&from, &to).map(Some),
            Editing::Rule(index) => tab.edit_correction(index, &from, &to),
        };
        match result {
            Ok(change) => {
                self.editing = None;
                self.vocabulary_message = None;
                Self::set_value(&self.correction_from, "", window, cx);
                Self::set_value(&self.correction_to, "", window, cx);
                self.change(change, cx);
            }
            Err(message) => {
                self.vocabulary_message = Some(message);
            }
        }
        cx.notify();
    }

    fn on_replacement(
        &mut self,
        _: &Entity<InputState>,
        event: &InputEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            InputEvent::PressEnter { .. } => self.fix(cx),
            InputEvent::Change => cx.notify(),
            _ => {}
        }
    }

    pub(super) fn pick_word(
        &mut self,
        entry: usize,
        piece: usize,
        extend: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let tab = HistoryTab::of(&self.model, Self::now_ms());
        let extend = if extend { self.pick } else { None };
        let Some(pick) = tab.pick(entry, piece, extend) else {
            return;
        };
        let picked = tab.picked(pick).unwrap_or_default();
        self.pick = Some(pick);
        self.history_message = None;
        Self::set_value(&self.replacement, picked, window, cx);
        self.replacement.update(cx, |input, cx| {
            input.focus(window, cx);
            input.select_all(window, cx);
        });
        cx.notify();
    }

    pub(super) fn fix(&mut self, cx: &mut Context<Self>) {
        let Some(pick) = self.pick else {
            return;
        };
        let replacement = Self::value(&self.replacement, cx);
        let tab = HistoryTab::of(&self.model, Self::now_ms());
        match tab.fix(
            pick,
            &replacement,
            self.correct_next_time,
            self.add_to_vocabulary,
        ) {
            Ok(actions) => {
                self.pick = None;
                self.history_message = None;
                for action in actions {
                    self.send(action, cx);
                }
            }
            Err(message) => self.history_message = Some(message),
        }
        cx.notify();
    }

    /// Gives every field the colours of the system's appearance, once each time it changes.
    fn style_inputs(&mut self, palette: &Palette, cx: &mut Context<Self>) {
        if self.styled_dark == Some(palette.dark) {
            return;
        }
        self.styled_dark = Some(palette.dark);
        let style = controls::input_style(palette);
        for input in [
            &self.sentence,
            &self.new_word,
            &self.correction_from,
            &self.correction_to,
            &self.try_text,
            &self.replacement,
        ] {
            input.update(cx, |input, _| input.set_editor_style(style.clone()));
        }
    }

    fn tab_bar(&self, palette: &Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let p = *palette;
        div()
            .flex_none()
            .flex()
            .justify_center()
            .gap(px(4.0))
            .px(px(16.0))
            .pt(px(10.0))
            .border_b_1()
            .border_color(p.line)
            .bg(p.surface)
            .children(Tab::ALL.into_iter().map(|tab| {
                let selected = tab == self.tab;
                let on_click = self.on_click(cx, move |view, _, cx| {
                    view.tab = tab;
                    cx.notify();
                });
                Button::new(("tab", tab as usize))
                    .selected(selected)
                    .role(gpui_kit::Role::Tab)
                    .px(px(14.0))
                    .pb(px(9.0))
                    .pt(px(4.0))
                    .text_size(px(TEXT_SIZE))
                    .border_b_2()
                    .cursor_pointer()
                    .child(tab.label())
                    .on_click(move |event, window, cx| on_click(event, window, cx))
                    .map(|button| {
                        if selected {
                            button
                                .border_color(p.gold)
                                .text_color(p.text)
                                .font_weight(FontWeight::SEMIBOLD)
                        } else {
                            button
                                .border_color(gpui_kit::transparent_black())
                                .text_color(p.quiet)
                                .hover(move |style| style.text_color(p.text))
                        }
                    })
            }))
    }
}

impl Focusable for SettingsView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for SettingsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = Palette::of(window.appearance());
        self.style_inputs(&palette, cx);
        let content: AnyElement = match self.tab {
            Tab::General => self.render_general(&palette, cx),
            Tab::Vocabulary => self.render_vocabulary(&palette, window, cx),
            Tab::History => self.render_history(&palette, window, cx),
            Tab::About => self.render_about(&palette, cx),
        };
        div()
            .key_context(KEY_CONTEXT)
            .track_focus(&self.focus)
            .on_action(|_: &CloseWindow, window, _| window.remove_window())
            .size_full()
            .flex()
            .flex_col()
            .bg(palette.ground)
            .text_color(palette.text)
            .text_size(px(TEXT_SIZE))
            .font_family(".SystemUIFont")
            .child(self.tab_bar(&palette, cx))
            .child(
                div()
                    .id("tab-content")
                    .flex_1()
                    .min_h(px(0.0))
                    .overflow_y_scroll()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(22.0))
                            .px(px(28.0))
                            .py(px(22.0))
                            .child(content),
                    ),
            )
    }
}
