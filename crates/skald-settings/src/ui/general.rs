//! Generelt, drawn.

use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, Context, Div, FontWeight, div, px, relative};

use super::controls::{
    self, Kind, SMALL_SIZE, button, disabled, section, segmented, switch_row, tag,
};
use super::view::SettingsView;
use crate::action::Change;
use crate::general::{
    Choosing, General, HoldKeyField, KEYS_TAKEN, MicrophoneRow, ModelRow, PRESS_A_KEY,
};
use skald_ui::Palette;

const MICROPHONE_HELP: &str = "Skald bruker den første som er koblet til. Bluetooth-hodesett får \
     telefonlyd mens mikrofonen er åpen, så sett dem sist.";

impl SettingsView {
    pub(super) fn render_general(&mut self, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let general = General::of(&self.model);
        let hold_key = self.hold_key_field(&general.hold_key, p, cx);
        let language = segmented("language", &general.language, p, |index| {
            self.on_click(cx, move |view, _, cx| {
                let change = General::of(&view.model)
                    .language
                    .choose(index, Change::Language);
                view.change(change, cx);
            })
        });
        let theme = segmented("overlay-theme", &general.overlay_theme, p, |index| {
            self.on_click(cx, move |view, _, cx| {
                let change = General::of(&view.model)
                    .overlay_theme
                    .choose(index, Change::OverlayTheme);
                view.change(change, cx);
            })
        });
        let unknown_theme = general.overlay_theme.unknown.clone();

        let microphones: Vec<AnyElement> = general
            .microphones
            .iter()
            .enumerate()
            .map(|(index, row)| self.microphone_row(index, row, p, cx))
            .collect();
        let unpin = {
            let button = button(
                "unpin",
                "Bruk den første som er koblet til",
                Kind::Secondary,
                p,
            );
            if general.pinned {
                let on_click = self.on_click(cx, |view, _, cx| {
                    let change = General::of(&view.model).unpin_microphone();
                    view.change(change, cx);
                });
                button.on_click(move |event, window, cx| on_click(event, window, cx))
            } else {
                disabled(button)
            }
        };
        let models: Vec<AnyElement> = general
            .models
            .iter()
            .enumerate()
            .map(|(index, row)| self.model_row(index, row, p, cx))
            .collect();

        let copy = !general.copy_to_clipboard;
        let login = !general.start_at_login;
        let sounds = !general.sounds;
        let switches = [
            switch_row(
                "copy-to-clipboard",
                "Kopier også teksten til utklippstavlen",
                general.copy_to_clipboard,
                p,
                self.on_click(cx, move |view, _, cx| {
                    view.change(Some(Change::CopyToClipboard(copy)), cx)
                }),
            )
            .into_any_element(),
            switch_row(
                "start-at-login",
                "Start Skald ved innlogging",
                general.start_at_login,
                p,
                self.on_click(cx, move |view, _, cx| {
                    view.change(Some(Change::StartAtLogin(login)), cx)
                }),
            )
            .into_any_element(),
            switch_row(
                "sounds",
                "Lyd når opptaket starter og slutter",
                general.sounds,
                p,
                self.on_click(cx, move |view, _, cx| {
                    view.change(Some(Change::Sounds(sounds)), cx)
                }),
            )
            .into_any_element(),
        ];

        tab_column()
            .child(section(
                "Tasten du holder inne for å snakke",
                None,
                p,
                hold_key,
            ))
            .child(section(
                "Skriftspråk",
                None,
                p,
                [language.into_any_element()],
            ))
            .child(section(
                "Mikrofoner",
                Some(MICROPHONE_HELP),
                p,
                [
                    if microphones.is_empty() {
                        list(
                            p,
                            [row()
                                .child(
                                    div()
                                        .text_color(p.quiet)
                                        .child("Ingen mikrofon er koblet til."),
                                )
                                .into_any_element()],
                        )
                    } else {
                        list(p, microphones)
                    }
                    .into_any_element(),
                    div().flex().child(unpin).into_any_element(),
                ],
            ))
            .child(section(
                "Talemodell",
                None,
                p,
                [list(p, models).into_any_element()],
            ))
            .child(section(
                "Overleggets utseende",
                None,
                p,
                [theme.into_any_element()]
                    .into_iter()
                    .chain(unknown_theme.map(|theme| {
                        note(
                            &format!("Satt til «{theme}», som denne versjonen ikke kjenner."),
                            p,
                        )
                    })),
            ))
            .child(div().flex().flex_col().children(switches))
            .into_any_element()
    }

    /// The key set in a box, with «Velg tast …» to press another in its place, or on Linux the lines that
    /// bind it; then what to know about it.
    fn hold_key_field(
        &self,
        field: &HoldKeyField,
        p: &Palette,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let waiting = field.choosing == Choosing::Pressed { waiting: true };
        let shown = div()
            .flex()
            .items_center()
            .min_w(px(180.0))
            .h(px(28.0))
            .px(px(10.0))
            .rounded(px(6.0))
            .bg(p.raised)
            .border_1()
            .border_color(if waiting { p.gold } else { p.line })
            .text_color(if waiting { p.quiet } else { p.text })
            .font_weight(if waiting {
                FontWeight::NORMAL
            } else {
                FontWeight::MEDIUM
            })
            .child(if waiting {
                PRESS_A_KEY.to_owned()
            } else {
                field.label.clone()
            });
        let mut row = div().flex().items_center().gap(px(8.0)).child(shown);
        if let Some(action) = field.button() {
            let label = if waiting { "Avbryt" } else { "Velg tast …" };
            let on_click = self.on_click(cx, move |view, _, cx| view.send(action.clone(), cx));
            row = row.child(
                button("choose-hold-key", label, Kind::Secondary, p)
                    .on_click(move |event, window, cx| on_click(event, window, cx)),
            );
        }
        let mut elements = vec![row.into_any_element()];
        if waiting {
            elements.push(note(KEYS_TAKEN, p));
        }
        if let Choosing::Bound(binding) = &field.choosing {
            let name = &field.name;
            match binding {
                Some(lines) => {
                    elements.push(note(
                        &format!(
                            "Skrivebordet holder tasten. Med disse linjene i Hyprland-oppsettet holder du {name}:"
                        ),
                        p,
                    ));
                    elements.push(
                        div()
                            .flex()
                            .flex_col()
                            .px(px(10.0))
                            .py(px(6.0))
                            .rounded(px(6.0))
                            .bg(p.raised)
                            .border_1()
                            .border_color(p.line)
                            .text_size(px(SMALL_SIZE))
                            .text_color(p.text)
                            .children(lines.iter().cloned())
                            .into_any_element(),
                    );
                }
                None => elements.push(note(
                    &format!("Hyprland kan ikke binde {name} slik. Velg en annen tast."),
                    p,
                )),
            }
            elements.push(note(
                "Tasten Skald viser, velger du med skald config set hold_key, med samme tast som i linjene.",
                p,
            ));
        }
        if let Some(warning) = &field.warning {
            elements.push(controls::message(warning, p).into_any_element());
        }
        if let Some(key) = &field.unknown {
            elements.push(note(
                &format!(
                    "Satt til «{key}», som denne versjonen ikke kjenner, så Skald lytter etter {} i stedet.",
                    field.name
                ),
                p,
            ));
        }
        elements
    }

    fn microphone_row(
        &self,
        index: usize,
        microphone: &MicrophoneRow,
        p: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let arrow =
            |id: &'static str, label: &'static str, enabled: bool, up: bool| {
                let button = button((id, index), label, Kind::Quiet, p)
                    .accessibility_label(if up { "Flytt opp" } else { "Flytt ned" });
                if enabled {
                    let on_click = self.on_click(cx, move |view, _, cx| {
                        let change = General::of(&view.model).move_microphone(index, up);
                        view.change(change, cx);
                    });
                    button.on_click(move |event, window, cx| on_click(event, window, cx))
                } else {
                    disabled(button)
                }
            };
        let up = arrow("microphone-up", "↑", microphone.can_move_up, true);
        let down = arrow("microphone-down", "↓", microphone.can_move_down, false);
        let pin = if microphone.pinned {
            tag("Festet", p).into_any_element()
        } else {
            let on_click = self.on_click(cx, move |view, _, cx| {
                let change = General::of(&view.model).pin_microphone(index);
                view.change(change, cx);
            });
            button(("microphone-pin", index), "Bruk denne", Kind::Quiet, p)
                .on_click(move |event, window, cx| on_click(event, window, cx))
                .into_any_element()
        };
        let remove = microphone.can_remove.then(|| {
            let on_click = self.on_click(cx, move |view, _, cx| {
                let change = General::of(&view.model).remove_microphone(index);
                view.change(change, cx);
            });
            button(("microphone-remove", index), "Fjern", Kind::Quiet, p)
                .on_click(move |event, window, cx| on_click(event, window, cx))
        });
        row()
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .flex()
                    .flex_col()
                    .gap(px(1.0))
                    .child(
                        div()
                            .text_color(if microphone.connected {
                                p.text
                            } else {
                                p.faint
                            })
                            .when(microphone.in_use, |name| {
                                name.font_weight(FontWeight::MEDIUM)
                            })
                            .child(microphone.name.clone()),
                    )
                    .child(
                        div()
                            .text_size(px(SMALL_SIZE))
                            .text_color(p.quiet)
                            .child(microphone.details()),
                    ),
            )
            .child(pin)
            .children(remove)
            .child(up)
            .child(down)
            .into_any_element()
    }

    fn model_row(
        &self,
        index: usize,
        model: &ModelRow,
        p: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colour = if model.greyed { p.faint } else { p.text };
        let marker = div()
            .flex_none()
            .size(px(14.0))
            .rounded_full()
            .border_1()
            .border_color(if model.selected { p.gold } else { p.quiet })
            .when(
                model.greyed || !(model.selectable || model.selected),
                |marker| marker.opacity(0.5),
            )
            .flex()
            .items_center()
            .justify_center()
            .when(model.selected, |marker| {
                marker.child(div().size(px(8.0)).rounded_full().bg(p.gold))
            });
        let action = model.button.map(|kind| {
            let id = model.id.clone();
            let on_click = self.on_click(cx, move |view, _, cx| {
                view.send(kind.action(&id), cx);
            });
            let style = if kind == crate::general::ModelButton::Stop {
                Kind::Secondary
            } else {
                Kind::Primary
            };
            button(("model-action", index), kind.label(), style, p)
                .on_click(move |event, window, cx| on_click(event, window, cx))
        });
        let choose = self.on_click(cx, move |view, _, cx| {
            let change = General::of(&view.model).choose_model(index);
            view.change(change, cx);
        });
        let progress = model.progress.map(|fill| {
            div()
                .mt(px(4.0))
                .h(px(4.0))
                .w_full()
                .rounded_full()
                .bg(p.line)
                .child(div().h_full().w(relative(fill)).rounded_full().bg(p.gold))
        });
        row()
            .child(
                div()
                    .id(("model", index))
                    .flex_1()
                    .min_w(px(0.0))
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .when(model.selectable && !model.selected, |chooser| {
                        chooser
                            .cursor_pointer()
                            .on_click(move |event, window, cx| choose(event, window, cx))
                    })
                    .child(marker)
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .flex()
                            .flex_col()
                            .gap(px(1.0))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(6.0))
                                    .child(div().text_color(colour).child(model.name.clone()))
                                    .when(model.beta, |name| name.child(tag("beta", p))),
                            )
                            .child(
                                div()
                                    .text_size(px(SMALL_SIZE))
                                    .text_color(p.quiet)
                                    .child(model.status.clone()),
                            )
                            .children(progress),
                    ),
            )
            .children(action)
            .into_any_element()
    }
}

/// A tab's sections, one under the other.
pub(super) fn tab_column() -> Div {
    div().flex().flex_col().gap(px(22.0))
}

/// Rows in a box, divided by lines.
pub(super) fn list(p: &Palette, rows: impl IntoIterator<Item = AnyElement>) -> Div {
    div()
        .flex()
        .flex_col()
        .rounded(px(8.0))
        .border_1()
        .border_color(p.line)
        .bg(p.surface)
        .overflow_hidden()
        .children(rows.into_iter().enumerate().map(|(index, row)| {
            div()
                .when(index > 0, |divided| {
                    divided.border_t_1().border_color(p.line)
                })
                .child(row)
        }))
}

pub(super) fn row() -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(6.0))
        .min_h(px(44.0))
        .px(px(12.0))
        .py(px(6.0))
}

fn note(text: &str, p: &Palette) -> AnyElement {
    div()
        .text_size(px(controls::SMALL_SIZE))
        .text_color(p.quiet)
        .child(text.to_owned())
        .into_any_element()
}
