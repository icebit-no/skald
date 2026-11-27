//! Om, drawn.

use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, Context, FontWeight, div, px};

use super::controls::{SMALL_SIZE, link, section, tag};
use super::general::{list, row, tab_column};
use super::palette::Palette;
use super::view::SettingsView;
use crate::about::{AboutTab, Credit, LICENCE, NETWORK, NOTHING_LEAVES, SOURCE};

impl SettingsView {
    pub(super) fn render_about(&mut self, p: &Palette, _: &mut Context<Self>) -> AnyElement {
        let tab = AboutTab::of(&self.model);
        tab_column()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(
                        div()
                            .text_size(px(20.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(tab.title.clone()),
                    )
                    .child(div().text_color(p.quiet).child(LICENCE))
                    .child(div().flex().child(link("source", SOURCE, p))),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .p(px(12.0))
                    .rounded(px(8.0))
                    .border_1()
                    .border_color(p.gold)
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(NOTHING_LEAVES),
                    )
                    .child(
                        div()
                            .text_size(px(SMALL_SIZE))
                            .text_color(p.quiet)
                            .child(NETWORK),
                    ),
            )
            .child(section(
                "Talemodeller",
                None,
                p,
                [credits("model", &tab.models, p)],
            ))
            .child(section(
                "Motoren",
                None,
                p,
                [credits("engine", &tab.engine, p)],
            ))
            .into_any_element()
    }
}

fn credits(id: &'static str, credits: &[Credit], p: &Palette) -> AnyElement {
    list(
        p,
        credits.iter().enumerate().map(|(index, credit)| {
            row()
                .py(px(10.0))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .flex()
                        .flex_col()
                        .gap(px(3.0))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(6.0))
                                .child(div().font_weight(FontWeight::MEDIUM).child(credit.name))
                                .child(tag(credit.licence, p)),
                        )
                        .child(
                            div()
                                .text_size(px(SMALL_SIZE))
                                .text_color(p.quiet)
                                .child(credit.credit),
                        )
                        .child(div().flex().child(link((id, index), credit.link, p))),
                )
                .into_any_element()
        }),
    )
    .into_any_element()
}
