//! The menu as data (docs/design.md, Menu): what it holds for a given [`TrayModel`], with its texts,
//! checks and shortcuts. Each platform draws these entries as they are; none of them decides what the
//! menu holds.

use skald_core::config::{HoldKey, Language};

use crate::model::{Action, TrayModel};

const MICROPHONE: &str = "Mikrofon";
const FIRST_CONNECTED: &str = "Bruk den første som er koblet til";
const LATEST_TEXTS: &str = "Siste tekster";
const WRITTEN_LANGUAGE: &str = "Skriftspråk";
const VOCABULARY: &str = "Ordskatt …";
const SETTINGS: &str = "Innstillinger …";
const QUIT: &str = "Avslutt Skald";

/// How many characters of a latest text its entry shows before it is cut with «…»; the whole text is
/// its tooltip and what it copies.
pub const TEXT_TITLE_CHARS: usize = 50;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entry {
    /// A line that does nothing when chosen: in the menu's text colour, or greyed.
    Label {
        title: String,
        greyed: bool,
    },
    Choice(Choice),
    /// Greyed when it holds nothing.
    Submenu {
        title: String,
        entries: Vec<Entry>,
    },
    Separator,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choice {
    pub title: String,
    pub action: Action,
    pub enabled: bool,
    /// A tick before the title.
    pub checked: bool,
    /// The microphone held ahead of the order (D-20), marked with a pin.
    pub pinned: bool,
    pub tooltip: Option<String>,
    /// The key that chooses it with ⌘ while the menu is open.
    pub shortcut: Option<char>,
}

impl Choice {
    fn new(title: impl Into<String>, action: Action) -> Self {
        Self {
            title: title.into(),
            action,
            enabled: true,
            checked: false,
            pinned: false,
            tooltip: None,
            shortcut: None,
        }
    }
}

impl TrayModel {
    /// The menu, top to bottom. It is the same whatever a take is doing; the icon shows that.
    pub fn menu(&self) -> Vec<Entry> {
        let heading = format!("Skald · {}", language_in_text(self.language));
        let mut menu = vec![Entry::Label {
            title: heading,
            greyed: false,
        }];
        if let Some(hint) = hint(&self.hold_key) {
            menu.push(Entry::Label {
                title: hint,
                greyed: true,
            });
        }
        menu.extend([
            Entry::Separator,
            Entry::Submenu {
                title: MICROPHONE.to_owned(),
                entries: self.microphone_entries(),
            },
            Entry::Submenu {
                title: LATEST_TEXTS.to_owned(),
                entries: self.text_entries(),
            },
            Entry::Submenu {
                title: WRITTEN_LANGUAGE.to_owned(),
                entries: self.language_entries(),
            },
            Entry::Separator,
            Entry::Choice(Choice::new(VOCABULARY, Action::OpenVocabulary)),
            Entry::Choice(Choice {
                shortcut: Some(','),
                ..Choice::new(SETTINGS, Action::OpenSettings)
            }),
            Entry::Separator,
            Entry::Choice(Choice {
                shortcut: Some('q'),
                ..Choice::new(QUIT, Action::Quit)
            }),
        ]);
        menu
    }

    /// Each microphone, ticked where it is in use and pinned where it is pinned, then the line that
    /// removes the pin, ticked while there is none.
    fn microphone_entries(&self) -> Vec<Entry> {
        let mut entries: Vec<Entry> = self
            .microphones
            .iter()
            .map(|microphone| {
                Entry::Choice(Choice {
                    enabled: microphone.connected,
                    checked: self.in_use.as_ref() == Some(&microphone.id),
                    pinned: self.pinned.as_ref() == Some(&microphone.id),
                    ..Choice::new(
                        &microphone.name,
                        Action::PinMicrophone(microphone.id.clone()),
                    )
                })
            })
            .collect();
        if !entries.is_empty() {
            entries.push(Entry::Separator);
        }
        entries.push(Entry::Choice(Choice {
            checked: self.pinned.is_none(),
            ..Choice::new(FIRST_CONNECTED, Action::UnpinMicrophone)
        }));
        entries
    }

    fn text_entries(&self) -> Vec<Entry> {
        self.latest_texts
            .iter()
            .map(|text| {
                let title = text_title(text);
                Entry::Choice(Choice {
                    tooltip: (title != *text).then(|| text.clone()),
                    ..Choice::new(title, Action::CopyText(text.clone()))
                })
            })
            .collect()
    }

    fn language_entries(&self) -> Vec<Entry> {
        [(Language::Bokmal, "Bokmål"), (Language::Nynorsk, "Nynorsk")]
            .into_iter()
            .map(|(language, title)| {
                Entry::Choice(Choice {
                    checked: self.language == language,
                    ..Choice::new(title, Action::SetLanguage(language))
                })
            })
            .collect()
    }
}

/// The written standard as it reads inside a line of text.
fn language_in_text(language: Language) -> &'static str {
    match language {
        Language::Bokmal => "bokmål",
        Language::Nynorsk => "nynorsk",
    }
}

/// The greyed line under the heading, naming the key held as this system's keyboard labels it; none for
/// a key this version cannot name.
fn hint(key: &HoldKey) -> Option<String> {
    key.is_known()
        .then(|| format!("Hold {} for å snakke", key.name()))
}

/// A latest text on one line, cut at [`TEXT_TITLE_CHARS`] with «…».
fn text_title(text: &str) -> String {
    let line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.chars().count() <= TEXT_TITLE_CHARS {
        return line;
    }
    let mut cut: String = line.chars().take(TEXT_TITLE_CHARS - 1).collect();
    cut.truncate(cut.trim_end().len());
    cut.push('…');
    cut
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Activity, Microphone};

    fn label(title: &str, greyed: bool) -> Entry {
        Entry::Label {
            title: title.to_owned(),
            greyed,
        }
    }

    fn choice(title: &str, action: Action) -> Choice {
        Choice::new(title, action)
    }

    fn submenu<'a>(menu: &'a [Entry], title: &str) -> &'a [Entry] {
        menu.iter()
            .find_map(|entry| match entry {
                Entry::Submenu { title: t, entries } if t == title => Some(entries.as_slice()),
                _ => None,
            })
            .unwrap_or_else(|| panic!("no submenu {title:?}"))
    }

    fn choices(entries: &[Entry]) -> Vec<&Choice> {
        entries
            .iter()
            .filter_map(|entry| match entry {
                Entry::Choice(choice) => Some(choice),
                _ => None,
            })
            .collect()
    }

    fn microphone(id: &str, name: &str, connected: bool) -> Microphone {
        Microphone {
            id: id.to_owned(),
            name: name.to_owned(),
            connected,
        }
    }

    fn mac_model() -> TrayModel {
        TrayModel {
            hold_key: HoldKey::RightCommand,
            ..TrayModel::default()
        }
    }

    #[test]
    fn a_fresh_model_holds_the_menu_design_md_lists() {
        let menu = mac_model().menu();
        let hint = format!("Hold {} for å snakke", HoldKey::RightCommand.name());
        let expected = vec![
            label("Skald · bokmål", false),
            label(&hint, true),
            Entry::Separator,
            Entry::Submenu {
                title: "Mikrofon".to_owned(),
                entries: vec![Entry::Choice(Choice {
                    checked: true,
                    ..choice("Bruk den første som er koblet til", Action::UnpinMicrophone)
                })],
            },
            Entry::Submenu {
                title: "Siste tekster".to_owned(),
                entries: vec![],
            },
            Entry::Submenu {
                title: "Skriftspråk".to_owned(),
                entries: vec![
                    Entry::Choice(Choice {
                        checked: true,
                        ..choice("Bokmål", Action::SetLanguage(Language::Bokmal))
                    }),
                    Entry::Choice(choice("Nynorsk", Action::SetLanguage(Language::Nynorsk))),
                ],
            },
            Entry::Separator,
            Entry::Choice(choice("Ordskatt …", Action::OpenVocabulary)),
            Entry::Choice(Choice {
                shortcut: Some(','),
                ..choice("Innstillinger …", Action::OpenSettings)
            }),
            Entry::Separator,
            Entry::Choice(Choice {
                shortcut: Some('q'),
                ..choice("Avslutt Skald", Action::Quit)
            }),
        ];
        assert_eq!(menu, expected);
    }

    #[test]
    fn nynorsk_shows_in_the_heading_and_is_ticked() {
        let model = TrayModel {
            language: Language::Nynorsk,
            ..mac_model()
        };
        let menu = model.menu();
        assert_eq!(menu[0], label("Skald · nynorsk", false));
        let ticked: Vec<&str> = choices(submenu(&menu, "Skriftspråk"))
            .into_iter()
            .filter(|c| c.checked)
            .map(|c| c.title.as_str())
            .collect();
        assert_eq!(ticked, ["Nynorsk"]);
    }

    #[test]
    fn the_hint_names_the_key_held() {
        let control = TrayModel {
            hold_key: HoldKey::RightControl,
            ..TrayModel::default()
        };
        let control_hint = if cfg!(target_os = "macos") {
            "Hold høyre Control for å snakke"
        } else {
            "Hold høyre Ctrl for å snakke"
        };
        assert_eq!(control.menu()[1], label(control_hint, true));

        let f13 = TrayModel {
            hold_key: HoldKey::Function(13),
            ..TrayModel::default()
        };
        assert_eq!(f13.menu()[1], label("Hold F13 for å snakke", true));

        let unknown = TrayModel {
            hold_key: HoldKey::Unknown("f20".to_owned()),
            ..TrayModel::default()
        };
        let menu = unknown.menu();
        assert_eq!(menu[0], label("Skald · bokmål", false));
        assert_eq!(menu[1], Entry::Separator);
    }

    #[test]
    fn the_microphone_in_use_is_ticked_and_the_pinned_one_marked() {
        let model = TrayModel {
            microphones: vec![
                microphone("usb", "Shure MV7", true),
                microphone("built-in", "MacBook Pro-mikrofon", true),
            ],
            in_use: Some("usb".to_owned()),
            pinned: Some("usb".to_owned()),
            ..mac_model()
        };
        let menu = model.menu();
        let entries = submenu(&menu, "Mikrofon");
        assert_eq!(
            entries,
            [
                Entry::Choice(Choice {
                    checked: true,
                    pinned: true,
                    ..choice("Shure MV7", Action::PinMicrophone("usb".to_owned()))
                }),
                Entry::Choice(choice(
                    "MacBook Pro-mikrofon",
                    Action::PinMicrophone("built-in".to_owned()),
                )),
                Entry::Separator,
                Entry::Choice(choice(
                    "Bruk den første som er koblet til",
                    Action::UnpinMicrophone,
                )),
            ]
        );
    }

    #[test]
    fn a_pinned_microphone_that_is_unplugged_stays_listed_greyed_with_its_pin() {
        let model = TrayModel {
            microphones: vec![
                microphone("usb", "Shure MV7", false),
                microphone("built-in", "MacBook Pro-mikrofon", true),
            ],
            in_use: Some("built-in".to_owned()),
            pinned: Some("usb".to_owned()),
            ..mac_model()
        };
        let menu = model.menu();
        let listed = choices(submenu(&menu, "Mikrofon"));
        let shure = listed[0];
        assert!(!shure.enabled && shure.pinned && !shure.checked);
        let built_in = listed[1];
        assert!(built_in.enabled && !built_in.pinned && built_in.checked);
        let first_connected = listed[2];
        assert_eq!(first_connected.action, Action::UnpinMicrophone);
        assert!(!first_connected.checked);
    }

    #[test]
    fn without_a_pin_the_order_applies_and_says_so() {
        let model = TrayModel {
            microphones: vec![microphone("built-in", "MacBook Pro-mikrofon", true)],
            in_use: Some("built-in".to_owned()),
            ..mac_model()
        };
        let menu = model.menu();
        let listed = choices(submenu(&menu, "Mikrofon"));
        assert!(listed[0].checked && !listed[0].pinned);
        assert!(listed[1].checked);
    }

    #[test]
    fn latest_texts_keep_their_order_and_copy_the_whole_text() {
        let long = "Dette er en lang tekst som går langt forbi det menyen viser på én linje, \
                    og resten står i verktøytipset.";
        let model = TrayModel {
            latest_texts: vec![
                "Nyeste tekst.".to_owned(),
                "To linjer\nblir én.".to_owned(),
                long.to_owned(),
            ],
            ..mac_model()
        };
        let menu = model.menu();
        let texts = choices(submenu(&menu, "Siste tekster"));
        assert_eq!(texts.len(), 3);

        assert_eq!(texts[0].title, "Nyeste tekst.");
        assert_eq!(texts[0].tooltip, None);
        assert_eq!(
            texts[0].action,
            Action::CopyText("Nyeste tekst.".to_owned())
        );

        assert_eq!(texts[1].title, "To linjer blir én.");
        assert_eq!(texts[1].tooltip.as_deref(), Some("To linjer\nblir én."));
        assert_eq!(
            texts[1].action,
            Action::CopyText("To linjer\nblir én.".to_owned())
        );

        assert_eq!(texts[2].title.chars().count(), TEXT_TITLE_CHARS);
        assert!(texts[2].title.starts_with("Dette er en lang tekst"));
        assert!(texts[2].title.ends_with('…'));
        assert_eq!(texts[2].tooltip.as_deref(), Some(long));
        assert_eq!(texts[2].action, Action::CopyText(long.to_owned()));
    }

    #[test]
    fn a_text_is_cut_without_a_space_before_the_ellipsis() {
        let text = format!("{} {}", "a".repeat(TEXT_TITLE_CHARS - 2), "bcdef");
        let title = text_title(&text);
        assert_eq!(title, format!("{}…", "a".repeat(TEXT_TITLE_CHARS - 2)));
    }

    #[test]
    fn a_text_of_exactly_the_width_is_not_cut() {
        let text = "æ".repeat(TEXT_TITLE_CHARS);
        assert_eq!(text_title(&text), text);
    }

    #[test]
    fn what_a_take_is_doing_does_not_change_the_menu() {
        let idle = mac_model().menu();
        for activity in [Activity::Recording, Activity::Transcribing] {
            let model = TrayModel {
                activity,
                ..mac_model()
            };
            assert_eq!(model.menu(), idle);
        }
    }
}
