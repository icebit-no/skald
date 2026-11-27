//! Skald on macOS and Linux: the hold key, the microphones and the model joined into one loop. The app runs
//! the loop on a thread of its own and the screen (`shell`: the overlay, the menu bar item, the settings
//! window) on the main thread, which GPUI owns; with `--terminal` the loop runs on the main thread alone.
//! Either way each line a user reads is printed as it happens.
//!
//! Start-up checks what can stop Skald before the model takes seconds to load: the files, Accessibility,
//! then the microphone permission. When the app finds the model or a permission missing, it opens the
//! first-run window instead and starts the loop once the window is done with them; otherwise the screen
//! appears once the checks have passed. The model loads on the pipeline's thread while the keys already
//! work, so a take made meanwhile waits for it. The command line's socket answers for as long as the loop
//! runs.

#[cfg(any(unix, windows))]
use skald_control::{BindError, Server, Serving};

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use skald_audio::MicrophoneAccess;
use skald_core::config::HoldKey as ConfigHoldKey;
use skald_core::paths::Paths;
use skald_core::state::State;
use skald_engine::Engine;
use skald_firstrun::{FirstRunModel, ModelCard};
use skald_input::{HoldKey, HoldListener, Inserter, ListenerEvent, Unavailable};
use skald_models::Status;

use crate::cli::Options;
use crate::control::{Control, Settings};
use crate::dictation::{Dictation, Microphones, SystemMicrophones, microphone_order};
use crate::events::{Ending, Event, MicrophoneList};
use crate::files::{self, ConfigFile};
use crate::login;
use crate::models::ModelFiles;
use crate::pipeline::{Worker, WorkerEvent, WorkerHandle};
use crate::say::{Exit, Line, Printer, report_lines};
use crate::shell;
use crate::signals;
use crate::sounds::{Afplay, Chime};

/// The longest the loop waits for a key before it looks at everything else.
const TICK: Duration = Duration::from_millis(100);

/// The same while a take records, so each level reading reaches the overlay soon after it is taken.
const TICK_WHILE_RECORDING: Duration = Duration::from_millis(20);

const CONFIG_EVERY: Duration = Duration::from_secs(1);

/// How often the microphones connected are listed for the menu and the settings window. The system tells
/// the loop only when the microphone in use changes, not when another one comes or goes.
const MICROPHONES_EVERY: Duration = Duration::from_secs(3);

/// The key listener says within this time whether it may listen.
const FIRST_KEY_EVENT_WITHIN: Duration = Duration::from_secs(2);

pub(crate) fn run(options: Options) -> Result<(), Exit> {
    // Windows has no screen for Skald yet, so there it runs in the terminal, as with --terminal.
    let result = if options.terminal || cfg!(windows) {
        dictate(options, None)
    } else {
        run_app(options)
    };
    if let Err(exit) = &result {
        open_settings_for(exit);
    }
    result
}

/// Opens the pane of System Settings a missing permission is granted in, which saves the user looking for
/// it.
pub(crate) fn open_settings_for(exit: &Exit) {
    match exit {
        Exit::NoAccessibility => {
            let _ = skald_input::open_accessibility_settings();
        }
        Exit::MicrophoneDenied => {
            let _ = std::process::Command::new("open")
                .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone")
                .status();
        }
        _ => {}
    }
}

/// What the screen needs of the running loop: the same running Skald the socket answers for, and the
/// model's thread for what needs the model.
pub(crate) struct Ready {
    pub(crate) control: Arc<Control>,
    pub(crate) worker: WorkerHandle,
}

/// The loop's side of the screen: where its events go, the screen's wish that it stop, and where it says
/// it is ready.
pub(crate) struct Link {
    events: async_channel::Sender<Event>,
    stop: Arc<AtomicBool>,
    ready: async_channel::Sender<Ready>,
}

/// The loop's thread, which returns why the loop stopped.
pub(crate) type LoopThread = JoinHandle<Result<(), Exit>>;

/// Starts the dictation loop on a thread of its own, linked to the screen, as often as the screen asks:
/// once, or again after a loop the first run started has stopped.
pub(crate) struct Launcher {
    options: Options,
    events: async_channel::Sender<Event>,
    stop: Arc<AtomicBool>,
}

impl Launcher {
    /// The loop's thread, and where it says it is ready; that ends without a word when the loop stops
    /// before it is ready, and the thread then holds the reason.
    pub(crate) fn start(&self) -> Result<(LoopThread, async_channel::Receiver<Ready>), Exit> {
        self.stop.store(false, Ordering::Release);
        let (ready, readied) = async_channel::bounded(1);
        let link = Link {
            events: self.events.clone(),
            stop: Arc::clone(&self.stop),
            ready,
        };
        let options = self.options.clone();
        let events = self.events.clone();
        let thread = thread::Builder::new()
            .name("skald-dictation".into())
            .spawn(move || {
                let result = dictate(options, Some(link));
                // The screen may already be gone, and then no one needs to know.
                let _ = events.send_blocking(Event::Stopped);
                result
            })
            .map_err(|error| Exit::CannotStart(error.to_string()))?;
        Ok((thread, readied))
    }

    /// Asks the running loop to stop; it says so with [`Event::Stopped`].
    pub(crate) fn stop(&self) {
        self.stop.store(true, Ordering::Release);
    }
}

/// How the screen starts: with the loop running and ready, or with the first-run window.
pub(crate) enum Start {
    Ready(Ready, LoopThread),
    FirstRun(FirstRun),
}

/// The first run, for a model or permission found missing.
pub(crate) struct FirstRun {
    pub(crate) model: FirstRunModel,
    /// The configured model, which the window downloads.
    pub(crate) model_id: &'static str,
}

/// Starts the screen on this thread. With nothing missing the loop starts first, and the screen appears
/// once it is ready, so a loop that stops before then never shows a screen and its reason is returned.
fn run_app(options: Options) -> Result<(), Exit> {
    let printer = Printer {
        verbose: options.verbose,
    };
    let paths = files::paths(options.dir.as_deref())?;
    let first_run = first_run(&options, &paths)?;
    let (events, inbox) = async_channel::unbounded();
    let launcher = Launcher {
        options,
        events,
        stop: Arc::new(AtomicBool::new(false)),
    };
    let start = match first_run {
        Some(first_run) => Start::FirstRun(first_run),
        None => {
            let (thread, readied) = launcher.start()?;
            match readied.recv_blocking() {
                Ok(ready) => Start::Ready(ready, thread),
                Err(_) => return thread.join().unwrap_or(Err(Exit::Stopped)),
            }
        }
    };
    shell::run(start, launcher, inbox, paths, printer)
}

/// The first-run window's model when the configured model or a permission is missing (docs/design.md,
/// First run). A model given with `--model` is the developer's to have in place.
fn first_run(options: &Options, paths: &Paths) -> Result<Option<FirstRun>, Exit> {
    if options.model.is_some() {
        return Ok(None);
    }
    let (_, config) = ConfigFile::open(paths.config_file()).map_err(Exit::Config)?;
    let Some(model) = skald_models::find(&config.model) else {
        return Ok(None);
    };
    let (Some(card), Some(status)) = (ModelCard::of(model), model.status(&paths.models_dir()))
    else {
        return Ok(None);
    };
    let needed = needs_first_run(
        status,
        skald_audio::microphone_access(),
        skald_input::accessibility_granted(),
    );
    Ok(needed.then(|| {
        let mut first_run = FirstRunModel::new(card, status);
        first_run.hold_key = config.hold_key.clone();
        FirstRun {
            model: first_run,
            model_id: model.id,
        }
    }))
}

/// Whether Skald cannot take dictation until the first run is done: the model is not in place and
/// checked, or a permission is not granted.
fn needs_first_run(status: Status, microphone: MicrophoneAccess, accessibility: bool) -> bool {
    status != Status::Ready || microphone != MicrophoneAccess::Granted || !accessibility
}

/// Where the loop's news goes: the terminal, and the screen when there is one.
struct Out {
    printer: Printer,
    link: Option<Link>,
}

impl Out {
    fn say(&self, line: Line) {
        self.printer.say(line);
    }

    fn show(&self, event: Event) {
        if let Some(link) = &self.link {
            // The channel holds any number of events; it fails only once the screen is gone.
            let _ = link.events.try_send(event);
        }
    }

    fn has_screen(&self) -> bool {
        self.link.is_some()
    }

    fn stop_asked(&self) -> bool {
        self.link
            .as_ref()
            .is_some_and(|link| link.stop.load(Ordering::Acquire))
    }

    fn ready(&self, ready: Ready) {
        if let Some(link) = &self.link {
            // A channel of its own for this loop, with room for the one message.
            let _ = link.ready.try_send(ready);
        }
    }
}

fn dictate(options: Options, link: Option<Link>) -> Result<(), Exit> {
    let out = Out {
        printer: Printer {
            verbose: options.verbose,
        },
        link,
    };
    out.say(Line::Version);
    let paths = files::paths(options.dir.as_deref())?;
    let _lock = files::lock(&paths.state_file().with_extension("lock"))?;
    let (config_file, config) = ConfigFile::open(paths.config_file()).map_err(Exit::Config)?;
    out.say(Line::ConfigFile {
        path: paths.config_file(),
        exists: config_file.exists(),
    });
    let models = ModelFiles::locate(
        &paths,
        &config.model,
        options.model.clone(),
        options.vad.clone(),
    )?;
    out.say(Line::ModelFiles {
        model: models.model.clone(),
        vad: models.vad.clone(),
    });
    out.say(Line::StateFile(paths.state_file()));
    models.check(|| out.say(Line::CheckingModel))?;
    let state = files::read_state(&paths.state_file()).unwrap_or_else(|reason| {
        out.say(Line::StateUnreadable {
            path: paths.state_file(),
            reason,
        });
        State::default()
    });
    signals::install().map_err(|error| Exit::CannotStart(error.to_string()))?;

    let mut keys = Keys::start(hold_key(&config.hold_key, &out))?;
    keys.wait_until_known(&out)?;

    match skald_audio::microphone_access() {
        MicrophoneAccess::Denied => return Err(Exit::MicrophoneDenied),
        MicrophoneAccess::Restricted => return Err(Exit::MicrophoneRestricted),
        MicrophoneAccess::NotAsked => out.say(Line::MicrophoneNotAsked),
        MicrophoneAccess::Granted => {}
    }
    let microphones =
        SystemMicrophones::start(microphone_order(&config)).map_err(Exit::Microphones)?;
    out.say(Line::Microphone(microphones.active().map(|m| m.name)));
    if let Some(line) = login::apply(config.start_at_login) {
        out.say(line);
    }

    out.say(Line::LoadingModel);
    let chime: Arc<dyn Chime> = Arc::new(Afplay);
    let (events, worker_events) = mpsc::channel();
    let ModelFiles { model, vad, .. } = models;
    let worker = Worker::spawn(
        move || {
            crate::pipeline::route_engine_logs();
            Engine::load(&model, &vad).map_err(|error| error.to_string())
        },
        Inserter::new(),
        Arc::clone(&chime),
        state.clone(),
        paths.state_file(),
        events,
    )
    .map_err(|error| Exit::CannotStart(error.to_string()))?;
    let config = Arc::new(config);
    let settings = Arc::new(Settings::new(Arc::clone(&config)));
    let control = Arc::new(Control::new(
        paths.clone(),
        Arc::clone(&settings),
        worker.handle(),
    ));
    let serving = serve(&paths, Arc::clone(&control), &out)?;
    let dictation = Dictation::new(microphones, chime, Arc::clone(&config));

    out.show(Event::Settings(Arc::clone(&config)));
    out.show(Event::State(state));
    let mut running = Running {
        keys,
        dictation,
        worker: &worker,
        worker_events,
        config_file,
        settings,
        model_given: options.model.is_some(),
        start_at_login: config.start_at_login,
        listed: None,
        next_listing: Instant::now(),
        out,
    };
    running.list_microphones();
    running.out.ready(Ready {
        control,
        worker: worker.handle(),
    });
    let result = running.listen();
    // The event tap goes first, then the socket and the microphones; the worker finishes the takes it has
    // and drops the model last.
    let Running {
        keys, dictation, ..
    } = running;
    drop(keys);
    drop(serving);
    drop(dictation);
    worker.finish();
    result
}

/// Answers the command line on the socket. Without it Skald still takes dictation, and the commands work
/// on the files, which the loop reads again when they change.
#[cfg(any(unix, windows))]
fn serve(paths: &Paths, control: Arc<Control>, out: &Out) -> Result<Option<Serving>, Exit> {
    match Server::bind(&paths.socket()) {
        Ok(server) => Ok(Some(server.serve(control))),
        Err(BindError::AlreadyRunning) => Err(Exit::AlreadyRunning),
        Err(error) => {
            out.say(Line::NoSocket(error.to_string()));
            Ok(None)
        }
    }
}

struct Running<'w> {
    keys: Keys,
    dictation: Dictation<SystemMicrophones>,
    worker: &'w Worker,
    worker_events: Receiver<WorkerEvent>,
    config_file: ConfigFile,
    settings: Arc<Settings>,
    model_given: bool,
    /// The setting the login item was last brought to.
    start_at_login: bool,
    /// The microphones last shown on the screen.
    listed: Option<MicrophoneList>,
    next_listing: Instant,
    out: Out,
}

impl Running<'_> {
    fn listen(&mut self) -> Result<(), Exit> {
        let mut next_config_look = Instant::now() + CONFIG_EVERY;
        let mut wanted_key = self.keys.key;
        loop {
            if signals::received() || self.out.stop_asked() {
                self.out.say(Line::Stopping);
                return Ok(());
            }
            let tick = if self.dictation.is_recording() {
                TICK_WHILE_RECORDING
            } else {
                TICK
            };
            if let Some(event) = self.keys.next(tick)? {
                self.on_key_event(event)?;
            }
            self.dictation.poll();
            for line in self.dictation.take_lines() {
                self.out.say(line);
            }
            let mut microphone_changed = false;
            for event in self.dictation.take_events() {
                microphone_changed |= matches!(event, Event::MicrophoneChanged(_));
                self.out.show(event);
            }
            if self.dictation.take_hands_free_ended() {
                self.keys.listener.hands_free_ended();
            }
            for job in self.dictation.take_jobs() {
                self.worker.submit(job);
            }
            for event in self.worker_events.try_iter() {
                self.on_worker_event(event)?;
            }
            if self.settings.take_written() || Instant::now() >= next_config_look {
                next_config_look = Instant::now() + CONFIG_EVERY;
                if let Some(key) = self.look_at_config() {
                    wanted_key = key;
                }
            }
            if microphone_changed || Instant::now() >= self.next_listing {
                self.list_microphones();
            }
            if wanted_key != self.keys.key && self.dictation.is_idle() {
                self.keys = Keys::start(wanted_key)?;
                self.out.say(Line::HoldKeyChanged(wanted_key));
            }
        }
    }

    fn on_key_event(&mut self, event: ListenerEvent) -> Result<(), Exit> {
        match event {
            ListenerEvent::Hold(event) => self.dictation.on_hold(event)?,
            ListenerEvent::Ready => {
                if std::mem::take(&mut self.keys.unavailable) {
                    self.out.say(Line::ListenerBack);
                }
            }
            ListenerEvent::Unavailable(Unavailable::NoAccessibility) => {
                return Err(Exit::NoAccessibility);
            }
            ListenerEvent::Unavailable(Unavailable::ListenerFailed(reason)) => {
                self.keys.unavailable = true;
                self.out.say(Line::ListenerFailed(reason));
            }
        }
        Ok(())
    }

    fn on_worker_event(&self, event: WorkerEvent) -> Result<(), Exit> {
        match event {
            WorkerEvent::Loaded { device, took } => {
                self.out.say(Line::ModelLoaded { device, took });
                if cfg!(windows) && !skald_engine::gpu_available() {
                    self.out.say(Line::NoVulkan);
                }
                self.out.say(Line::Ready(self.keys.key));
                self.out.show(Event::ModelLoaded);
            }
            WorkerEvent::LoadFailed(reason) => return Err(Exit::ModelLoad(reason)),
            WorkerEvent::Done(report) => {
                for line in report_lines(&report) {
                    self.out.say(line);
                }
                self.out.show(Event::Ended {
                    take: report.take,
                    ending: Ending::of(&report),
                });
                self.out.show(Event::State(report.state));
            }
            WorkerEvent::HistoryCorrected(corrected) => {
                if let Some(reason) = corrected.error {
                    self.out.say(Line::StateNotSaved(reason));
                }
                self.out.show(Event::State(corrected.state));
            }
        }
        Ok(())
    }

    /// Takes a changed configuration; returns the hold key it asks for.
    fn look_at_config(&mut self) -> Option<HoldKey> {
        let config = match self.config_file.changed()? {
            Ok(config) => config,
            Err(error) => {
                self.out.say(Line::ConfigBroken(error));
                return None;
            }
        };
        self.out.say(Line::ConfigReloaded);
        if config.model != self.dictation.config().model && !self.model_given {
            self.out.say(Line::ModelAtRestart);
        }
        if config.start_at_login != self.start_at_login {
            self.start_at_login = config.start_at_login;
            if let Some(line) = login::apply(config.start_at_login) {
                self.out.say(line);
            }
        }
        let key = hold_key(&config.hold_key, &self.out);
        let config = Arc::new(config);
        self.settings.set(Arc::clone(&config));
        self.dictation.set_config(Arc::clone(&config));
        self.out.show(Event::Settings(config));
        Some(key)
    }

    /// Shows the microphones connected when they differ from those last shown; only with a screen.
    fn list_microphones(&mut self) {
        self.next_listing = Instant::now() + MICROPHONES_EVERY;
        if !self.out.has_screen() {
            return;
        }
        let Ok(connected) = skald_audio::connected_microphones() else {
            return;
        };
        let list = MicrophoneList {
            connected: connected.microphones.into_iter().map(|m| m.name).collect(),
            in_use: self.dictation.microphones().active().map(|m| m.name),
        };
        if self.listed.as_ref() != Some(&list) {
            self.listed = Some(list.clone());
            self.out.show(Event::Microphones(list));
        }
    }
}

/// The hold-key listener, and whether it last said it could not listen.
struct Keys {
    listener: HoldListener,
    key: HoldKey,
    unavailable: bool,
}

impl Keys {
    fn start(key: HoldKey) -> Result<Self, Exit> {
        let listener =
            HoldListener::start(key).map_err(|error| Exit::CannotStart(error.to_string()))?;
        Ok(Self {
            listener,
            key,
            unavailable: false,
        })
    }

    fn next(&self, timeout: Duration) -> Result<Option<ListenerEvent>, Exit> {
        match self.listener.events().recv_timeout(timeout) {
            Ok(event) => Ok(Some(event)),
            Err(RecvTimeoutError::Timeout) => Ok(None),
            Err(RecvTimeoutError::Disconnected) => Err(Exit::KeysStopped),
        }
    }

    /// Waits for the listener's first word on whether it may listen, so a missing permission is told
    /// before the model loads.
    fn wait_until_known(&mut self, out: &Out) -> Result<(), Exit> {
        match self.next(FIRST_KEY_EVENT_WITHIN)? {
            Some(ListenerEvent::Unavailable(Unavailable::NoAccessibility)) => {
                Err(Exit::NoAccessibility)
            }
            Some(ListenerEvent::Unavailable(Unavailable::ListenerFailed(reason))) => {
                self.unavailable = true;
                out.say(Line::ListenerFailed(reason));
                Ok(())
            }
            Some(ListenerEvent::Ready | ListenerEvent::Hold(_)) | None => Ok(()),
        }
    }
}

/// The configuration's hold key, or the platform's own, said, for a key this version does not know.
fn hold_key(key: &ConfigHoldKey, out: &Out) -> HoldKey {
    listened_key(key).unwrap_or_else(|name| {
        out.say(Line::UnknownHoldKey(name));
        HoldKey::PLATFORM_DEFAULT
    })
}

/// The key to listen for, or the name of one this version does not know.
fn listened_key(key: &ConfigHoldKey) -> Result<HoldKey, String> {
    match key {
        ConfigHoldKey::RightCommand => Ok(HoldKey::RightCommand),
        ConfigHoldKey::RightControl => Ok(HoldKey::RightControl),
        ConfigHoldKey::Unknown(name) => Err(name.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_hold_key_the_configuration_knows_is_listened_for() {
        assert_eq!(
            listened_key(&ConfigHoldKey::RightCommand),
            Ok(HoldKey::RightCommand)
        );
        assert_eq!(
            listened_key(&ConfigHoldKey::RightControl),
            Ok(HoldKey::RightControl)
        );
        assert_eq!(
            listened_key(&ConfigHoldKey::Unknown("right-option".into())),
            Err("right-option".into())
        );
    }

    fn linked() -> (
        Out,
        async_channel::Receiver<Event>,
        Arc<AtomicBool>,
        async_channel::Receiver<Ready>,
    ) {
        let (events, inbox) = async_channel::unbounded();
        let stop = Arc::new(AtomicBool::new(false));
        let (ready, started) = async_channel::bounded(1);
        let out = Out {
            printer: Printer { verbose: false },
            link: Some(Link {
                events,
                stop: Arc::clone(&stop),
                ready,
            }),
        };
        (out, inbox, stop, started)
    }

    #[test]
    fn the_screen_hears_the_loop_and_can_ask_it_to_stop() {
        let (out, inbox, stop, _started) = linked();
        assert!(out.has_screen());
        out.show(Event::ModelLoaded);
        out.show(Event::Level(-20.0));
        assert_eq!(inbox.try_recv(), Ok(Event::ModelLoaded));
        assert_eq!(inbox.try_recv(), Ok(Event::Level(-20.0)));
        assert!(!out.stop_asked());
        stop.store(true, Ordering::Release);
        assert!(out.stop_asked());

        // The screen gone, the loop still runs.
        drop(inbox);
        out.show(Event::ModelLoaded);
    }

    #[test]
    fn the_first_run_opens_while_the_model_or_a_permission_is_missing() {
        use MicrophoneAccess::{Denied, Granted, NotAsked, Restricted};
        assert!(!needs_first_run(Status::Ready, Granted, true));
        for status in [
            Status::Missing,
            Status::Unchecked,
            Status::Partial { received: 9 },
            Status::Other { size: 3 },
        ] {
            assert!(needs_first_run(status, Granted, true), "{status:?}");
        }
        for microphone in [NotAsked, Denied, Restricted] {
            assert!(needs_first_run(Status::Ready, microphone, true));
        }
        assert!(needs_first_run(Status::Ready, Granted, false));
    }

    #[test]
    fn a_model_given_on_the_command_line_or_one_with_nothing_to_fetch_has_no_first_run() {
        let scratch = crate::files::tests::Scratch::new("app-first-run");
        let paths = Paths::in_one_folder(&scratch.0);
        let given = Options {
            model: Some("/m.bin".into()),
            ..Options::default()
        };
        assert!(first_run(&given, &paths).unwrap().is_none());

        std::fs::write(paths.config_file(), "model = \"nordlyd\"\n").unwrap();
        assert!(first_run(&Options::default(), &paths).unwrap().is_none());

        std::fs::write(paths.config_file(), "hold_key = \"right-control\"\n").unwrap();
        let first = first_run(&Options::default(), &paths).unwrap().unwrap();
        assert_eq!(first.model_id, "nb-whisper-large");
        assert_eq!(first.model.model.name, "NB-Whisper Large");
        assert_eq!(
            first.model.hold_key,
            skald_core::config::HoldKey::RightControl
        );
        assert_eq!(first.model.download, skald_firstrun::Download::NotStarted);

        std::fs::write(paths.config_file(), "sounds = \"ja\"\n").unwrap();
        assert!(matches!(
            first_run(&Options::default(), &paths),
            Err(Exit::Config(_))
        ));
    }

    #[test]
    fn in_the_terminal_nobody_is_shown_anything_or_stops_the_loop() {
        let out = Out {
            printer: Printer { verbose: false },
            link: None,
        };
        assert!(!out.has_screen());
        out.show(Event::ModelLoaded);
        assert!(!out.stop_asked());
    }
}
