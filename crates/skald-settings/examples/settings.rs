//! Opens the settings window with sample data and prints each action it sends. The example applies
//! them as the app would: a change goes into its configuration, «Last ned» runs a pretend download a
//! tenth of the model a second, and «Bytt» corrects the history entry. Nothing is written to disk and
//! nothing is fetched. Closing the window ends it.
//!
//! ```sh
//! cargo run -p skald-settings --example settings
//! cargo run -p skald-settings --example settings -- --tab ordskatt
//! ```
//!
//! `--tab generelt|ordskatt|historikk|om` opens that tab.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use gpui_kit::{App, QuitMode};
use skald_core::Config;
use skald_core::history::Entry;
use skald_core::state::State;
use skald_models::Status;
use skald_settings::{
    Action, Compute, Microphones, ModelState, SettingsModel, SettingsWindow, Tab, TokenCounter,
};

const WHISPER: &str = "nb-whisper-large";
const WHISPER_SIZE: u64 = 3_095_033_483;
const PROMPT_TOKENS: usize = 223;

fn main() {
    let arguments: Vec<String> = std::env::args().collect();
    let tab = match arguments
        .iter()
        .position(|argument| argument == "--tab")
        .and_then(|at| arguments.get(at + 1))
        .map(String::as_str)
    {
        Some("ordskatt") => Tab::Vocabulary,
        Some("historikk") => Tab::History,
        Some("om") => Tab::About,
        _ => Tab::General,
    };

    let application = gpui_kit::application().with_quit_mode(QuitMode::LastWindowClosed);
    application.run(move |cx: &mut App| {
        let app = Rc::new(Example {
            model: RefCell::new(sample()),
            window: RefCell::new(None),
            download: Cell::new(0),
        });
        let acting = app.clone();
        let window = SettingsWindow::new(move |action, cx| acting.act(action, cx), cx);
        app.window.replace(Some(window));
        if let Some(window) = app.window.borrow_mut().as_mut() {
            window.show(&app.model.borrow(), Some(tab), cx);
        }
    });
}

struct Example {
    model: RefCell<SettingsModel>,
    window: RefCell<Option<SettingsWindow>>,
    /// Counts the downloads started, so a stopped one's timer knows it is no longer wanted.
    download: Cell<u64>,
}

impl Example {
    fn act(self: &Rc<Self>, action: Action, cx: &mut App) {
        println!("{action:?}");
        match action {
            Action::Change(change) => {
                let changed = change.apply(&mut self.model.borrow_mut().config);
                println!(
                    "  {}",
                    if changed {
                        "config.toml would be written"
                    } else {
                        "nothing changed"
                    }
                );
            }
            Action::DownloadModel(id) => self.download(id, cx),
            Action::CaptureHoldKey(on) => {
                // The app's key listener would report the key pressed; here the field only shows that
                // it waits, until «Avbryt».
                self.model.borrow_mut().capturing_hold_key = on;
            }
            Action::StopDownload(id) => {
                self.download.set(self.download.get() + 1);
                let mut model = self.model.borrow_mut();
                if let Some(ModelState::Downloading { received }) = model.models.get(&id).copied() {
                    model
                        .models
                        .insert(id, ModelState::OnDisk(Status::Partial { received }));
                }
            }
            Action::CorrectHistory {
                unix_ms,
                before,
                after,
            } => {
                let replaced = self
                    .model
                    .borrow_mut()
                    .state
                    .history
                    .replace(unix_ms, &before, &after);
                if !replaced {
                    println!("  the entry has changed since");
                }
            }
        }
        self.show(cx);
    }

    fn show(&self, cx: &mut App) {
        if let Some(window) = self.window.borrow_mut().as_mut() {
            window.update(&self.model.borrow(), cx);
        }
    }

    /// Pretends to fetch the model, a tenth of it each second, then to check it.
    fn download(self: &Rc<Self>, id: String, cx: &mut App) {
        let started = self.download.get() + 1;
        self.download.set(started);
        let from = match self.model.borrow().models.get(&id) {
            Some(ModelState::OnDisk(Status::Partial { received })) => *received,
            _ => 0,
        };
        self.model
            .borrow_mut()
            .models
            .insert(id.clone(), ModelState::Downloading { received: from });
        let app = self.clone();
        cx.spawn(async move |cx| {
            let mut received = from;
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(250))
                    .await;
                if app.download.get() != started {
                    return;
                }
                let done = received >= WHISPER_SIZE;
                received = (received + WHISPER_SIZE / 40).min(WHISPER_SIZE);
                let state = if done {
                    ModelState::OnDisk(Status::Ready)
                } else {
                    ModelState::Downloading { received }
                };
                app.model.borrow_mut().models.insert(id.clone(), state);
                cx.update(|cx| app.show(cx));
                if done {
                    println!("  {id} downloaded and checked");
                    return;
                }
            }
        })
        .detach();
    }
}

fn sample() -> SettingsModel {
    let mut config = Config::default();
    config.microphones = vec!["Shure MV7".into(), "AirPods Pro".into()];
    config.vocabulary.sentence =
        "Ingrid og Sindre hentet ungene i barnehagen på Lade før kampen til Rosenborg.".into();
    for word in [
        "Skald",
        "Nasjonalbiblioteket",
        "Sindre",
        "Lade",
        "Rosenborg",
    ] {
        config.vocabulary.add_word(word);
    }
    for (from, to) in [
        ("Sin dre", "Sindre"),
        ("Rosen borg", "Rosenborg"),
        ("barne hagen", "barnehagen"),
    ] {
        config
            .corrections
            .add(from, to)
            .expect("the sample rules have words");
    }

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_millis() as i64);
    let mut state = State::default();
    let takes = [
        (42, "AirPods Pro", "Husk å kjøpe brød og melk på vei hjem."),
        (
            31,
            "Shure MV7",
            "Møtet er flyttet til torsdag klokka ti, og vi tar det på Teams.",
        ),
        (
            12,
            "Shure MV7",
            "Sin dre henter ungene i barne hagen etter jobb.",
        ),
        (3, "Shure MV7", "Kampen til Rosen borg starter klokka sju."),
        (0, "Shure MV7", "Kan du sende meg notatene fra møtet i går?"),
    ];
    for (minutes_ago, microphone, said) in takes {
        let text = config.corrections.apply_and_count(said, &mut state.counts);
        state.history.push(Entry {
            text,
            unix_ms: now - minutes_ago * 60_000,
            microphone: microphone.to_owned(),
        });
    }
    // A text the model got wrong that no rule covers yet, for «Bytt».
    state.history.push(Entry {
        text: "Skal vi merka den i dag eller vente til Skald er ferdig?".into(),
        unix_ms: now - 20_000,
        microphone: "Shure MV7".into(),
    });

    SettingsModel {
        config,
        state,
        microphones: Microphones {
            connected: vec!["MacBook Pro-mikrofon".into(), "Shure MV7".into()],
            in_use: Some("Shure MV7".into()),
        },
        models: [(
            WHISPER.to_owned(),
            ModelState::OnDisk(Status::Partial {
                received: 1_100_000_000,
            }),
        )]
        .into(),
        // A rough stand-in for the model's tokenizer: about four letters a token.
        tokens: Some(TokenCounter::new(PROMPT_TOKENS, |text: &str| {
            text.split_whitespace()
                .map(|word| 1 + word.chars().count() / 4)
                .sum()
        })),
        version: env!("CARGO_PKG_VERSION").to_owned(),
        compute: Some(Compute::Metal),
        capturing_hold_key: false,
    }
}
