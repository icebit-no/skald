//! Ordskatt, drawn.

use gpui_kit::base::Button;
use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, Context, Focusable, Window, div, px, relative};

use super::controls::{Kind, SMALL_SIZE, button, field, message, section};
use super::general::{list, row, tab_column};
use super::palette::{Palette, with_alpha};
use super::view::{Editing, SettingsView};
use crate::vocabulary::{VocabularyTab, try_corrections};

const SENTENCE_HELP: &str =
    "Én setning med ordene dine bøyd slik du sier dem. Modellen får den før ordene.";
const WORDS_HELP: &str = "Ord og navn modellen skal kjenne. Det viktigste står sist: modellen \
     beholder slutten når det blir for mye.";
const CORRECTIONS_HELP: &str = "Bytter ord modellen hører riktig, men skriver feil. Hele ord, \
     uansett store og små bokstaver.";

impl SettingsView {
    pub(super) fn render_vocabulary(
        &mut self,
        p: &Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let tab = VocabularyTab::of(&self.model);
        // A sentence changed elsewhere, by `skald vocab sentence`, shows unless it is being typed.
        let sentence_focused = self.sentence.read(cx).focus_handle(cx).is_focused(window);
        if !sentence_focused && Self::value(&self.sentence, cx) != tab.sentence {
            Self::set_value(&self.sentence, tab.sentence.clone(), window, cx);
        }

        let sentence = section(
            "Setningen",
            Some(SENTENCE_HELP),
            p,
            [field(&self.sentence, p, window, cx).into_any_element()]
                .into_iter()
                .chain(tab.sentence_dropped.then(|| {
                    message("Får ikke plass, så modellen ser den ikke.", p).into_any_element()
                })),
        );

        let words = tab.words.iter().enumerate().map(|(index, word)| {
            let on_remove = self.on_click(cx, move |view, _, cx| {
                let change = VocabularyTab::of(&view.model).remove_word(index);
                view.change(change, cx);
            });
            let pp = *p;
            div()
                .flex()
                .items_center()
                .gap(px(2.0))
                .pl(px(9.0))
                .pr(px(3.0))
                .h(px(26.0))
                .rounded(px(13.0))
                .bg(p.raised)
                .border_1()
                .border_color(p.line)
                .child(
                    div()
                        .text_color(if word.dropped { p.faint } else { p.text })
                        .when(word.dropped, |text| text.line_through())
                        .child(word.word.clone()),
                )
                .child(
                    Button::new(("remove-word", index))
                        .accessibility_label(format!("Fjern «{}»", word.word))
                        .size(px(20.0))
                        .rounded_full()
                        .text_color(p.quiet)
                        .cursor_pointer()
                        .hover(move |style| style.text_color(pp.text).bg(pp.line))
                        .child("×")
                        .on_click(move |event, window, cx| on_remove(event, window, cx)),
                )
                .into_any_element()
        });
        let add_word = self.on_click(cx, |view, window, cx| view.add_word(window, cx));
        let meter_fill = tab.meter.fill();
        let meter_colour = if tab.meter.is_over() { p.ochre } else { p.gold };
        let meter = div()
            .flex()
            .flex_col()
            .gap(px(4.0))
            .child(
                div().h(px(6.0)).w_full().rounded_full().bg(p.line).child(
                    div()
                        .h_full()
                        .w(relative(meter_fill))
                        .rounded_full()
                        .bg(meter_colour),
                ),
            )
            .child(
                div()
                    .flex()
                    .gap(px(8.0))
                    .text_size(px(SMALL_SIZE))
                    .child(div().text_color(p.quiet).child(tab.meter.text()))
                    .children(
                        tab.meter
                            .warning()
                            .map(|warning| div().text_color(p.ochre).child(warning)),
                    ),
            );
        let words_section = section(
            "Ord og navn",
            Some(WORDS_HELP),
            p,
            [
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(6.0))
                    .children(words)
                    .when(tab.words.is_empty(), |words| {
                        words.child(div().text_color(p.quiet).child("Ingen ord ennå."))
                    })
                    .into_any_element(),
                div()
                    .flex()
                    .gap(px(8.0))
                    .child(field(&self.new_word, p, window, cx).flex_1())
                    .child(
                        button("add-word", "Legg til", Kind::Secondary, p)
                            .h(px(28.0))
                            .on_click(move |event, window, cx| add_word(event, window, cx)),
                    )
                    .into_any_element(),
                meter.into_any_element(),
            ]
            .into_iter()
            .chain(
                self.vocabulary_message
                    .as_deref()
                    .filter(|_| self.editing.is_none())
                    .map(|text| message(text, p).into_any_element()),
            ),
        );

        let corrections_section = section(
            "Rettinger",
            Some(CORRECTIONS_HELP),
            p,
            [self.corrections(&tab, p, window, cx)],
        );

        let tried = try_corrections(&self.model.config, &Self::value(&self.try_text, cx));
        let typed = !Self::value(&self.try_text, cx).trim().is_empty();
        let try_section = section(
            "Prøv på en tekst",
            Some("Slik blir teksten med rettingene du har nå."),
            p,
            [field(&self.try_text, p, window, cx).into_any_element()]
                .into_iter()
                .chain(typed.then(|| {
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(4.0))
                        .p(px(10.0))
                        .rounded(px(6.0))
                        .bg(p.surface)
                        .border_1()
                        .border_color(p.line)
                        .child(div().child(tried.text.clone()))
                        .child(div().text_size(px(SMALL_SIZE)).text_color(p.quiet).child(
                            if tried.applied.is_empty() {
                                "Ingen retting passer.".to_owned()
                            } else {
                                format!("Brukt: {}", tried.applied.join(", "))
                            },
                        ))
                        .into_any_element()
                })),
        );

        tab_column()
            .child(sentence)
            .child(words_section)
            .child(corrections_section)
            .child(try_section)
            .into_any_element()
    }

    fn corrections(
        &self,
        tab: &VocabularyTab,
        p: &Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let rows = tab.corrections.iter().enumerate().map(|(index, rule)| {
            if self.editing == Some(Editing::Rule(index)) {
                return self.correction_editor(p, window, cx);
            }
            let on_edit = self.on_click(cx, move |view, window, cx| {
                view.edit_correction(Editing::Rule(index), window, cx)
            });
            let on_remove = self.on_click(cx, move |view, _, cx| {
                let change = VocabularyTab::of(&view.model).remove_correction(index);
                view.change(change, cx);
            });
            row()
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap(px(6.0))
                        .child(div().text_color(p.quiet).child(format!("«{}»", rule.from)))
                        .child(div().text_color(p.faint).child("→"))
                        .child(div().child(format!("«{}»", rule.to)))
                        .child(
                            div()
                                .text_size(px(SMALL_SIZE))
                                .text_color(p.quiet)
                                .child(format!("· {}", rule.applied)),
                        ),
                )
                .child(
                    button(("edit-correction", index), "Endre", Kind::Quiet, p)
                        .on_click(move |event, window, cx| on_edit(event, window, cx)),
                )
                .child(
                    button(("remove-correction", index), "Fjern", Kind::Quiet, p)
                        .on_click(move |event, window, cx| on_remove(event, window, cx)),
                )
                .into_any_element()
        });
        let rows: Vec<AnyElement> = rows.collect();
        let empty = rows.is_empty() && self.editing != Some(Editing::New);
        let new = if self.editing == Some(Editing::New) {
            Some(self.correction_editor(p, window, cx))
        } else {
            None
        };
        let on_new = self.on_click(cx, |view, window, cx| {
            view.edit_correction(Editing::New, window, cx)
        });
        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .when(!rows.is_empty() || new.is_some(), |column| {
                column.child(list(p, rows.into_iter().chain(new)))
            })
            .when(empty, |column| {
                column.child(div().text_color(p.quiet).child("Ingen rettinger ennå."))
            })
            .when(self.editing.is_none(), |column| {
                column.child(
                    div().flex().child(
                        button("new-correction", "+ Ny retting", Kind::Secondary, p)
                            .on_click(move |event, window, cx| on_new(event, window, cx)),
                    ),
                )
            })
            .into_any_element()
    }

    fn correction_editor(
        &self,
        p: &Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let on_save = self.on_click(cx, |view, window, cx| view.save_correction(window, cx));
        let on_cancel = self.on_click(cx, |view, _, cx| {
            view.editing = None;
            view.vocabulary_message = None;
            cx.notify();
        });
        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .p(px(12.0))
            .bg(with_alpha(p.gold, 0.06))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(field(&self.correction_from, p, window, cx).flex_1())
                    .child(div().text_color(p.faint).child("→"))
                    .child(field(&self.correction_to, p, window, cx).flex_1()),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        button("save-correction", "Lagre", Kind::Primary, p)
                            .on_click(move |event, window, cx| on_save(event, window, cx)),
                    )
                    .child(
                        button("cancel-correction", "Avbryt", Kind::Quiet, p)
                            .on_click(move |event, window, cx| on_cancel(event, window, cx)),
                    )
                    .children(
                        self.vocabulary_message
                            .as_deref()
                            .map(|text| message(text, p)),
                    ),
            )
            .into_any_element()
    }
}
