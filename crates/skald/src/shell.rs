//! The app's screen on the main thread, inside GPUI: the overlay, the menu bar item, the settings window
//! and, while the model or a permission is missing, the first-run window. The dictation loop's events
//! drive them through [`Screen`]; what the menu and the windows ask for is carried out here. A setting
//! changes the configuration file through the same running Skald the socket answers for, so the menu, the
//! settings window and `skald` change it alike.
//!
//! The first run comes before the loop: the loop starts when the window's test asks for it, or when the
//! window is done, and the menu bar item appears once the window is done and the loop is ready. A loop the
//! test started that stops, as when the model will not load, says why in the window, which can start it
//! again.
//!
//! The model must be dropped before the process exits, so quitting stops the loop first: «Avslutt Skald»
//! asks it to stop, and the app quits once it has. When the system ends the app, as at log out, the main
//! thread waits for the loop before it lets the process go.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::{Rc, Weak};
use std::sync::Arc;
use std::thread;
use std::time::Instant;

use async_channel::Receiver;
use gpui_kit::{ActivationPolicy, App, ClipboardItem, QuitMode};
use skald_control::Skald;
use skald_core::paths::Paths;
use skald_engine::PROMPT_TOKENS;
use skald_firstrun::{
    Action as FirstRunAction, Download as FirstRunDownload, FirstRunWindow, TestDictation,
};
use skald_models::{Cancel, Downloader, Status};
use skald_overlay::Overlay;
use skald_settings::{Action as SettingsAction, Change, SettingsWindow, Tab, TokenCounter};
use skald_tray::{Action as TrayAction, Tray};

use crate::app::{self, FirstRun, Launcher, LoopThread, Ready, Start};
use crate::control::Control;
use crate::events::Event;
use crate::permissions;
use crate::pipeline::WorkerHandle;
use crate::say::{self, Exit, Line, Printer};
use crate::screen::{self, Pill, Screen};

/// The loop's thread while it runs, shared with the hook that waits for it as the app quits.
type Running = Rc<RefCell<Option<LoopThread>>>;

/// Runs the screen until the app quits; on macOS the process ends inside, once the loop has stopped.
pub(crate) fn run(
    start: Start,
    launcher: Launcher,
    events: Receiver<Event>,
    paths: Paths,
    printer: Printer,
) -> Result<(), Exit> {
    let running: Running = Rc::default();
    let left = Rc::clone(&running);
    // Without it GPUI quits on Windows and Linux when the overlay's window closes after a take.
    let application = gpui_kit::application().with_quit_mode(QuitMode::Explicit);
    application.run(move |cx: &mut App| {
        // No Dock icon and no app menu: Skald lives in the menu bar.
        cx.set_activation_policy(ActivationPolicy::Accessory);
        let (inbox, arriving) = async_channel::unbounded();
        let shell = Rc::new_cyclic(|this: &Weak<RefCell<Shell>>| {
            let acting = this.clone();
            let settings = SettingsWindow::new(
                move |action, cx| {
                    if let Some(shell) = acting.upgrade() {
                        shell.borrow_mut().on_settings_action(action, cx);
                    }
                },
                cx,
            );
            RefCell::new(Shell {
                screen: Screen::default(),
                overlay: Overlay::new(&Default::default(), cx),
                tray: None,
                settings,
                first_run: None,
                linked: None,
                launcher,
                running: Rc::clone(&running),
                quitting: false,
                paths,
                printer,
                tokens: Rc::default(),
                downloading: HashMap::new(),
                inbox,
                this: this.clone(),
            })
        });
        match start {
            Start::Ready(ready, thread) => {
                *running.borrow_mut() = Some(thread);
                shell.borrow_mut().on_ready(ready, cx);
            }
            Start::FirstRun(first_run) => shell.borrow_mut().open_first_run(first_run, cx),
        }

        let on_events = Rc::clone(&shell);
        cx.spawn(async move |cx| {
            while let Ok(event) = events.recv().await {
                cx.update(|cx| on_events.borrow_mut().on_event(event, cx));
            }
        })
        .detach();

        let on_inbox = Rc::clone(&shell);
        cx.spawn(async move |cx| {
            while let Ok(message) = arriving.recv().await {
                cx.update(|cx| on_inbox.borrow_mut().on_message(message, cx));
            }
        })
        .detach();

        let waited = Rc::clone(&running);
        let stopping = Rc::clone(&shell);
        cx.on_app_quit(move |_| {
            stopping.borrow().launcher.stop();
            if let Some(thread) = waited.borrow_mut().take() {
                let _ = thread.join();
            }
            async {}
        })
        .detach();
    });
    let thread = left.borrow_mut().take();
    thread.map_or(Ok(()), |thread| thread.join().unwrap_or(Err(Exit::Stopped)))
}

/// What reaches the main thread from elsewhere than the loop: the menu, downloads, the system's prompts.
#[derive(Debug)]
enum Message {
    Tray(TrayAction),
    Download(Download),
    /// The microphone's prompt was answered.
    Answered,
}

/// How far a model's download has come, as its thread reports it.
#[derive(Debug)]
enum Download {
    Event {
        model: &'static str,
        event: skald_models::Event,
    },
    Ended {
        model: &'static str,
        result: Result<(), skald_models::Error>,
    },
}

/// The first-run window and what it shows.
struct Window {
    window: FirstRunWindow,
    first_run: FirstRun,
}

/// The running loop as the screen reaches it once it is ready.
struct Linked {
    control: Arc<Control>,
    worker: WorkerHandle,
}

struct Shell {
    screen: Screen,
    overlay: Overlay,
    tray: Option<Tray>,
    settings: SettingsWindow,
    first_run: Option<Window>,
    linked: Option<Linked>,
    launcher: Launcher,
    running: Running,
    /// The user asked to quit, so the loop's end is the app's.
    quitting: bool,
    paths: Paths,
    printer: Printer,
    /// Token counts the model gave, by text, so the meter asks it once per text.
    tokens: Rc<RefCell<HashMap<String, usize>>>,
    /// The downloads running, by model id, with how to stop each and how much has arrived.
    downloading: HashMap<String, (Cancel, u64)>,
    inbox: async_channel::Sender<Message>,
    this: Weak<RefCell<Shell>>,
}

impl Shell {
    fn on_event(&mut self, event: Event, cx: &mut App) {
        if event == Event::Stopped {
            self.on_stopped(cx);
            return;
        }
        if let Some(window) = &mut self.first_run
            && window.first_run.model.test != TestDictation::NotStarted
            && let Some(test) = screen::test_dictation(&event)
        {
            window.first_run.model.test = test;
            window.window.update(&window.first_run.model, cx);
        }
        let changed = self.screen.apply(event);
        self.carry_out(changed, cx);
    }

    fn on_message(&mut self, message: Message, cx: &mut App) {
        match message {
            Message::Tray(action) => self.on_tray_action(action, cx),
            Message::Download(download) => self.on_download(download, cx),
            Message::Answered => {
                if let Some(window) = &mut self.first_run {
                    window.window.check_permissions(cx);
                }
            }
        }
    }

    /// The loop is ready: the menu bar item appears, unless the first run is still open.
    fn on_ready(&mut self, ready: Ready, cx: &mut App) {
        self.linked = Some(Linked {
            control: ready.control,
            worker: ready.worker,
        });
        if self.first_run.is_none() {
            self.show_tray();
        }
        self.refresh_settings(cx);
    }

    fn show_tray(&mut self) {
        if self.tray.is_some() {
            return;
        }
        let inbox = self.inbox.clone();
        match Tray::new(&self.screen.tray(), move |action| {
            // AppKit calls this outside any GPUI update, so the action waits for the inbox's task.
            let _ = inbox.try_send(Message::Tray(action));
        }) {
            Ok(tray) => self.tray = Some(tray),
            Err(error) => self
                .printer
                .say(Line::NoMenuBarItem(say::tray_reason(&error))),
        }
    }

    /// Starts the loop, and hands the screen its running Skald once it is ready.
    fn start_loop(&mut self, cx: &mut App) {
        if self.running.borrow().is_some() {
            return;
        }
        let (thread, readied) = match self.launcher.start() {
            Ok(started) => started,
            Err(exit) => {
                self.loop_failed(exit, cx);
                return;
            }
        };
        *self.running.borrow_mut() = Some(thread);
        let this = self.this.clone();
        cx.spawn(async move |cx| {
            if let Ok(ready) = readied.recv().await {
                cx.update(|cx| {
                    if let Some(shell) = this.upgrade() {
                        shell.borrow_mut().on_ready(ready, cx);
                    }
                });
            }
        })
        .detach();
    }

    /// The loop has stopped and dropped the model. While the first run is open its test says why; else
    /// the app quits, or on an error says why and exits with it.
    fn on_stopped(&mut self, cx: &mut App) {
        let thread = self.running.borrow_mut().take();
        let result = thread.map_or(Ok(()), |thread| thread.join().unwrap_or(Err(Exit::Stopped)));
        self.linked = None;
        if self.quitting {
            cx.quit();
            return;
        }
        match result {
            Err(exit) if self.first_run.is_some() => self.loop_failed(exit, cx),
            Ok(()) => cx.quit(),
            Err(exit) => {
                eprintln!("{exit}");
                app::open_settings_for(&exit);
                std::process::exit(i32::from(exit.code()));
            }
        }
    }

    fn loop_failed(&mut self, exit: Exit, cx: &mut App) {
        self.printer.say(Line::LoopStopped(exit.to_string()));
        if let Some(window) = &mut self.first_run {
            window.first_run.model.test = TestDictation::Failed {
                reason: exit.to_string(),
            };
            window.window.update(&window.first_run.model, cx);
        }
    }

    fn open_first_run(&mut self, first_run: FirstRun, cx: &mut App) {
        let acting = self.this.clone();
        let mut window = FirstRunWindow::new(
            permissions::checks(),
            move |action, cx| {
                if let Some(shell) = acting.upgrade() {
                    shell.borrow_mut().on_first_run_action(action, cx);
                }
            },
            cx,
        );
        window.show(&first_run.model, cx);
        self.first_run = Some(Window { window, first_run });
    }

    fn on_first_run_action(&mut self, action: FirstRunAction, cx: &mut App) {
        match action {
            FirstRunAction::StartDownload => {
                if let Some(window) = &mut self.first_run {
                    let size = window.first_run.model.model.size;
                    window.first_run.model.download.start(size);
                    window.window.update(&window.first_run.model, cx);
                    let id = window.first_run.model_id;
                    self.download(id, cx);
                }
            }
            FirstRunAction::CancelDownload => {
                if let Some(window) = &mut self.first_run {
                    if let Some((cancel, _)) = self.downloading.get(window.first_run.model_id) {
                        cancel.cancel();
                    }
                    window.first_run.model.download.stopping();
                    window.window.update(&window.first_run.model, cx);
                }
            }
            FirstRunAction::RequestAccess(permission) => {
                let inbox = self.inbox.clone();
                permissions::request(permission, move || {
                    let _ = inbox.try_send(Message::Answered);
                });
            }
            FirstRunAction::OpenSettings(permission) => permissions::open_settings(permission),
            FirstRunAction::RunTest => {
                if let Some(window) = &mut self.first_run {
                    window.first_run.model.test = if self.running.borrow().is_some() {
                        TestDictation::Listening
                    } else {
                        TestDictation::Loading
                    };
                    window.window.update(&window.first_run.model, cx);
                }
                self.start_loop(cx);
            }
            FirstRunAction::Done => self.first_run_done(cx),
            FirstRunAction::Closed => {
                if self.can_take_dictation() {
                    self.first_run_done(cx);
                } else {
                    self.quit(cx);
                }
            }
        }
    }

    /// The window has closed itself: the loop starts if the test has not started it, and the menu bar
    /// item appears once it is ready.
    fn first_run_done(&mut self, cx: &mut App) {
        self.first_run = None;
        if self.linked.is_some() {
            self.show_tray();
        } else {
            self.start_loop(cx);
        }
    }

    /// The model is in place and checked, and every permission is granted, as the first run asks.
    fn can_take_dictation(&self) -> bool {
        let Some(window) = &self.first_run else {
            return true;
        };
        let checks = permissions::checks();
        window.first_run.model.download == FirstRunDownload::Ready
            && (checks.microphone)() == skald_firstrun::Access::Granted
            && (checks.accessibility)()
    }

    /// Ends the app: the loop stops first and the app quits once it has; without a loop, at once.
    fn quit(&mut self, cx: &mut App) {
        self.quitting = true;
        if self.running.borrow().is_some() {
            self.launcher.stop();
        } else {
            cx.quit();
        }
    }

    fn carry_out(&mut self, changed: screen::Changed, cx: &mut App) {
        if changed.theme {
            self.overlay.set_theme(self.screen.theme(), cx);
        }
        if changed.hold_key {
            self.overlay.set_hold_key(self.screen.hold_key(), cx);
        }
        for pill in changed.pills {
            match pill {
                Pill::Record { hands_free: false } => self.overlay.record(cx),
                Pill::Record { hands_free: true } => self.overlay.record_hands_free(cx),
                Pill::Level(dbfs) => self.overlay.level(dbfs, cx),
                Pill::Release => self.overlay.release(cx),
                Pill::Inserted(text) => self.overlay.inserted(&text, cx),
                Pill::HeardNothing(microphone) => self.overlay.heard_nothing(&microphone, cx),
                Pill::NotInserted => self.overlay.not_inserted(cx),
                Pill::AdministratorWindow => self.overlay.administrator_window(cx),
                Pill::TranscriptionFailed => self.overlay.transcription_failed(cx),
                Pill::NoMicrophone => self.overlay.no_microphone(cx),
                Pill::MicrophoneNotOpened(microphone) => {
                    self.overlay.microphone_not_opened(&microphone, cx);
                }
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
            TrayAction::Quit => self.quit(cx),
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
            } => {
                if let Some(linked) = &self.linked {
                    linked.worker.correct_history(unix_ms, before, after);
                }
            }
        }
    }

    /// Writes the change to the configuration file, only if it changes anything (D-19), and shows the
    /// result at once; the loop takes it in within a tenth of a second.
    fn change(&mut self, change: &Change, cx: &mut App) {
        let Some(linked) = &self.linked else {
            return;
        };
        let control = linked.control.as_ref();
        match apply(change, control).and_then(|()| control.config()) {
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
        self.screen.settings(models, self.token_counter())
    }

    /// Counts with the loaded model, asking it once per text. A count asked while a take is being
    /// transcribed waits for that take.
    fn token_counter(&self) -> Option<TokenCounter> {
        let worker = self.linked.as_ref()?.worker.clone();
        let counted = Rc::clone(&self.tokens);
        Some(TokenCounter::new(PROMPT_TOKENS, move |text| {
            *counted
                .borrow_mut()
                .entry(text.to_owned())
                .or_insert_with(|| worker.count_tokens(text).unwrap_or(0))
        }))
    }

    /// Fetches the model on a thread of its own, from where an earlier download stopped, or checks a file
    /// already there.
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
        let inbox = self.inbox.clone();
        let started = thread::Builder::new()
            .name("skald-download".into())
            .spawn(move || {
                let result = Downloader::new().download(file, &dir, &cancel, |event| {
                    let _ = inbox.send_blocking(Message::Download(Download::Event {
                        model: model.id,
                        event,
                    }));
                });
                let _ = inbox.send_blocking(Message::Download(Download::Ended {
                    model: model.id,
                    result: result.map(|_| ()),
                }));
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
        let first_run = self
            .first_run
            .as_mut()
            .filter(|window| window.first_run.model_id == download.model());
        match download {
            Download::Event { model, event } => {
                if let skald_models::Event::Progress { received, .. } = event
                    && let Some((_, arrived)) = self.downloading.get_mut(model)
                {
                    *arrived = received;
                }
                if let Some(window) = first_run {
                    window.first_run.model.download.event(event, Instant::now());
                    window.window.update(&window.first_run.model, cx);
                }
            }
            Download::Ended { model, result } => {
                self.downloading.remove(model);
                if let Some(window) = first_run {
                    window.first_run.model.download.finish(&result);
                    window.window.update(&window.first_run.model, cx);
                }
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

impl Download {
    fn model(&self) -> &'static str {
        match self {
            Download::Event { model, .. } | Download::Ended { model, .. } => model,
        }
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
