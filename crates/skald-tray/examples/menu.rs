//! Puts Skald's item in the menu bar with sample data and prints each action its menu sends. A take
//! plays on the icon every ten seconds: the red dot for four seconds, the gold dot for two, then a new
//! text under «Siste tekster». Choosing a microphone, «Bruk den første som er koblet til» or a written
//! standard changes the menu as the app would; «Avslutt Skald» ends the example. It needs no
//! permissions. On Windows the item is the icon in the notification area.
//!
//! ```sh
//! cargo run -p skald-tray --example menu
//! ```

#[cfg(target_os = "macos")]
fn main() {
    mac::run();
}

#[cfg(windows)]
fn main() {
    win::run();
}

#[cfg(not(any(target_os = "macos", windows)))]
fn main() {
    eprintln!("The menu bar item exists on macOS and Windows only so far.");
}

/// A take every ten seconds, and what the app would do with each action.
#[cfg(any(target_os = "macos", windows))]
mod sample {
    use std::time::{Duration, Instant};

    use skald_core::config::{HoldKey, Language};
    use skald_tray::{Action, Activity, Microphone, TrayModel};

    const TAKE_EVERY: Duration = Duration::from_secs(10);
    const RECORDING_FROM: Duration = Duration::from_secs(4);
    const TRANSCRIBING_FROM: Duration = Duration::from_secs(8);

    const SAMPLE_TEXTS: [&str; 6] = [
        "Hei, dette er en prøve.",
        "Kan du merge pull-requesten etter lunsj?",
        "Møtet er flyttet til torsdag klokka ti, og vi tar det på Teams i stedet for på kontoret.",
        "Husk å kjøpe brød og melk på vei hjem.",
        "Rettingen fra «mercha» til «merget» virker.",
        "Ordskatten har plass til de viktigste navnene til slutt.",
    ];

    pub struct Takes {
        started: Instant,
        taken: usize,
    }

    impl Takes {
        pub fn new() -> Self {
            Self {
                started: Instant::now(),
                taken: 0,
            }
        }

        /// Moves the model to this point of the take's cycle; a take that has been transcribed adds its
        /// text under «Siste tekster».
        pub fn tick(&mut self, model: &mut TrayModel) {
            let activity = activity_at(self.started.elapsed());
            if model.activity == Activity::Transcribing && activity == Activity::Idle {
                let text = SAMPLE_TEXTS[self.taken % SAMPLE_TEXTS.len()];
                self.taken += 1;
                model.latest_texts.insert(0, text.to_owned());
                model.latest_texts.truncate(5);
            }
            model.activity = activity;
        }
    }

    /// What a take is doing at this point of the ten-second cycle.
    fn activity_at(elapsed: Duration) -> Activity {
        let into_take = Duration::from_nanos((elapsed.as_nanos() % TAKE_EVERY.as_nanos()) as u64);
        if into_take >= TRANSCRIBING_FROM {
            Activity::Transcribing
        } else if into_take >= RECORDING_FROM {
            Activity::Recording
        } else {
            Activity::Idle
        }
    }

    /// The AirPods are pinned but unplugged, so the order applies and the Shure is in use.
    pub fn model() -> TrayModel {
        let microphone = |id: &str, name: &str, connected| Microphone {
            id: id.to_owned(),
            name: name.to_owned(),
            connected,
        };
        TrayModel {
            activity: Activity::Idle,
            hold_key: if cfg!(target_os = "macos") {
                HoldKey::RightCommand
            } else {
                HoldKey::RightControl
            },
            language: Language::Bokmal,
            microphones: vec![
                microphone("airpods", "AirPods Pro", false),
                microphone("usb", "Shure MV7", true),
                microphone("built-in", "Innebygd mikrofon", true),
            ],
            in_use: Some("usb".to_owned()),
            pinned: Some("airpods".to_owned()),
            latest_texts: SAMPLE_TEXTS[..2]
                .iter()
                .rev()
                .map(|&t| t.to_owned())
                .collect(),
        }
    }

    /// The app's part, roughly: a pin wins while its microphone is connected, else the first connected
    /// one is used. True for «Avslutt Skald».
    pub fn act(model: &mut TrayModel, action: Action) -> bool {
        match action {
            Action::PinMicrophone(id) => model.pinned = Some(id),
            Action::UnpinMicrophone => model.pinned = None,
            Action::SetLanguage(language) => model.language = language,
            Action::CopyText(text) => println!("  the app copies {text:?}"),
            Action::OpenVocabulary | Action::OpenSettings => println!("  the app opens settings"),
            Action::Quit => return true,
        }
        let connected = |id: &String| model.microphones.iter().any(|m| &m.id == id && m.connected);
        model.in_use = model.pinned.clone().filter(connected).or_else(|| {
            model
                .microphones
                .iter()
                .find(|m| m.connected)
                .map(|m| m.id.clone())
        });
        false
    }
}

#[cfg(target_os = "macos")]
mod mac {
    use std::cell::RefCell;
    use std::ptr::NonNull;
    use std::rc::Rc;

    use block2::RcBlock;
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};
    use objc2_foundation::NSTimer;
    use skald_tray::{Action, Tray};

    use crate::sample::{self, Takes};

    pub fn run() {
        let mtm = MainThreadMarker::new().expect("main runs on the main thread");
        let app = NSApplication::sharedApplication(mtm);
        // A menu bar item with no Dock icon, as Skald runs.
        app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);

        let model = Rc::new(RefCell::new(sample::model()));
        let chosen = Rc::new(RefCell::new(Vec::new()));
        let inbox = chosen.clone();
        let tray = Tray::new(&model.borrow(), move |action| {
            println!("{action:?}");
            inbox.borrow_mut().push(action);
        })
        .expect("the item is made on the main thread");
        let tray = RefCell::new(tray);
        println!("Skald's item is in the menu bar; choose its entries, «Avslutt Skald» ends.");

        let takes = RefCell::new(Takes::new());
        let tick = RcBlock::new(move |_timer: NonNull<NSTimer>| {
            let mut model = model.borrow_mut();
            let actions: Vec<Action> = chosen.borrow_mut().drain(..).collect();
            for action in actions {
                if sample::act(&mut model, action) {
                    NSApplication::sharedApplication(mtm).terminate(None);
                }
            }
            takes.borrow_mut().tick(&mut model);
            tray.borrow_mut().update(&model);
        });
        // SAFETY: the block takes the timer it is given and nothing else; the run loop keeps the
        // timer.
        let _timer =
            unsafe { NSTimer::scheduledTimerWithTimeInterval_repeats_block(0.1, true, &tick) };
        app.run();
    }
}

#[cfg(windows)]
mod win {
    use std::cell::RefCell;
    use std::ptr::null_mut;
    use std::rc::Rc;

    use skald_tray::{Action, Tray};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, GetMessageW, MSG, SetTimer, TranslateMessage, WM_TIMER,
    };

    use crate::sample::{self, Takes};

    pub fn run() {
        let mut model = sample::model();
        let chosen = Rc::new(RefCell::new(Vec::new()));
        let inbox = chosen.clone();
        let mut tray = Tray::new(&model, move |action| {
            println!("{action:?}");
            inbox.borrow_mut().push(action);
        })
        .expect("the notification area takes the icon");
        println!("Skald's icon is in the notification area; click it, «Avslutt Skald» ends.");

        let mut takes = Takes::new();
        // Safety: a timer for this thread's message queue, with no window and no procedure.
        unsafe { SetTimer(null_mut(), 0, 100, None) };
        // Safety: a valid message structure, filled by the system and handed back to it.
        let mut message: MSG = unsafe { std::mem::zeroed() };
        while unsafe { GetMessageW(&mut message, null_mut(), 0, 0) } > 0 {
            unsafe {
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
            if message.message != WM_TIMER {
                continue;
            }
            let actions: Vec<Action> = chosen.borrow_mut().drain(..).collect();
            for action in actions {
                if sample::act(&mut model, action) {
                    return;
                }
            }
            takes.tick(&mut model);
            tray.update(&model);
        }
    }
}
