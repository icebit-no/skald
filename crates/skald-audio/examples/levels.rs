//! Lists the microphones and prints the level of one for a few seconds. No audio is saved.
//!
//! ```sh
//! cargo run -p skald-audio --example levels              # the microphone Skald would choose
//! cargo run -p skald-audio --example levels -- Yeti      # the first whose name contains "Yeti"
//! cargo run -p skald-audio --example levels -- --list    # list only; opens no microphone
//! cargo run -p skald-audio --example levels -- --watch   # report changes; opens no microphone
//! ```

use std::error::Error;
use std::process::ExitCode;
use std::thread;
use std::time::{Duration, Instant};

use skald_audio::{
    Connected, Microphone, MicrophoneWatcher, Take, TakeEvent, choose, connected_microphones,
};

const RECORD_FOR: Duration = Duration::from_secs(5);
const WATCH_FOR: Duration = Duration::from_secs(120);

fn main() -> Result<ExitCode, Box<dyn Error>> {
    let argument = std::env::args().nth(1);
    let connected = connected_microphones()?;
    let chosen = match argument.as_deref() {
        None | Some("--list" | "--watch") => choose(&[], &connected),
        Some(part) => connected.microphones.iter().find(|m| m.name.contains(part)),
    };
    print_list(&connected, chosen);

    match argument.as_deref() {
        Some("--list") => Ok(ExitCode::SUCCESS),
        Some("--watch") => watch(),
        _ => match chosen {
            Some(microphone) => record(microphone),
            None => {
                eprintln!("No microphone matches.");
                Ok(ExitCode::FAILURE)
            }
        },
    }
}

fn print_list(connected: &Connected, chosen: Option<&Microphone>) {
    println!("Connected microphones (> is the one used):");
    for microphone in &connected.microphones {
        let marker = if Some(microphone) == chosen { ">" } else { " " };
        let default = if connected.default.as_ref() == Some(&microphone.id) {
            "  [system default]"
        } else {
            ""
        };
        println!(
            "{marker} {}{default}\n      {}",
            microphone.name, microphone.id
        );
    }
}

fn record(microphone: &Microphone) -> Result<ExitCode, Box<dyn Error>> {
    println!(
        "\nListening to {} for {} s. Nothing is saved.\n",
        microphone.name,
        RECORD_FOR.as_secs()
    );
    let take = Take::start(microphone)?;
    let until = Instant::now() + RECORD_FOR;
    while Instant::now() < until {
        for event in take.events().try_iter() {
            match event {
                TakeEvent::Level(dbfs) => println!("{dbfs:7.1} dBFS {}", bar(dbfs)),
                other => println!("{other:?}"),
            }
        }
        thread::sleep(Duration::from_millis(20));
    }
    let recording = take.stop();
    println!(
        "\n{:.2} s, {} samples at 16 kHz, loudest {:.1} dBFS, ended {:?}",
        recording.duration().as_secs_f64(),
        recording.samples.len(),
        recording.loudest_dbfs(),
        recording.ending,
    );
    Ok(ExitCode::SUCCESS)
}

fn watch() -> Result<ExitCode, Box<dyn Error>> {
    let (watcher, changes) = MicrophoneWatcher::start(Vec::new())?;
    let name = |m: Option<Microphone>| m.map_or("none".to_owned(), |m| m.name);
    println!(
        "\nIn use: {}. Plug microphones in and out; watching for {} s.",
        name(watcher.active()),
        WATCH_FOR.as_secs()
    );
    let until = Instant::now() + WATCH_FOR;
    while let Ok(change) = changes.recv_timeout(until.saturating_duration_since(Instant::now())) {
        println!("{} -> {}", name(change.from), name(change.to));
    }
    Ok(ExitCode::SUCCESS)
}

/// −60 dBFS and below is empty; 0 dBFS is 60 characters.
fn bar(dbfs: f32) -> String {
    "█".repeat((dbfs.max(-60.0) + 60.0).round() as usize)
}
