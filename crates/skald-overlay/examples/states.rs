//! Every state of the overlay in every theme, one after the other, with sample levels and text:
//!
//! ```sh
//! cargo run -p skald-overlay --example states
//! cargo run -p skald-overlay --example states -- --theme kjedelig --still
//! ```
//!
//! `--theme runepinne|smidd-jern|kjedelig` shows one theme only, `--still` shows reduced motion. The
//! frame rate is printed while the pill shows. Ctrl-C ends it.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui_kit::{ActivationPolicy, App, AsyncApp, QuitMode};
use skald_core::config::OverlayTheme;
use skald_overlay::{Motion, Overlay};

const LONG_TEXT: &str = "Kan du sende meg notatene fra møtet i går, så ser jeg på dem før lunsj.";
const SHORT_TEXT: &str = "Hei, dette er en prøve.";
const BUILT_IN: &str = "MacBook Pro-mikrofon";
const PLUGGED_IN: &str = "Shure MV7";

fn main() {
    let arguments: Vec<String> = std::env::args().collect();
    let still = arguments.iter().any(|argument| argument == "--still");
    let themes: Vec<OverlayTheme> =
        match arguments.iter().position(|argument| argument == "--theme") {
            Some(at) => vec![OverlayTheme::from(
                arguments.get(at + 1).cloned().unwrap_or_default(),
            )],
            None => vec![
                OverlayTheme::Runepinne,
                OverlayTheme::SmiddJern,
                OverlayTheme::Kjedelig,
            ],
        };

    let application = gpui_kit::application().with_quit_mode(QuitMode::Explicit);
    application.run(move |cx: &mut App| {
        // No Dock icon and no menu bar, as Skald itself runs.
        cx.set_activation_policy(ActivationPolicy::Accessory);
        let overlay = Rc::new(RefCell::new(Overlay::new(&themes[0], cx)));
        if still {
            overlay.borrow_mut().set_motion(Some(Motion::Reduced), cx);
        }
        cx.spawn(async move |cx| {
            loop {
                for theme in &themes {
                    println!("── {theme:?}");
                    cx.update(|cx| overlay.borrow_mut().set_theme(theme, cx));
                    show_every_state(&overlay, cx).await;
                }
            }
        })
        .detach();
    });
}

async fn show_every_state(overlay: &Rc<RefCell<Overlay>>, cx: &mut AsyncApp) {
    let act = |cx: &mut AsyncApp, action: &dyn Fn(&mut Overlay, &mut App)| {
        cx.update(|cx| action(&mut overlay.borrow_mut(), cx));
    };

    println!("Opptak, Rister with the text early, Satt inn");
    act(cx, &|overlay, cx| overlay.record(cx));
    speak(overlay, cx, Duration::from_millis(3400), 1.0).await;
    act(cx, &|overlay, cx| overlay.release(cx));
    wait(overlay, cx, Duration::from_millis(250)).await;
    act(cx, &|overlay, cx| overlay.inserted(LONG_TEXT, cx));
    wait(overlay, cx, Duration::from_millis(2600)).await;

    println!("Opptak, Rister with the text late, Satt inn");
    act(cx, &|overlay, cx| overlay.record(cx));
    speak(overlay, cx, Duration::from_millis(2400), 1.0).await;
    act(cx, &|overlay, cx| overlay.release(cx));
    wait(overlay, cx, Duration::from_millis(1400)).await;
    act(cx, &|overlay, cx| overlay.inserted(SHORT_TEXT, cx));
    wait(overlay, cx, Duration::from_millis(2600)).await;

    println!("Hørte ingenting");
    act(cx, &|overlay, cx| overlay.record(cx));
    speak(overlay, cx, Duration::from_millis(1500), 0.05).await;
    act(cx, &|overlay, cx| overlay.release(cx));
    wait(overlay, cx, Duration::from_millis(300)).await;
    act(cx, &|overlay, cx| overlay.heard_nothing(BUILT_IN, cx));
    wait(overlay, cx, Duration::from_millis(2600)).await;

    println!("Mikrofon byttet");
    act(cx, &|overlay, cx| {
        overlay.microphone_changed(PLUGGED_IN, cx)
    });
    wait(overlay, cx, Duration::from_millis(3100)).await;

    println!("Håndfritt");
    act(cx, &|overlay, cx| overlay.record_hands_free(cx));
    speak(overlay, cx, Duration::from_millis(3000), 1.0).await;
    act(cx, &|overlay, cx| overlay.release(cx));
    wait(overlay, cx, Duration::from_millis(600)).await;
    act(cx, &|overlay, cx| overlay.inserted("Ferdig.", cx));
    wait(overlay, cx, Duration::from_millis(2800)).await;
}

/// A level reading every 60 ms for `length`, shaped like speech: phrases, syllables and pauses.
async fn speak(overlay: &Rc<RefCell<Overlay>>, cx: &mut AsyncApp, length: Duration, loudness: f32) {
    let start = Instant::now();
    let mut reading = 0u32;
    while start.elapsed() < length {
        let level = speech(reading) * loudness;
        // The overlay draws −60 dBFS and below as its lowest bar and −15 dBFS as its highest.
        let dbfs = -60.0 + 45.0 * level;
        cx.update(|cx| overlay.borrow_mut().level(dbfs, cx));
        reading += 1;
        wait(overlay, cx, Duration::from_millis(60)).await;
    }
}

/// Waits, printing the frame rate once a second.
async fn wait(overlay: &Rc<RefCell<Overlay>>, cx: &mut AsyncApp, length: Duration) {
    thread_local! {
        static LAST: RefCell<(Instant, u64)> = RefCell::new((Instant::now(), 0));
    }
    cx.background_executor().timer(length).await;
    let frames = cx.update(|cx| overlay.borrow().frames_drawn(cx));
    LAST.with(|last| {
        let mut last = last.borrow_mut();
        let seconds = last.0.elapsed().as_secs_f64();
        if seconds >= 1.0 {
            let rate = (frames - last.1) as f64 / seconds;
            if rate > 0.0 {
                println!("   {rate:.0} frames a second");
            }
            *last = (Instant::now(), frames);
        }
    });
}

/// A speech-like level from 0 to 1 for reading `index`.
fn speech(index: u32) -> f32 {
    let t = index as f32 * 0.06;
    let phrase = smooth(((noise(t * 0.6, 5) - 0.25) * 4.0).clamp(0.0, 1.0));
    let syllable = smooth(((noise(t * 3.6, 9) - 0.2) * 1.7).clamp(0.0, 1.0));
    let fine = noise(t * 11.0, 2);
    (phrase * (0.06 + 1.25 * syllable * syllable * (0.45 + 0.75 * fine)) + 0.015).clamp(0.0, 1.0)
}

fn noise(x: f32, seed: u32) -> f32 {
    let lattice = |i: i32| {
        let mut h = (i as u32).wrapping_mul(374_761_393) ^ seed.wrapping_mul(1_442_695_041);
        h = (h ^ (h >> 13)).wrapping_mul(1_274_126_177);
        (h ^ (h >> 16)) as f32 / u32::MAX as f32
    };
    let i = x.floor();
    let f = x - i;
    let (a, b) = (lattice(i as i32), lattice(i as i32 + 1));
    a + (b - a) * smooth(f)
}

fn smooth(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}
