//! Generelt: the hold key, bokmål or nynorsk, the microphones in order with the pin, the talemodell with
//! what of it is downloaded, «Overleggets utseende», copying to the clipboard, starting at login and the
//! sounds (docs/design.md, Settings and Microphones).

use skald_core::config::{HoldKey, Language, OverlayTheme};
use skald_models::{Availability, MODELS, Status};

use crate::action::{Action, Change};
use crate::model::{ModelState, SettingsModel};
use crate::words;

#[derive(Debug, Clone, PartialEq)]
pub struct General {
    pub hold_key: HoldKeyField,
    pub language: Choices<Language>,
    /// The order first, then connected microphones that are not in it, then a pinned one that is neither.
    pub microphones: Vec<MicrophoneRow>,
    /// A microphone is pinned, so «Bruk den første som er koblet til» has something to undo.
    pub pinned: bool,
    pub models: Vec<ModelRow>,
    pub overlay_theme: Choices<OverlayTheme>,
    pub copy_to_clipboard: bool,
    pub start_at_login: bool,
    pub sounds: bool,
}

/// «Tasten du holder inne for å snakke»: the key set, and how another is chosen (D-61).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HoldKeyField {
    /// The key set as the field shows it: «Høyre ⌘», «F13».
    pub label: String,
    /// The same as a sentence names it: «høyre ⌘», «F13».
    pub name: String,
    /// What to know before holding the key set: one Norwegian text needs, one shortcuts use.
    pub warning: Option<String>,
    /// A key a newer Skald wrote, kept as written; Skald listens for the system's own meanwhile.
    pub unknown: Option<String>,
    pub choosing: Choosing,
}

/// How a hold key is chosen on this system.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Choosing {
    /// The user presses the key in the field; `waiting` while the field waits for it.
    Pressed { waiting: bool },
    /// The desktop's key binding holds the key, as on Linux: the two lines that bind the key set, or none
    /// for a key the desktop cannot bind that way.
    Bound(Option<[String; 2]>),
}

/// What the field shows while it waits for the key.
pub const PRESS_A_KEY: &str = "Trykk tasten du vil holde inne …";

/// The keys the field takes, under it while it waits.
pub const KEYS_TAKEN: &str = "En modifikatortast alene, som høyre ⌘, ⇧, ⌥ eller fn, eller en funksjonstast \
     fra F1 til F19. Esc avbryter.";

impl HoldKeyField {
    fn of(key: &HoldKey, waiting: bool) -> Self {
        Self {
            label: key.label(),
            name: key.name(),
            warning: key.warning(),
            unknown: (!key.is_known()).then(|| String::from(key.clone())),
            choosing: if cfg!(target_os = "linux") {
                Choosing::Bound(hyprland_binding(key))
            } else {
                Choosing::Pressed { waiting }
            },
        }
    }

    /// What the field's button asks: to wait for a key, or to stop waiting.
    pub fn button(&self) -> Option<Action> {
        match self.choosing {
            Choosing::Pressed { waiting } => Some(Action::CaptureHoldKey(!waiting)),
            Choosing::Bound(_) => None,
        }
    }
}

/// The Hyprland lines that hold `key` for Skald: `skald start` as it goes down, `skald stop` as it comes up.
/// A modifier coming up still carries its own kind, so the release line names it. Alt Gr and fn are left
/// out: a Norwegian layout makes Alt Gr a level shift, and fn never reaches the desktop.
pub fn hyprland_binding(key: &HoldKey) -> Option<[String; 2]> {
    let (name, kind) = match key {
        HoldKey::RightControl => ("Control_R".to_owned(), "CTRL"),
        HoldKey::LeftControl => ("Control_L".to_owned(), "CTRL"),
        HoldKey::RightShift => ("Shift_R".to_owned(), "SHIFT"),
        HoldKey::LeftShift => ("Shift_L".to_owned(), "SHIFT"),
        HoldKey::LeftOption => ("Alt_L".to_owned(), "ALT"),
        HoldKey::RightCommand => ("Super_R".to_owned(), "SUPER"),
        HoldKey::LeftCommand => ("Super_L".to_owned(), "SUPER"),
        HoldKey::Function(number) => (format!("F{number}"), ""),
        HoldKey::RightOption | HoldKey::Fn | HoldKey::Unknown(_) => return None,
    };
    Some([
        format!("bindn = , {name}, exec, skald start"),
        format!("bindrn = {kind}, {name}, exec, skald stop"),
    ])
}

/// One setting's options, as a row of buttons.
#[derive(Debug, Clone, PartialEq)]
pub struct Choices<T> {
    pub options: Vec<Choice<T>>,
    /// A value set that none of the options is, as one a newer Skald wrote, kept as written. Where Skald
    /// acts on an option in its place, that option is selected.
    pub unknown: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Choice<T> {
    pub value: T,
    pub label: &'static str,
    pub selected: bool,
}

impl<T: Clone> Choices<T> {
    /// The change choosing an option makes; `None` when it is the value already set.
    pub fn choose(&self, index: usize, change: impl Fn(T) -> Change) -> Option<Change> {
        let choice = self.options.get(index)?;
        if choice.selected && self.unknown.is_none() {
            return None;
        }
        Some(change(choice.value.clone()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MicrophoneRow {
    pub name: String,
    pub connected: bool,
    pub in_use: bool,
    pub pinned: bool,
    /// In the saved order; a connected microphone that is not joins it the first time the order is moved.
    pub in_order: bool,
    pub can_move_up: bool,
    pub can_move_down: bool,
    /// An unplugged microphone can be taken out of the order.
    pub can_remove: bool,
}

impl MicrophoneRow {
    /// What the row says after the name, as `skald mic list` says it.
    pub fn details(&self) -> String {
        let mut parts = vec![if self.connected {
            "koblet til"
        } else {
            "ikke koblet til"
        }];
        if self.in_use {
            parts.push("brukes nå");
        }
        if !self.in_order && self.connected {
            parts.push("ikke i rekkefølgen");
        }
        parts.join(" · ")
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModelRow {
    pub id: String,
    pub name: String,
    pub beta: bool,
    pub selected: bool,
    /// Only a model that is downloaded and checked can be chosen, as with `skald model use`.
    pub selectable: bool,
    /// Listed greyed, until its maker opens it to everyone.
    pub greyed: bool,
    pub status: String,
    /// From 0 to 1 while a download runs.
    pub progress: Option<f32>,
    pub button: Option<ModelButton>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelButton {
    Download,
    Resume,
    /// A file of the right size is there but was never checked.
    Check,
    Stop,
}

impl ModelButton {
    pub fn label(self) -> &'static str {
        match self {
            Self::Download => "Last ned",
            Self::Resume => "Fortsett",
            Self::Check => "Sjekk filen",
            Self::Stop => "Stopp",
        }
    }

    pub fn action(self, model_id: &str) -> Action {
        match self {
            Self::Download | Self::Resume | Self::Check => {
                Action::DownloadModel(model_id.to_owned())
            }
            Self::Stop => Action::StopDownload(model_id.to_owned()),
        }
    }
}

impl General {
    pub fn of(model: &SettingsModel) -> Self {
        let config = &model.config;
        Self {
            hold_key: HoldKeyField::of(&config.hold_key, model.capturing_hold_key),
            language: Choices {
                options: [(Language::Bokmal, "Bokmål"), (Language::Nynorsk, "Nynorsk")]
                    .into_iter()
                    .map(|(value, label)| Choice {
                        selected: value == config.language,
                        value,
                        label,
                    })
                    .collect(),
                unknown: None,
            },
            microphones: microphones(model),
            pinned: config.pinned_microphone.is_some(),
            models: models(model),
            overlay_theme: overlay_themes(&config.overlay_theme),
            copy_to_clipboard: config.copy_to_clipboard,
            start_at_login: config.start_at_login,
            sounds: config.sounds,
        }
    }

    /// Moves the microphone one place up or down. A connected microphone that was not in the order joins
    /// it where it stands in the list.
    pub fn move_microphone(&self, index: usize, up: bool) -> Option<Change> {
        let row = self.microphones.get(index)?;
        if !(if up {
            row.can_move_up
        } else {
            row.can_move_down
        }) {
            return None;
        }
        let mut order: Vec<&str> = self
            .microphones
            .iter()
            .filter(|row| row.in_order || row.connected)
            .map(|row| row.name.as_str())
            .collect();
        let at = order.iter().position(|name| *name == row.name)?;
        let to = if up { at.checked_sub(1)? } else { at + 1 };
        if to >= order.len() {
            return None;
        }
        order.swap(at, to);
        Some(Change::MicrophoneOrder(
            order.into_iter().map(str::to_owned).collect(),
        ))
    }

    pub fn remove_microphone(&self, index: usize) -> Option<Change> {
        let row = self.microphones.get(index)?;
        if !row.can_remove {
            return None;
        }
        Some(Change::MicrophoneOrder(
            self.microphones
                .iter()
                .filter(|other| other.in_order && other.name != row.name)
                .map(|other| other.name.clone())
                .collect(),
        ))
    }

    /// «Bruk denne»: puts the microphone ahead of the order, as choosing it in the menu does.
    pub fn pin_microphone(&self, index: usize) -> Option<Change> {
        let row = self.microphones.get(index)?;
        (!row.pinned).then(|| Change::PinMicrophone(Some(row.name.clone())))
    }

    /// «Bruk den første som er koblet til».
    pub fn unpin_microphone(&self) -> Option<Change> {
        self.pinned.then_some(Change::PinMicrophone(None))
    }

    pub fn choose_model(&self, index: usize) -> Option<Change> {
        let row = self.models.get(index)?;
        (row.selectable && !row.selected).then(|| Change::Model(row.id.clone()))
    }
}

fn overlay_themes(current: &OverlayTheme) -> Choices<OverlayTheme> {
    // A theme from a newer Skald is drawn as the default, so the default shows as chosen.
    let drawn = match current {
        OverlayTheme::Unknown(_) => &OverlayTheme::Runepinne,
        known => known,
    };
    Choices {
        options: [
            (OverlayTheme::Runepinne, "Runepinne (standard)"),
            (OverlayTheme::SmiddJern, "Smidd jern"),
            (OverlayTheme::Kjedelig, "Kjedelig"),
        ]
        .into_iter()
        .map(|(value, label)| Choice {
            selected: &value == drawn,
            value,
            label,
        })
        .collect(),
        unknown: match current {
            OverlayTheme::Unknown(name) => Some(name.clone()),
            _ => None,
        },
    }
}

fn microphones(model: &SettingsModel) -> Vec<MicrophoneRow> {
    let config = &model.config;
    let connected = &model.microphones.connected;
    let pinned = config.pinned_microphone.as_deref();
    let mut names: Vec<(&str, bool)> = Vec::new();
    for name in &config.microphones {
        if names.iter().all(|(known, _)| known != name) {
            names.push((name, true));
        }
    }
    for name in connected {
        if names.iter().all(|(known, _)| known != name) {
            names.push((name, false));
        }
    }
    if let Some(name) = pinned
        && names.iter().all(|(known, _)| *known != name)
    {
        names.push((name, false));
    }

    let movable: Vec<usize> = names
        .iter()
        .enumerate()
        .filter(|(_, (name, in_order))| *in_order || connected.iter().any(|c| c == name))
        .map(|(index, _)| index)
        .collect();
    names
        .iter()
        .enumerate()
        .map(|(index, (name, in_order))| {
            let is_connected = connected.iter().any(|c| c == name);
            let place = movable.iter().position(|&m| m == index);
            MicrophoneRow {
                name: (*name).to_owned(),
                connected: is_connected,
                in_use: model.microphones.in_use.as_deref() == Some(*name),
                pinned: pinned == Some(*name),
                in_order: *in_order,
                can_move_up: place.is_some_and(|place| place > 0),
                can_move_down: place.is_some_and(|place| place + 1 < movable.len()),
                can_remove: *in_order && !is_connected,
            }
        })
        .collect()
}

fn models(model: &SettingsModel) -> Vec<ModelRow> {
    let chosen = model.config.model.as_str();
    let mut rows: Vec<ModelRow> = MODELS
        .iter()
        .map(|entry| {
            let selected = entry.id == chosen;
            let base = ModelRow {
                id: entry.id.to_owned(),
                name: entry.name.to_owned(),
                beta: entry.beta,
                selected,
                selectable: false,
                greyed: false,
                status: String::new(),
                progress: None,
                button: None,
            };
            match &entry.availability {
                Availability::NotHosted => ModelRow {
                    status: "Kan ikke lastes ned ennå".to_owned(),
                    ..base
                },
                Availability::NotOpen => ModelRow {
                    status: "Ikke åpnet for alle ennå".to_owned(),
                    greyed: true,
                    ..base
                },
                Availability::Download(file) => {
                    let state = model
                        .models
                        .get(entry.id)
                        .copied()
                        .unwrap_or(ModelState::OnDisk(Status::Missing));
                    downloadable(base, state, file.size)
                }
            }
        })
        .collect();
    if MODELS.iter().all(|entry| entry.id != chosen) {
        rows.push(ModelRow {
            id: chosen.to_owned(),
            name: chosen.to_owned(),
            beta: false,
            selected: true,
            selectable: false,
            greyed: false,
            status: "Ukjent modell, fra en nyere Skald".to_owned(),
            progress: None,
            button: None,
        });
    }
    rows
}

fn downloadable(row: ModelRow, state: ModelState, size: u64) -> ModelRow {
    let (status, button, progress) = match state {
        ModelState::OnDisk(Status::Ready) => ("Lastet ned".to_owned(), None, None),
        ModelState::OnDisk(Status::Missing) => (
            format!("Ikke lastet ned · {}", words::size(size)),
            Some(ModelButton::Download),
            None,
        ),
        ModelState::OnDisk(Status::Partial { received }) => (
            format!("Stoppet ved {}", words::part_of(received, size)),
            Some(ModelButton::Resume),
            None,
        ),
        ModelState::OnDisk(Status::Unchecked) => (
            "Filen er her, men ikke sjekket".to_owned(),
            Some(ModelButton::Check),
            None,
        ),
        ModelState::OnDisk(Status::Other { .. }) => (
            format!("En annen fil ligger her · {}", words::size(size)),
            Some(ModelButton::Download),
            None,
        ),
        ModelState::Downloading { received } if received >= size => (
            "Sjekker filen …".to_owned(),
            Some(ModelButton::Stop),
            Some(1.0),
        ),
        ModelState::Downloading { received } => (
            format!("Laster ned {}", words::part_of(received, size)),
            Some(ModelButton::Stop),
            Some(received as f32 / size.max(1) as f32),
        ),
    };
    ModelRow {
        selectable: state == ModelState::OnDisk(Status::Ready),
        status,
        button,
        progress,
        ..row
    }
}

#[cfg(test)]
mod tests {
    use skald_core::Config;

    use super::*;
    use crate::model::Microphones;

    fn model(edit: impl FnOnce(&mut Config)) -> SettingsModel {
        let mut config = Config::default();
        edit(&mut config);
        SettingsModel {
            config,
            ..SettingsModel::default()
        }
    }

    fn with_microphones(order: &[&str], connected: &[&str], pinned: Option<&str>) -> SettingsModel {
        let mut settings = model(|config| {
            config.microphones = order.iter().map(|name| name.to_string()).collect();
            config.pinned_microphone = pinned.map(str::to_owned);
        });
        SettingsModel {
            config: std::mem::take(&mut settings.config),
            microphones: Microphones {
                connected: connected.iter().map(|name| name.to_string()).collect(),
                in_use: connected.first().map(|name| name.to_string()),
            },
            ..SettingsModel::default()
        }
    }

    #[test]
    fn shows_the_settings_as_they_are() {
        let general = General::of(&model(|config| {
            config.language = Language::Nynorsk;
            config.copy_to_clipboard = true;
            config.sounds = false;
            config.overlay_theme = OverlayTheme::Kjedelig;
        }));
        let selected: Vec<&str> = general
            .language
            .options
            .iter()
            .filter(|choice| choice.selected)
            .map(|choice| choice.label)
            .collect();
        assert_eq!(selected, ["Nynorsk"]);
        assert!(general.copy_to_clipboard);
        assert!(!general.start_at_login);
        assert!(!general.sounds);
        let themes: Vec<(&str, bool)> = general
            .overlay_theme
            .options
            .iter()
            .map(|choice| (choice.label, choice.selected))
            .collect();
        assert_eq!(
            themes,
            [
                ("Runepinne (standard)", false),
                ("Smidd jern", false),
                ("Kjedelig", true)
            ]
        );
    }

    #[test]
    fn choosing_the_value_already_set_changes_nothing() {
        let general = General::of(&model(|_| {}));
        assert_eq!(general.language.choose(0, Change::Language), None);
        assert_eq!(
            general.language.choose(1, Change::Language),
            Some(Change::Language(Language::Nynorsk))
        );
        assert_eq!(
            general.overlay_theme.choose(1, Change::OverlayTheme),
            Some(Change::OverlayTheme(OverlayTheme::SmiddJern))
        );
    }

    #[test]
    fn a_theme_from_a_newer_skald_shows_the_default_and_can_be_chosen_away() {
        let general = General::of(&model(|config| {
            config.overlay_theme = OverlayTheme::Unknown("nordlys".into());
        }));
        assert_eq!(general.overlay_theme.unknown.as_deref(), Some("nordlys"));
        assert!(general.overlay_theme.options[0].selected);
        assert_eq!(
            general.overlay_theme.choose(0, Change::OverlayTheme),
            Some(Change::OverlayTheme(OverlayTheme::Runepinne)),
            "choosing the default writes it in place of the unknown theme"
        );
    }

    #[test]
    fn the_field_names_the_key_set_and_warns_of_one_text_or_shortcuts_need() {
        let general = General::of(&model(|_| {}));
        assert_eq!(general.hold_key.label, HoldKey::default().label());
        assert_eq!(general.hold_key.warning, None);
        assert_eq!(general.hold_key.unknown, None);

        let f13 = General::of(&model(|config| config.hold_key = HoldKey::Function(13)));
        assert_eq!(f13.hold_key.label, "F13");
        assert_eq!(f13.hold_key.name, "F13");

        let right_option = General::of(&model(|config| config.hold_key = HoldKey::RightOption));
        assert!(
            right_option
                .hold_key
                .warning
                .unwrap()
                .contains("@, [ ], { } og |")
        );
        let left_shift = General::of(&model(|config| config.hold_key = HoldKey::LeftShift));
        assert!(left_shift.hold_key.warning.unwrap().contains("snarveier"));

        let unknown = General::of(&model(|config| {
            config.hold_key = HoldKey::Unknown("right-hyper".into());
        }));
        assert_eq!(unknown.hold_key.unknown.as_deref(), Some("right-hyper"));
        assert_eq!(unknown.hold_key.label, HoldKey::default().label());
    }

    #[test]
    fn the_key_is_pressed_in_the_field_except_where_the_desktop_binds_it() {
        let general = General::of(&model(|_| {}));
        if cfg!(target_os = "linux") {
            assert!(matches!(
                general.hold_key.choosing,
                Choosing::Bound(Some(_))
            ));
            assert_eq!(general.hold_key.button(), None);
            return;
        }
        assert_eq!(
            general.hold_key.choosing,
            Choosing::Pressed { waiting: false }
        );
        assert_eq!(
            general.hold_key.button(),
            Some(Action::CaptureHoldKey(true))
        );
        let mut waiting = model(|_| {});
        waiting.capturing_hold_key = true;
        let general = General::of(&waiting);
        assert_eq!(
            general.hold_key.choosing,
            Choosing::Pressed { waiting: true }
        );
        assert_eq!(
            general.hold_key.button(),
            Some(Action::CaptureHoldKey(false)),
            "the button gives up waiting"
        );
    }

    #[test]
    fn hyprland_binds_the_key_with_its_release_carrying_its_kind() {
        assert_eq!(
            hyprland_binding(&HoldKey::RightControl),
            Some([
                "bindn = , Control_R, exec, skald start".to_owned(),
                "bindrn = CTRL, Control_R, exec, skald stop".to_owned(),
            ]),
            "the lines in packaging/linux"
        );
        assert_eq!(
            hyprland_binding(&HoldKey::Function(13)),
            Some([
                "bindn = , F13, exec, skald start".to_owned(),
                "bindrn = , F13, exec, skald stop".to_owned(),
            ])
        );
        assert_eq!(
            hyprland_binding(&HoldKey::LeftCommand).unwrap()[1],
            "bindrn = SUPER, Super_L, exec, skald stop"
        );
        assert_eq!(hyprland_binding(&HoldKey::RightOption), None);
        assert_eq!(hyprland_binding(&HoldKey::Fn), None);
    }

    #[test]
    fn lists_the_order_then_the_connected_then_an_unplugged_pin() {
        let general = General::of(&with_microphones(
            &["Shure MV7", "AirPods"],
            &["MacBook Pro-mikrofon", "Shure MV7"],
            Some("Yeti"),
        ));
        let rows: Vec<(&str, bool, bool, bool)> = general
            .microphones
            .iter()
            .map(|row| (row.name.as_str(), row.connected, row.in_order, row.pinned))
            .collect();
        assert_eq!(
            rows,
            [
                ("Shure MV7", true, true, false),
                ("AirPods", false, true, false),
                ("MacBook Pro-mikrofon", true, false, false),
                ("Yeti", false, false, true),
            ]
        );
        assert_eq!(general.microphones[0].details(), "koblet til");
        assert_eq!(general.microphones[1].details(), "ikke koblet til");
        assert_eq!(
            general.microphones[2].details(),
            "koblet til · brukes nå · ikke i rekkefølgen"
        );
        assert!(general.pinned);
        assert!(
            !general.microphones[3].can_move_up,
            "the pin alone has no place"
        );
        assert!(general.microphones[1].can_remove);
        assert!(!general.microphones[0].can_remove, "a connected one stays");
    }

    #[test]
    fn moving_a_microphone_writes_the_whole_order() {
        let general = General::of(&with_microphones(
            &["Shure MV7", "AirPods"],
            &["MacBook Pro-mikrofon", "Shure MV7"],
            None,
        ));
        assert_eq!(general.move_microphone(0, true), None);
        assert_eq!(
            general.move_microphone(0, false),
            Some(Change::MicrophoneOrder(vec![
                "AirPods".into(),
                "Shure MV7".into(),
                "MacBook Pro-mikrofon".into()
            ]))
        );
        assert_eq!(
            general.move_microphone(2, true),
            Some(Change::MicrophoneOrder(vec![
                "Shure MV7".into(),
                "MacBook Pro-mikrofon".into(),
                "AirPods".into()
            ])),
            "a connected microphone joins the order where it is moved to"
        );
        assert_eq!(general.move_microphone(2, false), None);
        assert_eq!(
            general.remove_microphone(1),
            Some(Change::MicrophoneOrder(vec!["Shure MV7".into()]))
        );
        assert_eq!(general.remove_microphone(0), None);
    }

    #[test]
    fn the_pin_is_set_from_a_row_and_removed_with_the_first_connected() {
        let general = General::of(&with_microphones(
            &["Shure MV7"],
            &["Shure MV7"],
            Some("Shure MV7"),
        ));
        assert_eq!(general.pin_microphone(0), None, "pinned already");
        assert_eq!(
            general.unpin_microphone(),
            Some(Change::PinMicrophone(None))
        );
        let general = General::of(&with_microphones(&["Shure MV7"], &["Shure MV7"], None));
        assert_eq!(
            general.pin_microphone(0),
            Some(Change::PinMicrophone(Some("Shure MV7".into())))
        );
        assert_eq!(general.unpin_microphone(), None);
    }

    #[test]
    fn each_model_shows_what_is_downloaded() {
        let mut settings = model(|_| {});
        let status = |settings: &SettingsModel| General::of(settings).models;

        let rows = status(&settings);
        let names: Vec<&str> = rows.iter().map(|row| row.name.as_str()).collect();
        assert_eq!(names, ["NB-Whisper Large", "NB-ASR Bokfink", "Nordlyd"]);
        assert_eq!(rows[0].status, "Ikke lastet ned · 3,1 GB");
        assert_eq!(rows[0].button, Some(ModelButton::Download));
        assert!(rows[0].selected && !rows[0].selectable);
        assert!(rows[1].beta && !rows[1].selectable && !rows[1].greyed);
        assert!(rows[2].greyed && !rows[2].selectable);

        settings.models.insert(
            "nb-whisper-large".into(),
            ModelState::Downloading {
                received: 1_200_000_000,
            },
        );
        let row = &status(&settings)[0];
        assert_eq!(row.status, "Laster ned 1,2 av 3,1 GB");
        assert_eq!(row.button, Some(ModelButton::Stop));
        assert!(row.progress.unwrap() > 0.38 && row.progress.unwrap() < 0.39);
        assert_eq!(
            row.button.unwrap().action(&row.id),
            Action::StopDownload("nb-whisper-large".into())
        );

        settings.models.insert(
            "nb-whisper-large".into(),
            ModelState::Downloading {
                received: 3_095_033_483,
            },
        );
        assert_eq!(status(&settings)[0].status, "Sjekker filen …");

        settings.models.insert(
            "nb-whisper-large".into(),
            ModelState::OnDisk(Status::Partial {
                received: 1_200_000_000,
            }),
        );
        let row = &status(&settings)[0];
        assert_eq!(row.status, "Stoppet ved 1,2 av 3,1 GB");
        assert_eq!(
            row.button.unwrap().action(&row.id),
            Action::DownloadModel("nb-whisper-large".into())
        );

        settings
            .models
            .insert("nb-whisper-large".into(), ModelState::OnDisk(Status::Ready));
        let row = &status(&settings)[0];
        assert_eq!(row.status, "Lastet ned");
        assert!(row.selectable && row.button.is_none());
    }

    #[test]
    fn only_a_downloaded_model_can_be_chosen() {
        let mut settings = model(|config| config.model = "nordlyd".into());
        assert_eq!(General::of(&settings).choose_model(0), None);
        settings
            .models
            .insert("nb-whisper-large".into(), ModelState::OnDisk(Status::Ready));
        let general = General::of(&settings);
        assert_eq!(
            general.choose_model(0),
            Some(Change::Model("nb-whisper-large".into()))
        );
        assert_eq!(general.choose_model(2), None);
    }

    #[test]
    fn a_model_from_a_newer_skald_is_listed_as_chosen() {
        let rows = General::of(&model(|config| config.model = "nordlyd-large".into())).models;
        let last = rows.last().unwrap();
        assert_eq!(last.id, "nordlyd-large");
        assert!(last.selected && !last.selectable);
    }
}
