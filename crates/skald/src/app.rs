//! Skald in a terminal on macOS: the hold key, the microphones and the model joined into one loop, each
//! line a user reads printed as it happens.
//!
//! Start-up checks what can stop Skald before the model takes seconds to load: the files, Accessibility,
//! then the microphone permission. The model loads on the pipeline's thread while the keys already work,
//! so a take made meanwhile waits for it.

use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use skald_audio::MicrophoneAccess;
use skald_core::config::HoldKey as ConfigHoldKey;
use skald_core::paths::{Env, Os, Paths};
use skald_core::state::State;
use skald_engine::Engine;
use skald_input::{HoldKey, HoldListener, Inserter, ListenerEvent, Unavailable};

use crate::cli::Options;
use crate::dictation::{Dictation, Microphones, SystemMicrophones, microphone_order};
use crate::files::{self, ConfigFile};
use crate::models::ModelFiles;
use crate::pipeline::{Worker, WorkerEvent};
use crate::say::{Exit, Line, Printer, report_lines};
use crate::signals;
use crate::sounds::{Afplay, Chime};

/// The longest the loop waits for a key before it looks at everything else.
const TICK: Duration = Duration::from_millis(100);

const CONFIG_EVERY: Duration = Duration::from_secs(1);

/// The key listener says within this time whether it may listen.
const FIRST_KEY_EVENT_WITHIN: Duration = Duration::from_secs(2);

pub(crate) fn run(options: Options) -> Result<(), Exit> {
    let result = run_until_stopped(options);
    // Opening the right pane of System Settings saves the user looking for it.
    match result {
        Err(Exit::NoAccessibility) => {
            let _ = skald_input::open_accessibility_settings();
        }
        Err(Exit::MicrophoneDenied) => {
            let _ = std::process::Command::new("open")
                .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone")
                .status();
        }
        _ => {}
    }
    result
}

fn run_until_stopped(options: Options) -> Result<(), Exit> {
    let out = Printer {
        verbose: options.verbose,
    };
    out.say(Line::Version);
    let paths = match &options.dir {
        Some(dir) => Paths::in_one_folder(dir),
        None => Paths::for_os(Os::CURRENT, &Env::from_process()).ok_or(Exit::NoHome)?,
    };
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
    models.check(out)?;
    let state = files::read_state(&paths.state_file()).unwrap_or_else(|reason| {
        out.say(Line::StateUnreadable {
            path: paths.state_file(),
            reason,
        });
        State::default()
    });
    signals::install().map_err(|error| Exit::CannotStart(error.to_string()))?;

    let mut keys = Keys::start(hold_key(&config.hold_key, out))?;
    keys.wait_until_known(out)?;

    match skald_audio::microphone_access() {
        MicrophoneAccess::Denied => return Err(Exit::MicrophoneDenied),
        MicrophoneAccess::Restricted => return Err(Exit::MicrophoneRestricted),
        MicrophoneAccess::NotAsked => out.say(Line::MicrophoneNotAsked),
        MicrophoneAccess::Granted => {}
    }
    let microphones =
        SystemMicrophones::start(microphone_order(&config)).map_err(Exit::Microphones)?;
    out.say(Line::Microphone(microphones.active().map(|m| m.name)));

    out.say(Line::LoadingModel);
    let chime: Arc<dyn Chime> = Arc::new(Afplay);
    let (events, worker_events) = mpsc::channel();
    let ModelFiles { model, vad, .. } = models;
    let worker = Worker::spawn(
        move || {
            skald_engine::route_logs_to_log_crate();
            Engine::load(&model, &vad).map_err(|error| error.to_string())
        },
        Inserter::new(),
        Arc::clone(&chime),
        state,
        paths.state_file(),
        events,
    )
    .map_err(|error| Exit::CannotStart(error.to_string()))?;
    let dictation = Dictation::new(microphones, chime, Arc::new(config));

    let mut running = Running {
        keys,
        dictation,
        worker: &worker,
        worker_events,
        config_file,
        model_given: options.model.is_some(),
        out,
    };
    let result = running.listen();
    // The event tap goes first, then the microphones; the worker finishes the takes it has and drops the
    // model last.
    let Running {
        keys, dictation, ..
    } = running;
    drop(keys);
    drop(dictation);
    worker.finish();
    result
}

struct Running<'w> {
    keys: Keys,
    dictation: Dictation<SystemMicrophones>,
    worker: &'w Worker,
    worker_events: Receiver<WorkerEvent>,
    config_file: ConfigFile,
    model_given: bool,
    out: Printer,
}

impl Running<'_> {
    fn listen(&mut self) -> Result<(), Exit> {
        let mut next_config_look = Instant::now() + CONFIG_EVERY;
        let mut wanted_key = self.keys.key;
        loop {
            if signals::received() {
                self.out.say(Line::Stopping);
                return Ok(());
            }
            if let Some(event) = self.keys.next(TICK)? {
                self.on_key_event(event)?;
            }
            self.dictation.poll();
            for line in self.dictation.take_lines() {
                self.out.say(line);
            }
            for job in self.dictation.take_jobs() {
                self.worker.submit(job);
            }
            for event in self.worker_events.try_iter() {
                match event {
                    WorkerEvent::Loaded { device, took } => {
                        self.out.say(Line::ModelLoaded { device, took });
                        self.out.say(Line::Ready(self.keys.key));
                    }
                    WorkerEvent::LoadFailed(reason) => return Err(Exit::ModelLoad(reason)),
                    WorkerEvent::Done(report) => {
                        for line in report_lines(&report) {
                            self.out.say(line);
                        }
                    }
                }
            }
            if Instant::now() >= next_config_look {
                next_config_look = Instant::now() + CONFIG_EVERY;
                if let Some(key) = self.look_at_config() {
                    wanted_key = key;
                }
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
        let key = hold_key(&config.hold_key, self.out);
        self.dictation.set_config(Arc::new(config));
        Some(key)
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
    fn wait_until_known(&mut self, out: Printer) -> Result<(), Exit> {
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
fn hold_key(key: &ConfigHoldKey, out: Printer) -> HoldKey {
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
}
