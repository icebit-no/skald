//! The window's controls, dressed in its colours: gpui-kit's headless button, progress bar and text
//! input, the step bar and the boxes a step is laid out in.

use std::time::Duration;

use gpui_kit::base::input::{Input, InputState};
use gpui_kit::base::{Button, Progress};
use gpui_kit::prelude::*;
use gpui_kit::{
    Animation, AnimationExt, App, ClickEvent, Div, ElementId, Entity, Focusable, FontWeight,
    SharedString, Window, div, px, relative,
};

use crate::button::ButtonSpec;
use skald_ui::{Palette, with_alpha};

pub(crate) type OnClick = Box<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

pub(crate) const TEXT_SIZE: f32 = 13.0;
pub(crate) const SMALL_SIZE: f32 = 12.0;

pub(crate) fn button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    primary: bool,
    enabled: bool,
    palette: &Palette,
    on_click: OnClick,
) -> Button {
    let p = *palette;
    let button = Button::new(id)
        .h(px(28.0))
        .px(px(14.0))
        .rounded(px(6.0))
        .text_size(px(TEXT_SIZE))
        .child(label.into());
    let button = if primary {
        button
            .bg(p.gold)
            .text_color(p.on_gold)
            .font_weight(FontWeight::MEDIUM)
            .hover(move |style| style.bg(with_alpha(p.gold, 0.85)))
    } else {
        button
            .bg(p.raised)
            .border_1()
            .border_color(p.line)
            .text_color(p.text)
            .hover(move |style| style.border_color(p.quiet))
    };
    if enabled {
        button
            .cursor_pointer()
            .on_click(move |event, window, cx| on_click(event, window, cx))
    } else {
        button.disabled(true).opacity(0.4).cursor_default()
    }
}

pub(crate) fn spec_button(
    id: impl Into<ElementId>,
    spec: &ButtonSpec,
    palette: &Palette,
    on_click: OnClick,
) -> Button {
    button(id, spec.label, spec.primary, true, palette, on_click)
}

/// A bar of how much is done; while `busy`, a segment that moves without a measure.
pub(crate) fn progress_bar(
    id: &'static str,
    fraction: f32,
    busy: bool,
    label: SharedString,
    palette: &Palette,
) -> Progress {
    let p = *palette;
    let fill = div()
        .absolute()
        .top_0()
        .left_0()
        .h_full()
        .w(relative(fraction.clamp(0.0, 1.0)))
        .rounded_full()
        .bg(p.gold);
    let bar = Progress::new(id)
        .value(fraction * 100.0)
        .indeterminate(busy)
        .accessibility_label(label)
        .relative()
        .w_full()
        .h(px(6.0))
        .rounded_full()
        .overflow_hidden()
        .bg(p.line)
        .child(fill);
    if !busy {
        return bar;
    }
    bar.child(
        div()
            .absolute()
            .top_0()
            .h_full()
            .w(relative(0.25))
            .rounded_full()
            .bg(with_alpha(p.gold, 0.55))
            .with_animation(
                (id, 1usize),
                Animation::new(Duration::from_millis(1400)).repeat(),
                |segment, delta| segment.left(relative(-0.25 + 1.25 * delta)),
            ),
    )
}

/// A disc with the step's number, a tick once it is done, gold while it is shown.
pub(crate) fn step_marker(number: usize, done: bool, current: bool, palette: &Palette) -> Div {
    let p = *palette;
    let disc = div()
        .flex_none()
        .size(px(22.0))
        .rounded_full()
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(SMALL_SIZE))
        .font_weight(FontWeight::SEMIBOLD);
    match (done, current) {
        (true, _) => disc.bg(p.gold).text_color(p.on_gold).child("✓"),
        (false, true) => disc
            .border_2()
            .border_color(p.gold)
            .text_color(p.text)
            .child(number.to_string()),
        (false, false) => disc
            .border_1()
            .border_color(p.line)
            .text_color(p.quiet)
            .child(number.to_string()),
    }
}

/// The raised box a step's main content stands in.
pub(crate) fn card(palette: &Palette) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(10.0))
        .p(px(16.0))
        .rounded(px(10.0))
        .bg(palette.surface)
        .border_1()
        .border_color(palette.line)
}

pub(crate) fn heading(text: impl Into<SharedString>, palette: &Palette) -> Div {
    div()
        .text_size(px(17.0))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(palette.text)
        .child(text.into())
}

pub(crate) fn quiet(text: impl Into<SharedString>, palette: &Palette) -> Div {
    div()
        .text_size(px(SMALL_SIZE))
        .text_color(palette.quiet)
        .child(text.into())
}

pub(crate) fn warning(text: impl Into<SharedString>, palette: &Palette) -> Div {
    div()
        .text_size(px(SMALL_SIZE))
        .text_color(palette.ochre)
        .child(text.into())
}

/// A small word beside a name, as the licence.
pub(crate) fn tag(text: impl Into<SharedString>, palette: &Palette) -> Div {
    div()
        .flex_none()
        .px(px(6.0))
        .py(px(1.0))
        .rounded(px(4.0))
        .border_1()
        .border_color(palette.line)
        .text_size(px(SMALL_SIZE - 1.0))
        .text_color(palette.quiet)
        .child(text.into())
}

/// A note set apart from the step, as the one about the clipboard.
pub(crate) fn note(text: impl Into<SharedString>, palette: &Palette) -> Div {
    div()
        .flex()
        .gap(px(8.0))
        .p(px(10.0))
        .rounded(px(8.0))
        .bg(with_alpha(
            palette.gold,
            if palette.dark { 0.08 } else { 0.12 },
        ))
        .text_size(px(SMALL_SIZE))
        .text_color(palette.quiet)
        .child(div().flex_none().text_color(palette.done).child("ⓘ"))
        .child(div().flex_1().min_w_0().child(text.into()))
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
        .w_full()
        .h(px(36.0))
        .px(px(10.0))
        .rounded(px(8.0))
        .bg(palette.raised)
        .border_1()
        .border_color(if focused { palette.gold } else { palette.line })
        .text_size(px(15.0))
        .child(Input::new(input))
}
