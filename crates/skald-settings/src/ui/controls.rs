//! The window's controls: gpui-kit's headless button, switch, checkbox and text input, dressed in the
//! window's colours.

use gpui_kit::base::input::InputState;
use gpui_kit::base::{Button, Checkbox, Input, Switch};
use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, App, ClickEvent, Div, ElementId, Entity, Focusable, FontWeight, SharedString,
    Window, div, px,
};

use crate::general::Choices;
use skald_ui::{Palette, with_alpha};

pub(crate) type OnClick = Box<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

pub(crate) const TEXT_SIZE: f32 = 13.0;
pub(crate) const SMALL_SIZE: f32 = 12.0;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    /// The one thing a row is for: gold.
    Primary,
    Secondary,
    /// Text only, for small things beside a row.
    Quiet,
}

pub(crate) fn button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    kind: Kind,
    palette: &Palette,
) -> Button {
    let p = *palette;
    let button = Button::new(id)
        .h(px(26.0))
        .px(px(10.0))
        .rounded(px(6.0))
        .text_size(px(SMALL_SIZE + 0.5))
        .cursor_pointer()
        .child(label.into());
    match kind {
        Kind::Primary => button
            .bg(p.gold)
            .text_color(p.on_gold)
            .font_weight(FontWeight::MEDIUM)
            .hover(move |style| style.bg(with_alpha(p.gold, 0.85))),
        Kind::Secondary => button
            .bg(p.raised)
            .border_1()
            .border_color(p.line)
            .text_color(p.text)
            .hover(move |style| style.border_color(p.quiet)),
        Kind::Quiet => button
            .px(px(6.0))
            .text_color(p.quiet)
            .hover(move |style| style.text_color(p.text)),
    }
}

/// A button that cannot be pressed now, greyed.
pub(crate) fn disabled(button: Button) -> Button {
    button.disabled(true).opacity(0.4).cursor_default()
}

/// One setting's options as a row of buttons, the chosen one gold.
pub(crate) fn segmented<T>(
    id: &'static str,
    choices: &Choices<T>,
    palette: &Palette,
    on_choose: impl Fn(usize) -> OnClick,
) -> Div {
    let p = *palette;
    div()
        .flex()
        .flex_wrap()
        .gap(px(6.0))
        .children(choices.options.iter().enumerate().map(|(index, choice)| {
            let on_click = on_choose(index);
            let button = Button::new((id, index))
                .selected(choice.selected)
                .h(px(28.0))
                .px(px(12.0))
                .rounded(px(6.0))
                .text_size(px(TEXT_SIZE))
                .cursor_pointer()
                .child(choice.label)
                .on_click(move |event, window, cx| on_click(event, window, cx));
            if choice.selected {
                button
                    .bg(p.gold)
                    .text_color(p.on_gold)
                    .font_weight(FontWeight::MEDIUM)
            } else {
                button
                    .bg(p.raised)
                    .border_1()
                    .border_color(p.line)
                    .text_color(p.text)
                    .hover(move |style| style.border_color(p.quiet))
            }
        }))
}

/// A setting that is on or off: the label, and the switch at the row's end. The whole row toggles.
pub(crate) fn switch_row(
    id: &'static str,
    label: &'static str,
    on: bool,
    palette: &Palette,
    on_click: OnClick,
) -> impl IntoElement {
    let p = *palette;
    Switch::new(id)
        .checked(on)
        .accessibility_label(label)
        .on_change(move |_, event, window, cx| on_click(event, window, cx))
        .flex()
        .items_center()
        .justify_between()
        .gap(px(12.0))
        .w_full()
        .py(px(7.0))
        .cursor_pointer()
        .child(div().text_color(p.text).child(label))
        .child(
            div()
                .flex_none()
                .w(px(34.0))
                .h(px(20.0))
                .p(px(2.0))
                .rounded_full()
                .flex()
                .when(on, |track| track.justify_end())
                .bg(match (on, p.dark) {
                    (true, _) => p.gold,
                    (false, true) => p.line,
                    (false, false) => p.faint,
                })
                .child(div().size(px(16.0)).rounded_full().bg(match (on, p.dark) {
                    (true, _) => p.on_gold,
                    (false, true) => p.text,
                    (false, false) => p.raised,
                })),
        )
}

pub(crate) fn checkbox_row(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    checked: bool,
    palette: &Palette,
    on_click: OnClick,
) -> impl IntoElement {
    let p = *palette;
    let label = label.into();
    Checkbox::new(id)
        .checked(checked)
        .accessibility_label(label.clone())
        .on_change(move |_, event, window, cx| on_click(event, window, cx))
        .flex()
        .items_center()
        .gap(px(8.0))
        .py(px(3.0))
        .cursor_pointer()
        .child(
            div()
                .flex_none()
                .size(px(15.0))
                .rounded(px(4.0))
                .border_1()
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(11.0))
                .map(|tick| {
                    if checked {
                        tick.bg(p.gold)
                            .border_color(p.gold)
                            .text_color(p.on_gold)
                            .child("✓")
                    } else {
                        tick.border_color(p.quiet)
                    }
                }),
        )
        .child(div().text_color(p.text).child(label))
}

/// A text field: the input in a box that turns gold while it has the focus.
pub(crate) fn field(
    input: &Entity<InputState>,
    palette: &Palette,
    window: &Window,
    cx: &App,
) -> Div {
    let focused = input.read(cx).focus_handle(cx).is_focused(window);
    div()
        .flex()
        .items_center()
        .h(px(28.0))
        .px(px(8.0))
        .rounded(px(6.0))
        .bg(palette.raised)
        .border_1()
        .border_color(if focused { palette.gold } else { palette.line })
        .text_size(px(TEXT_SIZE))
        .child(Input::new(input))
}

/// A part of a tab: its heading, an optional line of help, and what it holds.
pub(crate) fn section(
    title: impl Into<SharedString>,
    help: Option<&str>,
    palette: &Palette,
    content: impl IntoIterator<Item = AnyElement>,
) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(8.0))
        .child(
            div()
                .text_size(px(SMALL_SIZE))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(palette.quiet)
                .child(title.into()),
        )
        .when_some(help, |section, help| {
            section.child(
                div()
                    .text_size(px(SMALL_SIZE))
                    .text_color(palette.quiet)
                    .child(help.to_owned()),
            )
        })
        .children(content)
}

/// A line that says what went wrong with what was typed.
pub(crate) fn message(text: &str, palette: &Palette) -> Div {
    div()
        .text_size(px(SMALL_SIZE))
        .text_color(palette.ochre)
        .child(text.to_owned())
}

/// A small word beside a name, as «beta» or «Festet».
pub(crate) fn tag(text: &'static str, palette: &Palette) -> Div {
    div()
        .flex_none()
        .px(px(6.0))
        .py(px(1.0))
        .rounded(px(4.0))
        .border_1()
        .border_color(palette.line)
        .text_size(px(SMALL_SIZE - 1.0))
        .text_color(palette.quiet)
        .child(text)
}

/// A link: the address without its scheme, opened in the browser.
pub(crate) fn link(
    id: impl Into<ElementId>,
    url: &'static str,
    palette: &Palette,
) -> impl IntoElement {
    let p = *palette;
    let shown = url.trim_start_matches("https://");
    Button::new(id)
        .role(gpui_kit::Role::Link)
        .accessibility_label(shown)
        .justify_start()
        .text_size(px(SMALL_SIZE))
        .text_color(p.link)
        .underline()
        .cursor_pointer()
        .child(shown)
        .on_click(move |_, _, cx| cx.open_url(url))
}
