//! Hold right ⌘ (macOS) or right Ctrl (Windows) and see what each hold means: pressed, armed or not
//! armed, released, cancelled. ⌃⌘X (Ctrl+Win+X on Windows) starts and stops a hands-free take.
//!
//! ```sh
//! cargo run -p skald-input --example hold                      # print only
//! cargo run -p skald-input --example hold -- --insert          # also insert a test text on release
//! cargo run -p skald-input --example hold -- --insert --keep   # and leave it on the clipboard
//! cargo run -p skald-input --example hold -- --insert --wait 3 # insert 3 s after the release
//! ```
//!
//! The wait leaves time to click into another window, which during a hold would cancel the take.
//!
//! On macOS the terminal running it needs Accessibility (System Settings › Privacy & Security ›
//! Accessibility). On Windows a window run as administrator neither shows the key to it nor takes its
//! paste.

fn main() {
    #[cfg(any(target_os = "macos", windows))]
    listen::main();
    #[cfg(not(any(target_os = "macos", windows)))]
    eprintln!("This example runs on macOS and Windows only.");
}

#[cfg(any(target_os = "macos", windows))]
mod listen {
    use std::time::{Duration, Instant};

    use skald_input::{HoldEvent, HoldKey, HoldListener, InsertOptions, Inserter, ListenerEvent};

    const KEY: &str = if cfg!(target_os = "macos") {
        "right ⌘"
    } else {
        "right Ctrl"
    };
    const CHORD: &str = if cfg!(target_os = "macos") {
        "⌃⌘X"
    } else {
        "Ctrl+Win+X"
    };

    const TEST_TEXT: &str = "Hei, dette er en prøve.";

    pub fn main() {
        let args: Vec<String> = std::env::args().skip(1).collect();
        let insert = args.iter().any(|arg| arg == "--insert");
        let wait = args
            .iter()
            .position(|arg| arg == "--wait")
            .and_then(|at| args.get(at + 1))
            .map(|seconds| Duration::from_secs_f64(seconds.parse().expect("--wait takes seconds")))
            .unwrap_or_default();
        let options = InsertOptions {
            keep_text_on_clipboard: args.iter().any(|arg| arg == "--keep"),
            ..InsertOptions::default()
        };

        let listener = HoldListener::start(HoldKey::PLATFORM_DEFAULT).expect("start the listener");
        let mut inserter = Inserter::new();
        let start = Instant::now();
        println!("Hold {KEY}; press any other key while holding to cancel. Ctrl-C quits.");
        if insert {
            println!("On release, «{TEST_TEXT}» is inserted where the cursor is.");
        }

        for event in listener.events() {
            let at = format!("{:>7.2} s", start.elapsed().as_secs_f64());
            match event {
                ListenerEvent::Ready => println!("{at}  listening"),
                ListenerEvent::Unavailable(reason) => {
                    println!("{at}  not listening: {reason}");
                    if reason == skald_input::Unavailable::NoAccessibility {
                        println!(
                            "          grant Accessibility to this terminal; listening resumes by itself"
                        );
                        #[cfg(target_os = "macos")]
                        let _ = skald_input::open_accessibility_settings();
                    }
                }
                ListenerEvent::Hold(HoldEvent::Pressed { .. }) => println!("{at}  pressed"),
                ListenerEvent::Hold(HoldEvent::NotArmed) => println!("{at}  not armed"),
                ListenerEvent::Hold(HoldEvent::Armed { pressed_at }) => {
                    let held = pressed_at.elapsed().as_millis();
                    println!("{at}  armed (key down {held} ms)");
                }
                ListenerEvent::Hold(HoldEvent::Cancelled) => println!("{at}  cancelled"),
                ListenerEvent::Hold(HoldEvent::HandsFreeStarted { .. }) => {
                    println!("{at}  hands-free started; {CHORD} stops it");
                }
                ListenerEvent::Hold(
                    end @ (HoldEvent::Released
                    | HoldEvent::ReleaseMissed
                    | HoldEvent::HandsFreeStopped),
                ) => {
                    match end {
                        HoldEvent::Released => println!("{at}  released"),
                        HoldEvent::ReleaseMissed => {
                            println!("{at}  release missed; the watchdog ended the take");
                        }
                        _ => println!("{at}  hands-free stopped"),
                    }
                    if insert {
                        std::thread::sleep(wait);
                        match inserter.insert(TEST_TEXT, &options) {
                            Ok(outcome) => println!("          inserted: {outcome:?}"),
                            Err(error) => println!("          not inserted: {error}"),
                        }
                    }
                }
            }
        }
    }
}
