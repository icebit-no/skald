//! Hold right ⌘ and see what each hold means: pressed, armed or not armed, released, cancelled.
//!
//! ```sh
//! cargo run -p skald-input --example hold                      # print only
//! cargo run -p skald-input --example hold -- --insert          # also insert a test text on release
//! cargo run -p skald-input --example hold -- --insert --keep   # and leave it on the clipboard
//! ```
//!
//! The terminal running it needs Accessibility (System Settings › Privacy & Security › Accessibility).

fn main() {
    #[cfg(target_os = "macos")]
    macos::main();
    #[cfg(not(target_os = "macos"))]
    eprintln!("This example runs on macOS only.");
}

#[cfg(target_os = "macos")]
mod macos {
    use std::time::Instant;

    use skald_input::{
        HoldEvent, HoldKey, HoldListener, InsertOptions, Inserter, ListenerEvent,
        open_accessibility_settings,
    };

    const TEST_TEXT: &str = "Hei, dette er en prøve.";

    pub fn main() {
        let args: Vec<String> = std::env::args().skip(1).collect();
        let insert = args.iter().any(|arg| arg == "--insert");
        let options = InsertOptions {
            keep_text_on_clipboard: args.iter().any(|arg| arg == "--keep"),
            ..InsertOptions::default()
        };

        let listener = HoldListener::start(HoldKey::PLATFORM_DEFAULT).expect("start the listener");
        let mut inserter = Inserter::new();
        let start = Instant::now();
        println!("Hold right ⌘; press any other key while holding to cancel. Ctrl-C quits.");
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
                        let _ = open_accessibility_settings();
                    }
                }
                ListenerEvent::Hold(HoldEvent::Pressed { .. }) => println!("{at}  pressed"),
                ListenerEvent::Hold(HoldEvent::NotArmed) => println!("{at}  not armed"),
                ListenerEvent::Hold(HoldEvent::Armed { pressed_at }) => {
                    let held = pressed_at.elapsed().as_millis();
                    println!("{at}  armed (key down {held} ms)");
                }
                ListenerEvent::Hold(HoldEvent::Cancelled) => println!("{at}  cancelled"),
                ListenerEvent::Hold(end @ (HoldEvent::Released | HoldEvent::ReleaseMissed)) => {
                    if end == HoldEvent::Released {
                        println!("{at}  released");
                    } else {
                        println!("{at}  release missed; the watchdog ended the take");
                    }
                    if insert {
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
