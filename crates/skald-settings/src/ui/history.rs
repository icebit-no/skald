//! Historikk, drawn.

use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, ClickEvent, ClipboardItem, Context, FontWeight, Window, div, px};

use super::controls::{Kind, SMALL_SIZE, button, checkbox_row, field, message};
use super::general::tab_column;
use super::palette::{Palette, with_alpha};
use super::view::SettingsView;
use crate::history::{self, HistoryRow, HistoryTab};

const HELP: &str =
    "Klikk på et ord som ble feil for å rette det. Hold ⇧ og klikk for å ta med flere ord.";

impl SettingsView {
    pub(super) fn render_history(
        &mut self,
        p: &Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let tab = HistoryTab::of(&self.model, Self::now_ms());
        if tab.entries.is_empty() {
            return tab_column()
                .child(div().text_color(p.quiet).child("Ingen tekster ennå."))
                .into_any_element();
        }
        let entries: Vec<AnyElement> = tab
            .entries
            .iter()
            .enumerate()
            .map(|(index, entry)| self.entry(&tab, index, entry, p, window, cx))
            .collect();
        tab_column()
            .gap(px(12.0))
            .child(
                div()
                    .text_size(px(SMALL_SIZE))
                    .text_color(p.quiet)
                    .child(HELP),
            )
            .children(entries)
            .into_any_element()
    }

    fn entry(
        &self,
        tab: &HistoryTab,
        index: usize,
        entry: &HistoryRow,
        p: &Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pick = self.pick.filter(|pick| pick.entry == index);
        let pp = *p;
        let pieces = entry.pieces.iter().enumerate().map(|(at, piece)| {
            let picked = pick.is_some_and(|pick| (pick.first..=pick.last).contains(&at));
            let text = piece.text.replace('\n', " ");
            if !piece.is_word {
                return div()
                    .when(picked, |gap| gap.bg(pp.selection))
                    .child(text)
                    .into_any_element();
            }
            let on_pick = cx.listener(move |view, event: &ClickEvent, window, cx| {
                view.pick_word(index, at, event.modifiers().shift, window, cx);
            });
            div()
                .id(("word", at))
                .rounded(px(3.0))
                .cursor_pointer()
                .map(|word| {
                    if picked {
                        word.bg(pp.selection)
                    } else {
                        word.hover(move |style| style.bg(with_alpha(pp.gold, 0.18)))
                    }
                })
                .on_click(on_pick)
                .child(text)
                .into_any_element()
        });
        let pieces: Vec<AnyElement> = pieces.collect();
        let text = entry.text.clone();
        let unix_ms = entry.unix_ms;
        let on_copy = self.on_click(cx, move |view, _, cx| {
            cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
            view.copied = Some(unix_ms);
            cx.notify();
        });
        let copied = self.copied == Some(entry.unix_ms);
        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .p(px(12.0))
            .rounded(px(8.0))
            .bg(p.surface)
            .border_1()
            .border_color(if pick.is_some() { p.gold } else { p.line })
            .child(
                div()
                    .id(("entry", index))
                    .flex()
                    .flex_wrap()
                    .line_height(px(20.0))
                    .children(pieces),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(SMALL_SIZE))
                            .text_color(p.quiet)
                            .child(entry.details.clone()),
                    )
                    .child(
                        button(
                            ("copy", index),
                            if copied { "Kopiert" } else { "Kopier" },
                            Kind::Secondary,
                            p,
                        )
                        .on_click(move |event, window, cx| on_copy(event, window, cx)),
                    ),
            )
            .children(pick.map(|pick| self.fix_panel(tab, pick, p, window, cx)))
            .into_any_element()
    }

    fn fix_panel(
        &self,
        tab: &HistoryTab,
        pick: history::Pick,
        p: &Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let wrong = tab.picked(pick).unwrap_or_default();
        let replacement = Self::value(&self.replacement, cx);
        let on_fix = self.on_click(cx, |view, _, cx| view.fix(cx));
        let on_cancel = self.on_click(cx, |view, _, cx| {
            view.pick = None;
            view.history_message = None;
            cx.notify();
        });
        let correct = !self.correct_next_time;
        let add = !self.add_to_vocabulary;
        div()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .pt(px(10.0))
            .border_t_1()
            .border_color(p.line)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        div()
                            .flex_none()
                            .font_weight(FontWeight::MEDIUM)
                            .child(history::replace_label(&wrong)),
                    )
                    .child(field(&self.replacement, p, window, cx).flex_1()),
            )
            .child(checkbox_row(
                "correct-next-time",
                history::CORRECT_NEXT_TIME,
                self.correct_next_time,
                p,
                self.on_click(cx, move |view, _, cx| {
                    view.correct_next_time = correct;
                    cx.notify();
                }),
            ))
            .child(checkbox_row(
                "add-to-vocabulary",
                history::add_word_label(&replacement),
                self.add_to_vocabulary,
                p,
                self.on_click(cx, move |view, _, cx| {
                    view.add_to_vocabulary = add;
                    cx.notify();
                }),
            ))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .pt(px(4.0))
                    .child(
                        button("fix", "Bytt", Kind::Primary, p)
                            .on_click(move |event, window, cx| on_fix(event, window, cx)),
                    )
                    .child(
                        button("cancel-fix", "Avbryt", Kind::Quiet, p)
                            .on_click(move |event, window, cx| on_cancel(event, window, cx)),
                    )
                    .children(self.history_message.as_deref().map(|text| message(text, p))),
            )
            .into_any_element()
    }
}
