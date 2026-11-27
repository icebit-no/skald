//! Opens the first-run window with pretend work behind it and prints each action it sends. Nothing is
//! downloaded, no permission is asked for and no microphone is opened: «Last ned» runs a pretend download
//! with a break in the connection along the way, «Gi tilgang» and «Åpne Systeminnstillinger» grant the
//! permission after a moment, and «Start prøven» runs a pretend take whose text is typed into the test's
//! field, standing in for the paste, so the clipboard is never touched. Closing the window ends it.
//!
//! ```sh
//! cargo run -p skald-firstrun --example firstrun
//! cargo run -p skald-firstrun --example firstrun -- --partial --denied
//! ```
//!
//! - `--partial`: an earlier download stopped at 40 %.
//! - `--unchecked`: a file of the right size is there, not yet checked.
//! - `--ready`: the model is downloaded and the permissions granted, so the window opens on the test.
//! - `--denied`: the microphone was refused, so only System Settings can turn it on.
//! - `--fail`: the first download ends with a file that does not match its SHA-256.
//! - `--offline`: the first download cannot connect.
//! - `--refuse`: the system's prompts are refused, so the permissions need System Settings.
//! - `--nothing`: the pretend take hears nothing.
//! - `--missed`: the pretend take's text does not reach the field.
//! - `--platform windows|linux`: the words and steps of another platform.
//! - `--speed <MB/s>`: how fast the pretend download goes; 120 by default.
//!
//! The window follows the system's light or dark mode as it switches.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui_kit::{App, AsyncApp, Keystroke, Modifiers, QuitMode};
use skald_core::config::HoldKey;
use skald_firstrun::{
    Access, Action, FirstRunModel, FirstRunWindow, ModelCard, Permission, PermissionChecks,
    Platform, TestDictation,
};
use skald_models::{Error, Event, Status};

const TICK: Duration = Duration::from_millis(100);

struct Options {
    status: Status,
    ready: bool,
    denied: bool,
    fail: bool,
    offline: bool,
    refuse: bool,
    nothing: bool,
    missed: bool,
    platform: Platform,
    bytes_per_tick: u64,
}

fn options() -> Options {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let has = |flag: &str| arguments.iter().any(|argument| argument == flag);
    let value = |flag: &str| {
        arguments
            .iter()
            .position(|argument| argument == flag)
            .and_then(|at| arguments.get(at + 1))
            .cloned()
    };
    let size = card().size;
    let status = if has("--ready") {
        Status::Ready
    } else if has("--unchecked") {
        Status::Unchecked
    } else if has("--partial") {
        Status::Partial {
            received: size * 2 / 5,
        }
    } else {
        Status::Missing
    };
    let megabytes: f64 = value("--speed")
        .and_then(|speed| speed.parse().ok())
        .unwrap_or(120.0);
    Options {
        status,
        ready: has("--ready"),
        denied: has("--denied"),
        fail: has("--fail"),
        offline: has("--offline"),
        refuse: has("--refuse"),
        nothing: has("--nothing"),
        missed: has("--missed"),
        platform: match value("--platform").as_deref() {
            Some("windows") => Platform::Windows,
            Some("linux") => Platform::Linux,
            _ => Platform::MacOs,
        },
        bytes_per_tick: (megabytes * 1e6 * TICK.as_secs_f64()) as u64,
    }
}

fn card() -> ModelCard {
    ModelCard::of(skald_models::find("nb-whisper-large").expect("in the registry"))
        .expect("downloadable")
}

fn main() {
    let options = options();
    let mut model = FirstRunModel::new(card(), options.status);
    model.platform = options.platform;
    model.hold_key = if options.platform == Platform::MacOs {
        HoldKey::RightCommand
    } else {
        HoldKey::RightControl
    };
    // What the system would answer; the window reads it through the checks.
    let microphone = Rc::new(Cell::new(if options.ready {
        Access::Granted
    } else if options.denied {
        Access::Denied
    } else if options.platform != Platform::MacOs {
        // Windows and Linux report the microphone as granted.
        Access::Granted
    } else {
        Access::NotAsked
    }));
    let accessibility = Rc::new(Cell::new(options.ready));

    let application = gpui_kit::application().with_quit_mode(QuitMode::LastWindowClosed);
    application.run(move |cx: &mut App| {
        let checks = PermissionChecks {
            microphone: Box::new({
                let microphone = microphone.clone();
                move || microphone.get()
            }),
            accessibility: Box::new({
                let accessibility = accessibility.clone();
                move || accessibility.get()
            }),
        };
        let example = Rc::new(Example {
            model: RefCell::new(model),
            window: RefCell::new(None),
            microphone,
            accessibility,
            download: Cell::new(0),
            downloads_failed: Cell::new(0),
            options,
        });
        let acting = example.clone();
        let window = FirstRunWindow::new(checks, move |action, cx| acting.act(action, cx), cx);
        example.window.replace(Some(window));
        example.show(cx);
    });
}

struct Example {
    model: RefCell<FirstRunModel>,
    window: RefCell<Option<FirstRunWindow>>,
    microphone: Rc<Cell<Access>>,
    accessibility: Rc<Cell<bool>>,
    /// Counts the downloads started, so a stopped one's ticks know they are no longer wanted.
    download: Cell<u64>,
    downloads_failed: Cell<u32>,
    options: Options,
}

impl Example {
    fn show(&self, cx: &mut App) {
        if let Some(window) = self.window.borrow_mut().as_mut() {
            window.show(&self.model.borrow(), cx);
        }
    }

    fn change(&self, cx: &mut App, change: impl FnOnce(&mut FirstRunModel)) {
        change(&mut self.model.borrow_mut());
        if let Some(window) = self.window.borrow_mut().as_mut() {
            window.update(&self.model.borrow(), cx);
        }
    }

    fn act(self: &Rc<Self>, action: Action, cx: &mut App) {
        println!("{action:?}");
        match action {
            Action::StartDownload => self.start_download(cx),
            Action::CancelDownload => {
                self.change(cx, |model| model.download.stopping());
                let example = self.clone();
                self.later(cx, Duration::from_millis(500), move |cx| {
                    example.download.set(example.download.get() + 1);
                    example.change(cx, |model| {
                        model.download.finish::<()>(&Err(Error::Cancelled))
                    });
                });
            }
            Action::RequestAccess(permission) if self.options.refuse => {
                println!("  pretending the system asked and the user said no");
                if permission == Permission::Microphone {
                    self.microphone.set(Access::Denied);
                }
                let example = self.clone();
                self.later(cx, Duration::from_millis(900), move |cx| {
                    example.check_permissions(cx)
                });
            }
            Action::RequestAccess(permission) => {
                println!("  pretending the system asked and the user said yes");
                self.grant_later(permission, Duration::from_millis(900), cx);
            }
            Action::OpenSettings(permission) => {
                println!(
                    "  would open System Settings at {permission:?}; pretending the user turns it on"
                );
                self.grant_later(permission, Duration::from_secs(3), cx);
            }
            Action::RunTest => self.run_test(cx),
            Action::Done | Action::Closed => cx.quit(),
        }
    }

    fn grant_later(self: &Rc<Self>, permission: Permission, delay: Duration, cx: &mut App) {
        let example = self.clone();
        self.later(cx, delay, move |cx| {
            match permission {
                Permission::Microphone => example.microphone.set(Access::Granted),
                Permission::Accessibility => example.accessibility.set(true),
            }
            example.check_permissions(cx);
        });
    }

    /// The system's prompt was answered while the window kept the front, so it reads the permissions
    /// only when told to.
    fn check_permissions(&self, cx: &mut App) {
        if let Some(window) = self.window.borrow_mut().as_mut() {
            window.check_permissions(cx);
        }
    }

    fn start_download(self: &Rc<Self>, cx: &mut App) {
        let generation = self.download.get() + 1;
        self.download.set(generation);
        let size = card().size;
        self.change(cx, |model| model.download.start(size));
        let first_try = self.downloads_failed.get() == 0;
        let fail = first_try && self.options.fail;
        let offline = first_try && self.options.offline;
        let step = self.options.bytes_per_tick;
        let example = self.clone();
        cx.spawn(async move |cx: &mut AsyncApp| {
            let wanted = |example: &Example| example.download.get() == generation;
            // Connecting, or checking what an earlier download left.
            cx.background_executor()
                .timer(Duration::from_millis(900))
                .await;
            if offline {
                example.downloads_failed.set(1);
                cx.update(|cx| {
                    example.change(cx, |model| {
                        model
                            .download
                            .finish::<()>(&Err(Error::Connection("no route to host".into())))
                    })
                });
                return;
            }
            let mut received = example.model.borrow().download.received();
            let mut broke = false;
            let mut tick = 0u64;
            while received < size {
                if !wanted(&example) {
                    return;
                }
                tick += 1;
                // Uneven, as a real connection is.
                let jitter = 0.6 + 0.8 * ((tick as f64 * 0.7).sin() * 0.5 + 0.5);
                received = (received + (step as f64 * jitter) as u64).min(size);
                cx.update(|cx| {
                    example.change(cx, |model| {
                        model.download.event(
                            Event::Progress {
                                received,
                                total: size,
                            },
                            Instant::now(),
                        )
                    })
                });
                if !broke && received > size * 3 / 5 {
                    broke = true;
                    for attempt in 1..=2 {
                        cx.update(|cx| {
                            example.change(cx, |model| {
                                model
                                    .download
                                    .event(Event::Retrying { attempt, of: 5 }, Instant::now())
                            })
                        });
                        cx.background_executor()
                            .timer(Duration::from_millis(1500))
                            .await;
                        if !wanted(&example) {
                            return;
                        }
                    }
                }
                cx.background_executor().timer(TICK).await;
            }
            cx.background_executor()
                .timer(Duration::from_millis(700))
                .await;
            if !wanted(&example) {
                return;
            }
            let result = if fail {
                example.downloads_failed.set(1);
                Err(Error::HashMismatch {
                    expected: card().sha256,
                    actual: "9d1c".repeat(16),
                })
            } else {
                Ok(PathBuf::from("nb-whisper-large/ggml-model.bin"))
            };
            cx.update(|cx| example.change(cx, |model| model.download.finish(&result)));
        })
        .detach();
    }

    fn run_test(self: &Rc<Self>, cx: &mut App) {
        const TEXT: &str = "Hei, dette er en prøve.";
        let ending = if self.options.nothing {
            TestDictation::HeardNothing {
                microphone: "MacBook Pro-mikrofon".into(),
            }
        } else {
            TestDictation::Heard { text: TEXT.into() }
        };
        let inserts = !self.options.nothing && !self.options.missed;
        let stages = [
            (Duration::ZERO, TestDictation::Loading),
            (Duration::from_millis(1500), TestDictation::Listening),
            (Duration::from_millis(2500), TestDictation::Recording),
            (Duration::from_millis(2600), TestDictation::Transcribing),
            (Duration::from_millis(1100), ending),
        ];
        let example = self.clone();
        cx.spawn(async move |cx: &mut AsyncApp| {
            for (wait, stage) in stages {
                cx.background_executor().timer(wait).await;
                if inserts && matches!(stage, TestDictation::Heard { .. }) {
                    // Skald inserts the text, with a trailing space, before it reports the take.
                    cx.update(|cx| type_into_focus(&format!("{TEXT} "), cx));
                }
                cx.update(|cx| example.change(cx, |model| model.test = stage));
            }
        })
        .detach();
    }

    fn later(&self, cx: &mut App, delay: Duration, then: impl FnOnce(&mut App) + 'static) {
        cx.spawn(async move |cx: &mut AsyncApp| {
            cx.background_executor().timer(delay).await;
            cx.update(then);
        })
        .detach();
    }
}

/// Types the text where the window's focus is, as one keystroke that carries it all.
fn type_into_focus(text: &str, cx: &mut App) {
    let Some(handle) = cx.windows().first().copied() else {
        return;
    };
    let keystroke = Keystroke {
        modifiers: Modifiers::default(),
        key: "h".into(),
        key_char: Some(text.into()),
    };
    let _ = handle.update(cx, |_, window, cx| window.dispatch_keystroke(keystroke, cx));
}
