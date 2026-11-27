//! The app's screen on the main thread, inside GPUI: the overlay, the menu bar item and the settings
//! window. The dictation loop's events drive them through [`Screen`]; what the menu and the window ask for
//! is carried out here. A setting changes the configuration file through the same running Skald the socket
//! answers for, so the menu, the window and `skald` change it alike.
//!
//! The model must be dropped before the process exits, so quitting stops the loop first: «Avslutt Skald»
//! asks it to stop, and the app quits once it has. When the system ends the app, as at log out, the main
//! thread waits for the loop before it lets the process go.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};

use async_channel::Receiver;
use gpui_kit::{ActivationPolicy, App, ClipboardItem, QuitMode};
use skald_control::Skald;
use skald_core::paths::Paths;
use skald_engine::PROMPT_TOKENS;
use skald_models::{Cancel, Downloader, Status};
use skald_overlay::Overlay;
use skald_settings::{Action as SettingsAction, Change, SettingsWindow, Tab, TokenCounter};
use skald_tray::{Action as TrayAction, Tray};

use crate::app::{self, Ready};
use crate::control::Control;
use crate::events::Event;
use crate::pipeline::WorkerHandle;
use crate::say::{Exit, Line, Printer};
use crate::screen::{self, Pill, Screen};

type Dictation = Rc<RefCell<Option<JoinHandle<Result<(), Exit>>>>>;

/// Runs the screen until the app quits; on macOS the process ends inside, once the loop has stopped.
pub(crate) fn run(
    ready: Ready,
    events: Receiver<Event>,
    stop: Arc<AtomicBool>,
    dictation: JoinHandle<Result<(), Exit>>,
    printer: Printer,
) -> Result<(), Exit> {
    let dictation: Dictation = Rc::new(RefCell::new(Some(dictation)));
    let joined = Rc::clone(&dictation);
    // Without it GPUI quits on Windows and Linux when the overlay's window closes after a take.
    let application = gpui_kit::application().with_quit_mode(QuitMode::Explicit);
    application.run(move |cx: &mut App| {
        // No Dock icon and no app menu: Skald lives in the menu bar.
        cx.set_activation_policy(ActivationPolicy::Accessory);
        let (tray_actions, chosen) = async_channel::unbounded();
        let (downloads, arriving) = async_channel::unbounded();
        let shell = Rc::new_cyclic(|this: &std::rc::Weak<RefCell<Shell>>| {
            let this = this.clone();
            let settings = SettingsWindow::new(
                move |action, cx| {
                    if let Some(shell) = this.upgrade() {
                        shell.borrow_mut().on_settings_action(action, cx);
                    }
                },
                cx,
            );
            RefCell::new(Shell::new(
                ready,
                settings,
                stop.clone(),
                downloads,
                printer,
                cx,
            ))
        });

        let screen = shell.borrow().screen.tray();
        match Tray::new(&screen, move |action| {
            // AppKit calls this outside any GPUI update, so the action waits for the task below.
            let _ = tray_actions.try_send(action);
        }) {
            Ok(tray) => shell.borrow_mut().tray = Some(tray),
            Err(error) => printer.say(Line::NoMenuBarItem(error.to_string())),
        }

        let on_events = Rc::clone(&shell);
        let stopped = Rc::clone(&dictation);
        cx.spawn(async move |cx| {
            while let Ok(event) = events.recv().await {
                if event == Event::Stopped {
                    cx.update(|cx| quit(&stopped, cx));
                    break;
                }
                cx.update(|cx| on_events.borrow_mut().on_event(event, cx));
            }
        })
        .detach();

        let on_actions = Rc::clone(&shell);
        cx.spawn(async move |cx| {
            while let Ok(action) = chosen.recv().await {
                cx.update(|cx| on_actions.borrow_mut().on_tray_action(action, cx));
            }
        })
        .detach();

        let on_downloads = Rc::clone(&shell);
        cx.spawn(async move |cx| {
            while let Ok(download) = arriving.recv().await {
                cx.update(|cx| on_downloads.borrow_mut().on_download(download, cx));
            }
        })
        .detach();

        let waited = Rc::clone(&dictation);
        cx.on_app_quit(move |_| {
            stop.store(true, Ordering::Release);
            if let Some(thread) = waited.borrow_mut().take() {
                let _ = thread.join();
            }
            async {}
        })
        .detach();
    });
    let thread = joined.borrow_mut().take();
    thread.map_or(Ok(()), |thread| thread.join().unwrap_or(Err(Exit::Stopped)))
}

/// The loop has stopped and dropped the model: the app quits, or on an error says why and exits with it.
fn quit(dictation: &Dictation, cx: &mut App) {
    let thread = dictation.borrow_mut().take();
    let result = thread.map_or(Ok(()), |thread| thread.join().unwrap_or(Err(Exit::Stopped)));
    match result {
        Ok(()) => cx.quit(),
        Err(exit) => {
            eprintln!("{exit}");
            app::open_settings_for(&exit);
            std::process::exit(i32::from(exit.code()));
        }
    }
}

/// How far a model's download has come, as its thread reports it.
#[derive(Debug)]
enum Download {
    Progress {
        model: &'static str,
        received: u64,
    },
    Ended {
        model: &'static str,
        result: Result<(), skald_models::Error>,
    },
}

struct Shell {
    screen: Screen,
    overlay: Overlay,
    tray: Option<Tray>,
    settings: SettingsWindow,
    control: Arc<Control>,
    worker: WorkerHandle,
    paths: Paths,
    printer: Printer,
    stop: Arc<AtomicBool>,
    /// Token counts the model gave, by text, so the meter asks it once per text.
    tokens: Rc<RefCell<HashMap<String, usize>>>,
    /// The downloads running, by model id, with how to stop each and how much has arrived.
    downloading: HashMap<String, (Cancel, u64)>,
    downloads: async_channel::Sender<Download>,
}

impl Shell {
    fn new(
        ready: Ready,
        settings: SettingsWindow,
        stop: Arc<AtomicBool>,
        downloads: async_channel::Sender<Download>,
        printer: Printer,
        cx: &mut App,
    ) -> Self {
        let screen = Screen::default();
        let overlay = Overlay::new(screen.theme(), cx);
        Self {
            screen,
            overlay,
            tray: None,
            settings,
            control: ready.control,
            worker: ready.worker,
            paths: ready.paths,
            printer,
            stop,
            tokens: Rc::default(),
            downloading: HashMap::new(),
            downloads,
        }
    }

    fn on_event(&mut self, event: Event, cx: &mut App) {
        let changed = self.screen.apply(event);
        self.carry_out(changed, cx);
    }

    fn carry_out(&mut self, changed: screen::Changed, cx: &mut App) {
        if changed.theme {
            self.overlay.set_theme(self.screen.theme(), cx);
        }
        for pill in changed.pills {
            match pill {
                Pill::Record { hands_free: false } => self.overlay.record(cx),
                Pill::Record { hands_free: true } => self.overlay.record_hands_free(cx),
                Pill::Level(dbfs) => self.overlay.level(dbfs, cx),
                Pill::Release => self.overlay.release(cx),
                Pill::Inserted(text) => self.overlay.inserted(&text, cx),
                Pill::HeardNothing(microphone) => self.overlay.heard_nothing(&microphone, cx),
                Pill::MicrophoneChanged(microphone) => {
                    self.overlay.microphone_changed(&microphone, cx);
                }
                Pill::Cancel => self.overlay.cancel(cx),
            }
        }
        if let Some(tray) = &mut self.tray {
            tray.update(&self.screen.tray());
        }
        if changed.settings {
            self.refresh_settings(cx);
        }
    }

    fn on_tray_action(&mut self, action: TrayAction, cx: &mut App) {
        if let Some(change) = screen::change_for(&action) {
            self.change(&change, cx);
            return;
        }
        match action {
            TrayAction::CopyText(text) => cx.write_to_clipboard(ClipboardItem::new_string(text)),
            TrayAction::OpenVocabulary => self.open_settings(Some(Tab::Vocabulary), cx),
            TrayAction::OpenSettings => self.open_settings(None, cx),
            TrayAction::Quit => self.stop.store(true, Ordering::Release),
            TrayAction::PinMicrophone(_)
            | TrayAction::UnpinMicrophone
            | TrayAction::SetLanguage(_) => {}
        }
    }

    fn on_settings_action(&mut self, action: SettingsAction, cx: &mut App) {
        match action {
            SettingsAction::Change(change) => self.change(&change, cx),
            SettingsAction::DownloadModel(id) => self.download(&id, cx),
            SettingsAction::StopDownload(id) => {
                if let Some((cancel, _)) = self.downloading.get(&id) {
                    cancel.cancel();
                }
            }
            SettingsAction::CorrectHistory {
                unix_ms,
                before,
                after,
            } => self.worker.correct_history(unix_ms, before, after),
        }
    }

    /// Writes the change to the configuration file, only if it changes anything (D-19), and shows the
    /// result at once; the loop takes it in within a tenth of a second.
    fn change(&mut self, change: &Change, cx: &mut App) {
        let written = apply(change, self.control.as_ref()).and_then(|()| self.control.config());
        match written {
            Ok(config) => {
                let changed = self.screen.set_config(Arc::new(config));
                self.carry_out(changed, cx);
            }
            Err(error) => self.printer.say(Line::SettingNotChanged(error.message)),
        }
    }

    fn open_settings(&mut self, tab: Option<Tab>, cx: &mut App) {
        let model = self.settings_model();
        self.settings.show(&model, tab, cx);
        cx.activate(true);
    }

    fn refresh_settings(&mut self, cx: &mut App) {
        let model = self.settings_model();
        self.settings.update(&model, cx);
    }

    fn settings_model(&self) -> skald_settings::SettingsModel {
        let downloading = self
            .downloading
            .iter()
            .map(|(id, (_, received))| (id.clone(), *received))
            .collect();
        let models = screen::model_states(&self.paths.models_dir(), &downloading);
        self.screen.settings(models, Some(self.token_counter()))
    }

    /// Counts with the loaded model, asking it once per text. A count asked while a take is being
    /// transcribed waits for that take.
    fn token_counter(&self) -> TokenCounter {
        let counted = Rc::clone(&self.tokens);
        let worker = self.worker.clone();
        TokenCounter::new(PROMPT_TOKENS, move |text| {
            *counted
                .borrow_mut()
                .entry(text.to_owned())
                .or_insert_with(|| worker.count_tokens(text).unwrap_or(0))
        })
    }

    /// Fetches the model on a thread of its own, from where an earlier download stopped.
    fn download(&mut self, id: &str, cx: &mut App) {
        let Some(model) = skald_models::find(id) else {
            return;
        };
        let Some(file) = model.file() else {
            return;
        };
        if self.downloading.contains_key(model.id) {
            return;
        }
        let models_dir = self.paths.models_dir();
        let received = match model.status(&models_dir) {
            Some(Status::Partial { received }) => received,
            _ => 0,
        };
        let cancel = Cancel::new();
        self.downloading
            .insert(model.id.to_owned(), (cancel.clone(), received));
        let dir = model.dir(&models_dir);
        let downloads = self.downloads.clone();
        let started = thread::Builder::new()
            .name("skald-download".into())
            .spawn(move || {
                let result = Downloader::new().download(file, &dir, &cancel, |event| {
                    if let skald_models::Event::Progress { received, .. } = event {
                        let _ = downloads.send_blocking(Download::Progress {
                            model: model.id,
                            received,
                        });
                    }
                });
                let _ = downloads.send_blocking(Download::Ended {
                    model: model.id,
                    result: result.map(|_| ()),
                });
            });
        if let Err(error) = started {
            self.downloading.remove(model.id);
            self.printer.say(Line::DownloadFailed {
                model: model.name,
                reason: error.to_string(),
            });
        }
        self.refresh_settings(cx);
    }

    fn on_download(&mut self, download: Download, cx: &mut App) {
        match download {
            Download::Progress { model, received } => {
                if let Some((_, arrived)) = self.downloading.get_mut(model) {
                    *arrived = received;
                }
            }
            Download::Ended { model, result } => {
                self.downloading.remove(model);
                let name = skald_models::find(model).map_or(model, |model| model.name);
                match result {
                    Ok(()) => self.printer.say(Line::Downloaded(name)),
                    Err(skald_models::Error::Cancelled) => {}
                    Err(error) => self.printer.say(Line::DownloadFailed {
                        model: name,
                        reason: error.to_string(),
                    }),
                }
            }
        }
        self.refresh_settings(cx);
    }
}

/// Applies a change from the menu or the settings window to the configuration file, as `skald` does.
fn apply(change: &Change, skald: &dyn Skald) -> Result<(), skald_control::Error> {
    skald.change_config(&mut |config| {
        change.apply(config);
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::mpsc;

    use skald_core::Config;
    use skald_core::config::Language;
    use skald_core::state::State;
    use skald_input::Inserted;

    use super::*;
    use crate::control::Settings;
    use crate::files::tests::Scratch;
    use crate::pipeline::Worker;
    use crate::pipeline::tests::{FakeInserter, FakeModel, quiet};

    #[test]
    fn a_change_from_the_menu_or_the_window_writes_the_file_once_and_tells_the_loop() {
        let scratch = Scratch::new("shell-change");
        let paths = Paths::in_one_folder(&scratch.0);
        let (events, _) = mpsc::channel();
        let worker = Worker::spawn(
            move || Ok(FakeModel::new([])),
            FakeInserter::answering(Ok(Inserted::PreviousRestored)),
            quiet(),
            State::default(),
            paths.state_file(),
            events,
        )
        .unwrap();
        let settings = Arc::new(Settings::new(Arc::new(Config::default())));
        let control = Control::new(paths.clone(), Arc::clone(&settings), worker.handle());

        let menu = screen::change_for(&TrayAction::SetLanguage(Language::Nynorsk)).unwrap();
        apply(&menu, &control).unwrap();
        let written = fs::read_to_string(paths.config_file()).unwrap();
        assert!(written.contains("language = \"nynorsk\""), "{written}");
        assert!(settings.take_written());
        assert_eq!(control.config().unwrap().language, Language::Nynorsk);

        // The same again changes nothing, so the file is left as it is (D-19).
        fs::write(paths.config_file(), format!("# kept\n{written}")).unwrap();
        apply(&Change::Language(Language::Nynorsk), &control).unwrap();
        assert!(
            fs::read_to_string(paths.config_file())
                .unwrap()
                .starts_with("# kept")
        );
        assert!(!settings.take_written());

        apply(&Change::PinMicrophone(Some("Yeti".into())), &control).unwrap();
        assert_eq!(
            control.config().unwrap().pinned_microphone.as_deref(),
            Some("Yeti")
        );
        worker.finish();
    }
}
