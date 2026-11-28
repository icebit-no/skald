//! The window's one view: the step bar, the step shown, and the buttons that move between steps.

use std::rc::Rc;
use std::time::Duration;

use gpui_kit::base::Button;
use gpui_kit::base::input::{InputEvent, InputState};
use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, ClickEvent, Context, Div, Entity, FocusHandle, FontWeight, Render, Subscription,
    Task, Window, div, px,
};
use skald_ui::Palette;

use super::controls::{self, OnClick, SMALL_SIZE, TEXT_SIZE};
use super::{CloseWindow, OnAction};
use crate::action::{Action, Permission};
use crate::download::DownloadView;
use crate::format;
use crate::model::{FirstRunModel, PermissionChecks, Permissions, Platform, TestDictation};
use crate::permissions::{self, PermissionRow};
use crate::steps::{Footer, Step};
use crate::test_step::{FIELD_PLACEHOLDER, TestView};

pub(super) const KEY_CONTEXT: &str = "SkaldFirstRun";

/// How often the permissions are read while one is missing. A grant is made in System Settings or the
/// system's prompt, which keep the front, so the window would not see it until it came back to the front.
const PERMISSIONS_EVERY: Duration = Duration::from_secs(1);

pub(crate) struct FirstRunView {
    model: FirstRunModel,
    step: Step,
    on_action: OnAction,
    checks: Rc<PermissionChecks>,
    permissions: Permissions,
    /// The system's prompt for Accessibility shows once; after it, only System Settings can grant it.
    asked_for_accessibility: bool,
    /// Where the test take is inserted.
    field: Entity<InputState>,
    styled_dark: Option<bool>,
    focus: FocusHandle,
    _subscriptions: Vec<Subscription>,
    _watching_permissions: Task<()>,
}

impl FirstRunView {
    pub(super) fn new(
        model: FirstRunModel,
        checks: Rc<PermissionChecks>,
        on_action: OnAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let field = cx.new(|cx| InputState::new(window, cx).placeholder(FIELD_PLACEHOLDER));
        let subscriptions = vec![
            cx.observe_window_appearance(window, |_, _, cx| cx.notify()),
            cx.observe_window_activation(window, |view, window, cx| {
                if window.is_window_active() {
                    view.check_permissions(window, cx);
                }
            }),
            cx.subscribe_in(&field, window, |_, _, event, _, cx| {
                if matches!(event, InputEvent::Change) {
                    cx.notify();
                }
            }),
        ];
        let closing = on_action.clone();
        window.on_window_should_close(cx, move |_, cx| {
            let closing = closing.clone();
            cx.defer(move |cx| closing(Action::Closed, cx));
            true
        });

        // ⌘W reaches the view only once something in it has the focus.
        let focus = cx.focus_handle();
        window.focus(&focus, cx);

        let watching_permissions = cx.spawn_in(window, async move |view, cx| {
            loop {
                cx.background_executor().timer(PERMISSIONS_EVERY).await;
                let watched = view.update_in(cx, |view, window, cx| {
                    view.watch_permissions(window, cx);
                });
                if watched.is_err() {
                    break;
                }
            }
        });

        let permissions = checks.read(model.platform, false);
        let mut view = Self {
            step: Step::first_open(&model, &permissions),
            model,
            on_action,
            checks,
            permissions,
            asked_for_accessibility: false,
            field,
            styled_dark: None,
            focus,
            _subscriptions: subscriptions,
            _watching_permissions: watching_permissions,
        };
        view.focus_field_for_test(window, cx);
        view
    }

    pub(super) fn set_model(
        &mut self,
        model: FirstRunModel,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let before = std::mem::replace(&mut self.model, model);
        if before.test != self.model.test {
            match self.model.test {
                // Each take is inserted into an empty field, so what it shows is that take's text.
                TestDictation::Recording => {
                    self.field
                        .update(cx, |field, cx| field.set_value("", window, cx));
                }
                TestDictation::Listening => self.focus_field_for_test(window, cx),
                _ => {}
            }
        }
        cx.notify();
    }

    /// Reads the permissions again; true when one is granted that was not before.
    pub(super) fn check_permissions(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let permissions = self
            .checks
            .read(self.model.platform, self.asked_for_accessibility);
        let newly_granted = permissions.newly_granted(&self.permissions);
        if permissions != self.permissions {
            self.permissions = permissions;
            self.focus_field_for_test(window, cx);
            cx.notify();
        }
        newly_granted
    }

    /// While a permission is missing, reads them again, and brings the window back to the front once one
    /// is granted, from behind System Settings or wherever the system's prompt left it.
    fn watch_permissions(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.permissions.granted(self.model.platform) {
            return;
        }
        if self.check_permissions(window, cx) {
            skald_ui::bring_to_front(window, cx);
        }
    }

    /// Gives the test's field the focus while the test step shows and can run, so the take is inserted
    /// there.
    fn focus_field_for_test(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.step == Step::Test && self.model.ready(&self.permissions) {
            self.field.update(cx, |field, cx| field.focus(window, cx));
        }
    }

    /// Sends the action to the app once this update is done.
    fn send(&mut self, action: Action, cx: &mut Context<Self>) {
        if action == Action::RequestAccess(Permission::Accessibility) {
            self.asked_for_accessibility = true;
        }
        let on_action = self.on_action.clone();
        cx.defer(move |cx| on_action(action, cx));
    }

    fn on_click(
        &self,
        cx: &Context<Self>,
        act: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) -> OnClick {
        Box::new(cx.listener(move |view, _: &ClickEvent, window, cx| act(view, window, cx)))
    }

    fn sending(&self, action: Action, cx: &Context<Self>) -> OnClick {
        self.on_click(cx, move |view, _, cx| view.send(action, cx))
    }

    fn go_to(&self, step: Step, cx: &Context<Self>) -> OnClick {
        self.on_click(cx, move |view, window, cx| {
            view.step = step;
            view.focus_field_for_test(window, cx);
            cx.notify();
        })
    }

    /// Gives the field the colours of the system's appearance, once each time it changes.
    fn style_field(&mut self, palette: &Palette, cx: &mut Context<Self>) {
        if self.styled_dark == Some(palette.dark) {
            return;
        }
        self.styled_dark = Some(palette.dark);
        let style = skald_ui::input_style(palette);
        self.field
            .update(cx, |field, _| field.set_editor_style(style));
    }

    fn close(&mut self, _: &CloseWindow, window: &mut Window, cx: &mut Context<Self>) {
        self.send(Action::Closed, cx);
        window.remove_window();
    }

    fn header(&self, p: &Palette) -> Div {
        div()
            .flex()
            .flex_col()
            .gap(px(4.0))
            .px(px(28.0))
            .pt(px(22.0))
            .child(
                div()
                    .text_size(px(22.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(p.text)
                    .child("Velkommen til Skald"),
            )
            .child(controls::quiet(
                "Tre steg, så er Skald klar: last ned talemodellen, gi tilgang og prøv.",
                p,
            ))
    }

    fn step_bar(&self, p: &Palette, cx: &Context<Self>) -> Div {
        let mut bar = div()
            .flex()
            .items_center()
            .gap(px(10.0))
            .px(px(28.0))
            .py(px(18.0));
        for (index, step) in Step::ALL.into_iter().enumerate() {
            if index > 0 {
                bar = bar.child(div().flex_1().h(px(1.0)).bg(p.line));
            }
            let current = step == self.step;
            let label = format!("Steg {} av 3: {}", step.number(), step.label());
            bar = bar.child(
                Button::new(("step", index))
                    .accessibility_label(label)
                    .selected(current)
                    .flex_none()
                    .gap(px(8.0))
                    .px(px(6.0))
                    .py(px(4.0))
                    .rounded(px(6.0))
                    .cursor_pointer()
                    .hover(|style| style.opacity(0.85))
                    .child(controls::step_marker(
                        step.number(),
                        step.is_done(&self.model, &self.permissions),
                        current,
                        p,
                    ))
                    .child(
                        div()
                            .text_size(px(TEXT_SIZE))
                            .when(current, |label| label.font_weight(FontWeight::SEMIBOLD))
                            .text_color(if current { p.text } else { p.quiet })
                            .child(step.label()),
                    )
                    .on_click({
                        let go = self.go_to(step, cx);
                        move |event, window, cx| go(event, window, cx)
                    }),
            );
        }
        bar
    }

    fn model_step(&self, p: &Palette, cx: &Context<Self>) -> Vec<AnyElement> {
        let card_info = &self.model.model;
        let view = DownloadView::of(&self.model.download, card_info);
        let mut card = controls::card(p)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        div()
                            .text_size(px(15.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(p.text)
                            .child(card_info.name),
                    )
                    .child(controls::tag(card_info.licence, p))
                    .child(div().flex_1())
                    .child(controls::quiet(format::bytes(card_info.size), p)),
            )
            .child(controls::quiet(card_info.credit, p));

        if let Some(fraction) = view.progress.or(view.busy.then_some(0.0)) {
            card = card.child(div().pt(px(4.0)).child(controls::progress_bar(
                "download",
                fraction,
                view.busy,
                view.status.clone().into(),
                p,
            )));
        }
        let mut status = div()
            .flex()
            .flex_col()
            .gap(px(2.0))
            .flex_1()
            .min_w_0()
            .child(
                div()
                    .flex()
                    .gap(px(6.0))
                    .text_size(px(TEXT_SIZE))
                    .text_color(p.text)
                    .when(view.done, |line| {
                        line.child(div().text_color(p.done).child("✓"))
                    })
                    .child(view.status.clone()),
            );
        if let Some(detail) = &view.detail {
            status = status.child(controls::quiet(detail.clone(), p));
        }
        if let Some(error) = &view.error {
            status = status.child(controls::warning(error.clone(), p));
        }
        let mut row = div().flex().items_start().gap(px(12.0)).child(status);
        if let Some(spec) = &view.button {
            row = row.child(controls::spec_button(
                "download-button",
                spec,
                p,
                self.sending(spec.action, cx),
            ));
        }
        card = card.child(row);

        let mut elements = vec![
            controls::heading("Last ned talemodellen", p).into_any_element(),
            controls::quiet(
                "Skald gjenkjenner talen din her på maskinen. Modellen lastes ned én gang, og \
                 ingenting sendes fra maskinen.",
                p,
            )
            .into_any_element(),
            card.into_any_element(),
            controls::quiet(
                format!(
                    "Filen kontrolleres mot SHA-256 {} før den tas i bruk.",
                    format::short_hash(&card_info.sha256)
                ),
                p,
            )
            .into_any_element(),
        ];
        if self.model.no_vulkan {
            elements.push(controls::warning(crate::NO_VULKAN, p).into_any_element());
        }
        elements
    }

    fn permissions_step(&self, p: &Palette, cx: &Context<Self>) -> Vec<AnyElement> {
        let mac = self.model.platform == Platform::MacOs;
        let rows = permissions::rows(self.model.platform, &self.permissions);
        let all_granted = rows.iter().all(|row| row.granted);
        let mut card = controls::card(p).gap(px(0.0)).p(px(0.0));
        for (index, row) in rows.iter().enumerate() {
            card = card.child(self.permission_row(index, row, p, cx));
        }
        let mut elements = vec![
            controls::heading("Gi Skald tilgang", p).into_any_element(),
            controls::quiet(
                if mac {
                    "macOS spør om lov før et program kan bruke mikrofonen og tastaturet."
                } else {
                    "Skald trenger mikrofonen for å høre det du sier."
                },
                p,
            )
            .into_any_element(),
            card.into_any_element(),
        ];
        if mac && !all_granted {
            elements.push(controls::quiet(permissions::COME_BACK_NOTE, p).into_any_element());
        }
        if mac {
            elements.push(controls::note(permissions::CLIPBOARD_NOTE, p).into_any_element());
        }
        elements
    }

    fn permission_row(
        &self,
        index: usize,
        row: &PermissionRow,
        p: &Palette,
        cx: &Context<Self>,
    ) -> Div {
        let marker = div()
            .flex_none()
            .size(px(22.0))
            .rounded_full()
            .flex()
            .items_center()
            .justify_center()
            .text_size(px(SMALL_SIZE))
            .map(|marker| {
                if row.granted {
                    marker.bg(p.gold).text_color(p.on_gold).child("✓")
                } else {
                    marker
                        .border_1()
                        .border_color(if row.refused { p.ochre } else { p.line })
                }
            });
        let state_color = if row.granted {
            p.done
        } else if row.refused {
            p.ochre
        } else {
            p.quiet
        };
        let mut text = div()
            .flex()
            .flex_col()
            .gap(px(2.0))
            .flex_1()
            .min_w_0()
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_x(px(6.0))
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(p.text)
                            .child(row.name),
                    )
                    .child(div().text_color(p.quiet).child(row.purpose)),
            )
            .child(
                div()
                    .text_size(px(SMALL_SIZE))
                    .text_color(state_color)
                    .child(row.state),
            );
        if let Some(hint) = &row.hint {
            text = text.child(controls::quiet(hint.clone(), p));
        }
        let mut line = div()
            .flex()
            .items_center()
            .gap(px(12.0))
            .px(px(16.0))
            .py(px(14.0))
            .when(index > 0, |line| line.border_t_1().border_color(p.line))
            .child(marker)
            .child(text);
        if let Some(spec) = &row.button {
            line = line.child(controls::spec_button(
                ("permission", index),
                spec,
                p,
                self.sending(spec.action, cx),
            ));
        }
        line
    }

    fn test_step(&self, p: &Palette, window: &Window, cx: &Context<Self>) -> Vec<AnyElement> {
        let field = self.field.read(cx).value();
        let view = TestView::of(&self.model, &self.permissions, &field);
        let waiting = view.waiting_for.is_some();
        let mut card = controls::card(p)
            .items_center()
            .py(px(28.0))
            .gap(px(12.0))
            .child(
                div()
                    .text_size(px(17.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(if waiting { p.faint } else { p.text })
                    .child(view.instruction.clone()),
            );
        if let Some(waiting_for) = view.waiting_for {
            card = card.child(controls::quiet(waiting_for, p));
        }
        if view.field {
            card = card.child(controls::field(&self.field, p, window, cx));
        }
        if let Some(heard) = &view.heard {
            card = card.child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(6.0))
                    .w_full()
                    .child(controls::quiet("Skald hørte:", p))
                    .child(
                        div()
                            .w_full()
                            .px(px(14.0))
                            .py(px(10.0))
                            .rounded(px(8.0))
                            .bg(p.raised)
                            .border_1()
                            .border_color(p.line)
                            .text_size(px(15.0))
                            .text_color(p.text)
                            .flex()
                            .justify_center()
                            .child(heard.clone()),
                    ),
            );
        }
        if let Some(problem) = &view.problem {
            card = card.child(controls::warning(problem.clone(), p));
        }
        if let Some(status) = &view.status {
            card = card.child(controls::quiet(status.clone(), p));
        }
        if let Some(spec) = &view.button {
            card = card.child(controls::spec_button(
                "test-button",
                spec,
                p,
                self.sending(spec.action, cx),
            ));
        }
        let mut elements = vec![
            controls::heading("Prøv en diktering", p).into_any_element(),
            controls::quiet(
                "Prøven går hele veien: Skald hører deg, gjenkjenner talen og setter teksten inn i \
                 feltet nedenfor, slik den blir satt inn der markøren står.",
                p,
            )
            .into_any_element(),
            card.into_any_element(),
        ];
        if waiting {
            let mut links = div().flex().gap(px(8.0));
            if !Step::Model.is_done(&self.model, &self.permissions) {
                links = links.child(controls::button(
                    "to-model",
                    "Til talemodellen",
                    false,
                    true,
                    p,
                    self.go_to(Step::Model, cx),
                ));
            }
            if !Step::Permissions.is_done(&self.model, &self.permissions) {
                links = links.child(controls::button(
                    "to-permissions",
                    "Til tilgangene",
                    false,
                    true,
                    p,
                    self.go_to(Step::Permissions, cx),
                ));
            }
            elements.push(links.into_any_element());
        }
        elements
    }

    fn footer(&self, p: &Palette, cx: &Context<Self>) -> Div {
        let footer = Footer::of(self.step, &self.model, &self.permissions);
        let mut buttons = div().flex().gap(px(8.0));
        if let Some(previous) = self.step.previous() {
            buttons = buttons.child(controls::button(
                "back",
                "Tilbake",
                false,
                true,
                p,
                self.go_to(previous, cx),
            ));
        }
        if let Some(next) = self.step.next() {
            buttons = buttons.child(controls::button(
                "next",
                "Neste",
                self.step.is_done(&self.model, &self.permissions),
                true,
                p,
                self.go_to(next, cx),
            ));
        }
        if let Some(finish) = footer.finish {
            buttons = buttons.child(controls::button(
                "done",
                finish.label,
                finish.primary,
                finish.enabled,
                p,
                self.on_click(cx, |view, window, cx| {
                    view.send(Action::Done, cx);
                    window.remove_window();
                }),
            ));
        }
        div()
            .flex()
            .items_center()
            .gap(px(12.0))
            .px(px(28.0))
            .py(px(14.0))
            .border_t_1()
            .border_color(p.line)
            .bg(p.surface)
            .child(
                div()
                    .flex_1()
                    .text_size(px(SMALL_SIZE))
                    .text_color(p.quiet)
                    .children(footer.download_note),
            )
            .child(buttons)
    }
}

impl Render for FirstRunView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = Palette::of(window.appearance());
        self.style_field(&p, cx);
        let content = match self.step {
            Step::Model => self.model_step(&p, cx),
            Step::Permissions => self.permissions_step(&p, cx),
            Step::Test => self.test_step(&p, window, cx),
        };
        div()
            .key_context(KEY_CONTEXT)
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::close))
            .size_full()
            .flex()
            .flex_col()
            .bg(p.ground)
            .text_color(p.text)
            .text_size(px(TEXT_SIZE))
            .child(self.header(&p))
            .child(self.step_bar(&p, cx))
            .child(div().h(px(1.0)).bg(p.line))
            .child(
                div()
                    .id("step")
                    .flex_1()
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .gap(px(12.0))
                    .px(px(28.0))
                    .py(px(20.0))
                    .children(content),
            )
            .child(self.footer(&p, cx))
    }
}
