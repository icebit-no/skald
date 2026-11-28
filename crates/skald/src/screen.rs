//! The app's screen as data: what the overlay is asked to show and for which take, what the menu bar item
//! shows, and what the settings window is handed. The GPUI side (`shell`) only carries this out, so every
//! rule here is tested without a screen.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::sync::Arc;

use skald_core::Config;
use skald_core::config::{HoldKey, OverlayTheme};
use skald_core::state::State;
use skald_firstrun::TestDictation;
use skald_models::MODELS;
use skald_settings::{Change, ModelState, SettingsModel, TokenCounter};
use skald_tray::{Action as TrayAction, Activity, Microphone, TrayModel};

use crate::events::{Ending, Event, MicrophoneList};
use crate::say::Line;

/// One call to skald-overlay's `Overlay`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Pill {
    Record { hands_free: bool },
    Level(f32),
    Release,
    Inserted(String),
    HeardNothing(String),
    NotInserted,
    AdministratorWindow,
    TranscriptionFailed,
    NoMicrophone,
    MicrophoneNotOpened(String),
    MicrophoneChanged(String),
    Cancel,
}

/// What an event changed besides the overlay.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct Changed {
    pub(crate) pills: Vec<Pill>,
    /// The overlay's theme.
    pub(crate) theme: bool,
    /// The key held to talk, which the overlay's hint names.
    pub(crate) hold_key: bool,
    /// Anything the settings window shows.
    pub(crate) settings: bool,
}

#[derive(Debug)]
pub(crate) struct Screen {
    config: Arc<Config>,
    state: State,
    microphones: MicrophoneList,
    model_loaded: bool,
    /// What the loaded model computes on, which Om shows.
    compute: Option<skald_engine::Compute>,
    /// The take the overlay shows: the latest that started, until it ends.
    on_screen: Option<u64>,
    recording: bool,
    /// Takes ended and waiting for their text.
    carving: usize,
}

impl Default for Screen {
    fn default() -> Self {
        Self {
            config: Arc::new(Config::default()),
            state: State::default(),
            microphones: MicrophoneList::default(),
            model_loaded: false,
            compute: None,
            on_screen: None,
            recording: false,
            carving: 0,
        }
    }
}

impl Screen {
    pub(crate) fn theme(&self) -> &OverlayTheme {
        &self.config.overlay_theme
    }

    pub(crate) fn hold_key(&self) -> &HoldKey {
        &self.config.hold_key
    }

    pub(crate) fn apply(&mut self, event: Event) -> Changed {
        let mut changed = Changed::default();
        match event {
            Event::Recording { take, hands_free } => {
                self.on_screen = Some(take);
                self.recording = true;
                changed.pills.push(Pill::Record { hands_free });
            }
            Event::Level(dbfs) => {
                if self.recording {
                    changed.pills.push(Pill::Level(dbfs));
                }
            }
            Event::Carving { take } => {
                self.recording = false;
                self.carving += 1;
                if self.on_screen == Some(take) {
                    changed.pills.push(Pill::Release);
                }
            }
            Event::Cancelled { take } => {
                self.recording = false;
                if self.on_screen.take_if(|shown| *shown == take).is_some() {
                    changed.pills.push(Pill::Cancel);
                }
            }
            Event::NoMicrophone => {
                self.on_screen = None;
                changed.pills.push(Pill::NoMicrophone);
            }
            Event::MicrophoneNotOpened { microphone, .. } => {
                self.on_screen = None;
                changed.pills.push(Pill::MicrophoneNotOpened(microphone));
            }
            Event::Ended { take, ending } => {
                self.carving = self.carving.saturating_sub(1);
                if self.on_screen.take_if(|shown| *shown == take).is_some() {
                    changed.pills.push(match ending {
                        Ending::Inserted(text) => Pill::Inserted(text),
                        Ending::HeardNothing { microphone } => Pill::HeardNothing(microphone),
                        Ending::NotInserted(_) => Pill::NotInserted,
                        Ending::AdministratorWindow(_) => Pill::AdministratorWindow,
                        Ending::Failed(_) => Pill::TranscriptionFailed,
                    });
                }
            }
            Event::MicrophoneChanged(name) => changed.pills.push(Pill::MicrophoneChanged(name)),
            Event::Microphones(list) => {
                changed.settings = list != self.microphones;
                self.microphones = list;
            }
            Event::State(state) => {
                changed.settings = state != self.state;
                self.state = state;
            }
            Event::ModelLoaded(compute) => {
                self.model_loaded = true;
                self.compute = Some(compute);
                changed.settings = true;
            }
            Event::Settings(config) => return self.set_config(config),
            Event::Stopped => {}
        }
        changed
    }

    /// Takes the settings in use, as read by the loop or as just written by the app.
    pub(crate) fn set_config(&mut self, config: Arc<Config>) -> Changed {
        let changed = Changed {
            pills: Vec::new(),
            theme: config.overlay_theme != self.config.overlay_theme,
            hold_key: config.hold_key != self.config.hold_key,
            settings: config != self.config,
        };
        self.config = config;
        changed
    }

    pub(crate) fn tray(&self) -> TrayModel {
        let pinned = self
            .config
            .pinned_microphone
            .clone()
            .filter(|name| !name.trim().is_empty());
        let mut microphones: Vec<Microphone> = self
            .microphones
            .connected
            .iter()
            .map(|name| Microphone {
                id: name.clone(),
                name: name.clone(),
                connected: true,
            })
            .collect();
        if let Some(name) = pinned
            .as_ref()
            .filter(|name| !self.microphones.connected.contains(name))
        {
            microphones.push(Microphone {
                id: name.clone(),
                name: name.clone(),
                connected: false,
            });
        }
        TrayModel {
            activity: if self.recording {
                Activity::Recording
            } else if self.carving > 0 {
                Activity::Transcribing
            } else {
                Activity::Idle
            },
            hold_key: self.config.hold_key.clone(),
            language: self.config.language,
            microphones,
            in_use: self.microphones.in_use.clone(),
            pinned,
            latest_texts: self
                .state
                .history
                .entries()
                .iter()
                .map(|entry| entry.text.clone())
                .collect(),
        }
    }

    /// What the settings window shows, with what of each model is on disk or arriving, and the meter's
    /// counter while a model is loaded to count with.
    pub(crate) fn settings(
        &self,
        models: BTreeMap<String, ModelState>,
        tokens: Option<TokenCounter>,
    ) -> SettingsModel {
        SettingsModel {
            config: (*self.config).clone(),
            state: self.state.clone(),
            microphones: skald_settings::Microphones {
                connected: self.microphones.connected.clone(),
                in_use: self.microphones.in_use.clone(),
            },
            models,
            tokens: tokens.filter(|_| self.model_loaded),
            version: env!("CARGO_PKG_VERSION").to_owned(),
            compute: self.compute.map(|compute| match compute {
                skald_engine::Compute::Metal => skald_settings::Compute::Metal,
                skald_engine::Compute::Vulkan => skald_settings::Compute::Vulkan,
                skald_engine::Compute::Cpu => skald_settings::Compute::Cpu,
            }),
        }
    }
}

/// The change to the configuration a menu entry makes, the same one the settings window and `skald`
/// make; `None` for an entry that changes no setting.
pub(crate) fn change_for(action: &TrayAction) -> Option<Change> {
    match action {
        TrayAction::PinMicrophone(name) => Some(Change::PinMicrophone(Some(name.clone()))),
        TrayAction::UnpinMicrophone => Some(Change::PinMicrophone(None)),
        TrayAction::SetLanguage(language) => Some(Change::Language(*language)),
        TrayAction::CopyText(_)
        | TrayAction::OpenVocabulary
        | TrayAction::OpenSettings
        | TrayAction::Quit => None,
    }
}

/// What the first run's test shows after an event of the loop it started; `None` for an event that does
/// not change it.
pub(crate) fn test_dictation(event: &Event) -> Option<TestDictation> {
    Some(match event {
        Event::ModelLoaded(_) | Event::Cancelled { .. } => TestDictation::Listening,
        Event::Recording { .. } => TestDictation::Recording,
        Event::Carving { .. } => TestDictation::Transcribing,
        Event::NoMicrophone => TestDictation::Failed {
            reason: Line::NoMicrophone.to_string(),
        },
        Event::MicrophoneNotOpened { microphone, error } => TestDictation::Failed {
            reason: Line::CaptureFailed {
                microphone: microphone.clone(),
                error: error.clone(),
            }
            .to_string(),
        },
        Event::Ended { ending, .. } => match ending {
            Ending::Inserted(text)
            | Ending::NotInserted(text)
            | Ending::AdministratorWindow(text) => TestDictation::Heard { text: text.clone() },
            Ending::HeardNothing { microphone } => TestDictation::HeardNothing {
                microphone: microphone.clone(),
            },
            Ending::Failed(reason) => TestDictation::Failed {
                reason: Line::TranscriptionFailed(reason.clone()).to_string(),
            },
        },
        Event::Level(_)
        | Event::MicrophoneChanged(_)
        | Event::Microphones(_)
        | Event::State(_)
        | Event::Settings(_)
        | Event::Stopped => return None,
    })
}

/// What of each model Skald can download is on disk, or how much of it has arrived while it downloads.
pub(crate) fn model_states(
    models_dir: &Path,
    downloading: &HashMap<String, u64>,
) -> BTreeMap<String, ModelState> {
    MODELS
        .iter()
        .filter_map(|model| {
            let state = match downloading.get(model.id) {
                Some(&received) => ModelState::Downloading { received },
                None => ModelState::OnDisk(model.status(models_dir)?),
            };
            Some((model.id.to_owned(), state))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use skald_core::config::{HoldKey, Language};
    use skald_core::history::Entry;
    use skald_models::Status;

    use super::*;
    use crate::files::tests::Scratch;

    fn recording(take: u64) -> Event {
        Event::Recording {
            take,
            hands_free: false,
        }
    }

    fn pills(screen: &mut Screen, events: Vec<Event>) -> Vec<Pill> {
        events
            .into_iter()
            .flat_map(|event| screen.apply(event).pills)
            .collect()
    }

    #[test]
    fn a_take_drives_the_overlay_from_the_first_reading_to_its_text() {
        let mut screen = Screen::default();
        assert_eq!(
            pills(
                &mut screen,
                vec![
                    recording(1),
                    Event::Level(-30.0),
                    Event::Carving { take: 1 },
                    Event::Level(-20.0),
                    Event::Ended {
                        take: 1,
                        ending: Ending::Inserted("Hei.".into()),
                    },
                ]
            ),
            [
                Pill::Record { hands_free: false },
                Pill::Level(-30.0),
                Pill::Release,
                Pill::Inserted("Hei.".into()),
            ]
        );
    }

    #[test]
    fn a_hands_free_take_shows_hands_free_and_silence_names_the_microphone() {
        let mut screen = Screen::default();
        assert_eq!(
            pills(
                &mut screen,
                vec![
                    Event::Recording {
                        take: 4,
                        hands_free: true
                    },
                    Event::Carving { take: 4 },
                    Event::Ended {
                        take: 4,
                        ending: Ending::HeardNothing {
                            microphone: "Yeti".into()
                        },
                    },
                ]
            ),
            [
                Pill::Record { hands_free: true },
                Pill::Release,
                Pill::HeardNothing("Yeti".into()),
            ]
        );
    }

    #[test]
    fn an_earlier_take_s_end_leaves_the_take_on_screen_alone() {
        let mut screen = Screen::default();
        let shown = pills(
            &mut screen,
            vec![
                recording(1),
                Event::Carving { take: 1 },
                recording(2),
                Event::Ended {
                    take: 1,
                    ending: Ending::Inserted("Første.".into()),
                },
                Event::Level(-25.0),
                Event::Cancelled { take: 2 },
                Event::Ended {
                    take: 2,
                    ending: Ending::Failed("out of memory".into()),
                },
            ],
        );
        assert_eq!(
            shown,
            [
                Pill::Record { hands_free: false },
                Pill::Release,
                Pill::Record { hands_free: false },
                Pill::Level(-25.0),
                Pill::Cancel,
            ]
        );
    }

    #[test]
    fn a_text_not_inserted_or_a_failed_take_says_so_and_microphone_changes_always_show() {
        let mut screen = Screen::default();
        assert_eq!(
            pills(
                &mut screen,
                vec![
                    recording(1),
                    Event::Carving { take: 1 },
                    Event::Ended {
                        take: 1,
                        ending: Ending::NotInserted("Hei.".into()),
                    },
                    recording(2),
                    Event::Carving { take: 2 },
                    Event::Ended {
                        take: 2,
                        ending: Ending::Failed("out of memory".into()),
                    },
                    Event::MicrophoneChanged("Shure MV7".into()),
                    Event::Level(-20.0),
                ]
            ),
            [
                Pill::Record { hands_free: false },
                Pill::Release,
                Pill::NotInserted,
                Pill::Record { hands_free: false },
                Pill::Release,
                Pill::TranscriptionFailed,
                Pill::MicrophoneChanged("Shure MV7".into()),
            ]
        );
    }

    #[test]
    fn a_take_without_a_microphone_says_so_and_takes_the_pill_from_one_being_carved() {
        let mut screen = Screen::default();
        let shown = pills(
            &mut screen,
            vec![
                recording(1),
                Event::Carving { take: 1 },
                Event::NoMicrophone,
                Event::Ended {
                    take: 1,
                    ending: Ending::Inserted("Første.".into()),
                },
            ],
        );
        assert_eq!(
            shown,
            [
                Pill::Record { hands_free: false },
                Pill::Release,
                Pill::NoMicrophone,
            ]
        );
        assert_eq!(screen.tray().activity, Activity::Idle);
    }

    #[test]
    fn a_microphone_that_will_not_open_is_named_and_takes_the_pill_from_one_being_carved() {
        let mut screen = Screen::default();
        let shown = pills(
            &mut screen,
            vec![
                recording(1),
                Event::Carving { take: 1 },
                Event::MicrophoneNotOpened {
                    microphone: "Shure MV7".into(),
                    error: skald_audio::CaptureError::Busy,
                },
                Event::Ended {
                    take: 1,
                    ending: Ending::Inserted("Første.".into()),
                },
            ],
        );
        assert_eq!(
            shown,
            [
                Pill::Record { hands_free: false },
                Pill::Release,
                Pill::MicrophoneNotOpened("Shure MV7".into()),
            ]
        );
        assert_eq!(screen.tray().activity, Activity::Idle);
    }

    #[test]
    fn the_dot_is_red_while_recording_and_gold_while_any_take_waits_for_its_text() {
        let mut screen = Screen::default();
        let activity = |screen: &Screen| screen.tray().activity;
        assert_eq!(activity(&screen), Activity::Idle);
        screen.apply(recording(1));
        assert_eq!(activity(&screen), Activity::Recording);
        screen.apply(Event::Carving { take: 1 });
        assert_eq!(activity(&screen), Activity::Transcribing);
        screen.apply(recording(2));
        assert_eq!(activity(&screen), Activity::Recording);
        screen.apply(Event::Carving { take: 2 });
        screen.apply(Event::Ended {
            take: 1,
            ending: Ending::Failed("out of memory".into()),
        });
        assert_eq!(activity(&screen), Activity::Transcribing, "take 2 waits");
        screen.apply(Event::Ended {
            take: 2,
            ending: Ending::Failed("out of memory".into()),
        });
        assert_eq!(activity(&screen), Activity::Idle);
        screen.apply(recording(3));
        screen.apply(Event::Cancelled { take: 3 });
        assert_eq!(activity(&screen), Activity::Idle);
    }

    #[test]
    fn the_menu_lists_the_microphones_connected_and_an_unplugged_pin_last() {
        let mut screen = Screen::default();
        let mut config = Config::default();
        config.pinned_microphone = Some("AirPods".into());
        config.language = Language::Nynorsk;
        config.hold_key = HoldKey::RightControl;
        screen.apply(Event::Settings(Arc::new(config.clone())));
        screen.apply(Event::Microphones(MicrophoneList {
            connected: vec!["MacBook Pro-mikrofon".into(), "Shure MV7".into()],
            in_use: Some("Shure MV7".into()),
        }));
        let mut state = State::default();
        for text in ["Eldst.", "Nyest."] {
            state.history.push(Entry {
                text: text.into(),
                unix_ms: 1,
                microphone: "Shure MV7".into(),
            });
        }
        screen.apply(Event::State(state));

        let tray = screen.tray();
        let listed: Vec<(&str, bool)> = tray
            .microphones
            .iter()
            .map(|m| (m.name.as_str(), m.connected))
            .collect();
        assert_eq!(
            listed,
            [
                ("MacBook Pro-mikrofon", true),
                ("Shure MV7", true),
                ("AirPods", false)
            ]
        );
        assert!(tray.microphones.iter().all(|m| m.id == m.name));
        assert_eq!(tray.in_use.as_deref(), Some("Shure MV7"));
        assert_eq!(tray.pinned.as_deref(), Some("AirPods"));
        assert_eq!(tray.language, Language::Nynorsk);
        assert_eq!(tray.hold_key, HoldKey::RightControl);
        assert_eq!(tray.latest_texts, ["Nyest.", "Eldst."]);

        // Plugged in, the pinned microphone is listed once; a blank pin is none.
        config.pinned_microphone = Some("Shure MV7".into());
        screen.set_config(Arc::new(config.clone()));
        assert_eq!(screen.tray().microphones.len(), 2);
        config.pinned_microphone = Some(" ".into());
        screen.set_config(Arc::new(config));
        assert_eq!(screen.tray().pinned, None);
    }

    #[test]
    fn the_theme_and_the_settings_window_hear_only_of_their_own_changes() {
        let mut screen = Screen::default();
        let mut config = Config::default();
        config.sounds = false;
        let changed = screen.apply(Event::Settings(Arc::new(config.clone())));
        assert!(changed.settings);
        assert!(!changed.theme && !changed.hold_key);
        config.overlay_theme = OverlayTheme::Kjedelig;
        let changed = screen.set_config(Arc::new(config.clone()));
        assert!(changed.theme && changed.settings && !changed.hold_key);
        assert_eq!(screen.theme(), &OverlayTheme::Kjedelig);
        config.hold_key = HoldKey::Unknown("right-option".into());
        let changed = screen.set_config(Arc::new(config.clone()));
        assert!(changed.hold_key && changed.settings && !changed.theme);
        assert_eq!(screen.hold_key(), &HoldKey::Unknown("right-option".into()));
        assert_eq!(
            screen.set_config(Arc::new(config)),
            Changed::default(),
            "the same settings again"
        );

        assert!(!screen.apply(Event::Level(-20.0)).settings);
        assert!(!screen.apply(Event::State(State::default())).settings);
        assert!(
            screen
                .apply(Event::ModelLoaded(skald_engine::Compute::Metal))
                .settings
        );
    }

    #[test]
    fn the_settings_window_counts_tokens_only_with_a_model_loaded() {
        let mut screen = Screen::default();
        let counter = || Some(TokenCounter::new(223, |text: &str| text.len()));
        let model = screen.settings(BTreeMap::new(), counter());
        assert!(model.tokens.is_none());
        assert_eq!(model.version, env!("CARGO_PKG_VERSION"));
        screen.apply(Event::ModelLoaded(skald_engine::Compute::Metal));
        screen.apply(Event::Microphones(MicrophoneList {
            connected: vec!["Yeti".into()],
            in_use: Some("Yeti".into()),
        }));
        let model = screen.settings(BTreeMap::new(), counter());
        assert_eq!(model.tokens.unwrap().count("Skald"), 5);
        assert_eq!(model.microphones.in_use.as_deref(), Some("Yeti"));
    }

    #[test]
    fn menu_entries_make_the_same_changes_as_the_settings_window() {
        assert_eq!(
            change_for(&TrayAction::PinMicrophone("Yeti".into())),
            Some(Change::PinMicrophone(Some("Yeti".into())))
        );
        assert_eq!(
            change_for(&TrayAction::UnpinMicrophone),
            Some(Change::PinMicrophone(None))
        );
        assert_eq!(
            change_for(&TrayAction::SetLanguage(Language::Nynorsk)),
            Some(Change::Language(Language::Nynorsk))
        );
        for action in [
            TrayAction::CopyText("Hei.".into()),
            TrayAction::OpenVocabulary,
            TrayAction::OpenSettings,
            TrayAction::Quit,
        ] {
            assert_eq!(change_for(&action), None, "{action:?}");
        }
    }

    #[test]
    fn the_first_run_s_test_follows_the_take_from_the_key_to_its_text() {
        assert_eq!(
            test_dictation(&Event::ModelLoaded(skald_engine::Compute::Metal)),
            Some(TestDictation::Listening)
        );
        assert_eq!(
            test_dictation(&recording(1)),
            Some(TestDictation::Recording)
        );
        assert_eq!(
            test_dictation(&Event::Carving { take: 1 }),
            Some(TestDictation::Transcribing)
        );
        for ending in [
            Ending::Inserted("Hei, dette er en prøve.".into()),
            Ending::NotInserted("Hei, dette er en prøve.".into()),
        ] {
            assert_eq!(
                test_dictation(&Event::Ended { take: 1, ending }),
                Some(TestDictation::Heard {
                    text: "Hei, dette er en prøve.".into()
                }),
                "the window says whether the text reached its field"
            );
        }
        assert_eq!(
            test_dictation(&Event::Ended {
                take: 1,
                ending: Ending::HeardNothing {
                    microphone: "Yeti".into()
                }
            }),
            Some(TestDictation::HeardNothing {
                microphone: "Yeti".into()
            })
        );
        assert_eq!(
            test_dictation(&Event::Ended {
                take: 1,
                ending: Ending::Failed("out of memory".into())
            }),
            Some(TestDictation::Failed {
                reason: "Transkriberingen feilet: out of memory".into()
            })
        );
        assert_eq!(
            test_dictation(&Event::NoMicrophone),
            Some(TestDictation::Failed {
                reason: "Ingen mikrofon er koblet til.".into()
            })
        );
        assert_eq!(
            test_dictation(&Event::MicrophoneNotOpened {
                microphone: "Shure MV7".into(),
                error: skald_audio::CaptureError::Busy,
            }),
            Some(TestDictation::Failed {
                reason: "Kunne ikke åpne Shure MV7: et annet program bruker den.".into()
            })
        );
        assert_eq!(
            test_dictation(&Event::Cancelled { take: 1 }),
            Some(TestDictation::Listening)
        );
        assert_eq!(test_dictation(&Event::Level(-20.0)), None);
        assert_eq!(test_dictation(&Event::State(State::default())), None);
    }

    #[test]
    fn each_model_to_download_is_on_disk_or_arriving() {
        let scratch = Scratch::new("screen-models");
        let mut downloading = HashMap::new();
        let states = model_states(&scratch.0, &downloading);
        assert_eq!(
            states.get("nb-whisper-large"),
            Some(&ModelState::OnDisk(Status::Missing))
        );
        assert!(
            !states.contains_key("nordlyd"),
            "a model with nothing to download is left to the window"
        );
        downloading.insert("nb-whisper-large".to_owned(), 1_000);
        assert_eq!(
            model_states(&scratch.0, &downloading).get("nb-whisper-large"),
            Some(&ModelState::Downloading { received: 1_000 })
        );
    }
}
