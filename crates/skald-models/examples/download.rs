//! Lists the models and what of each is in the models folder; downloads one only when asked to.
//!
//! ```sh
//! cargo run -p skald-models --example download                  # the list; no network
//! cargo run -p skald-models --example download -- --download nb-whisper-large [--dir <folder>]
//! ```
//!
//! The download is the real file (3.1 GB for NB-Whisper Large). Stopping it with Ctrl-C keeps what arrived,
//! and running it again goes on from there.

use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

use skald_core::paths::{Env, Os, Paths};
use skald_models::{Availability, Cancel, Downloader, Event, MODELS};

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let mut download = None;
    let mut dir = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--download" => download = args.next(),
            "--dir" => dir = args.next().map(PathBuf::from),
            _ => {
                eprintln!("usage: download [--download <model id>] [--dir <models folder>]");
                return ExitCode::FAILURE;
            }
        }
    }
    let Some(models_dir) = dir.or_else(|| {
        Paths::for_os(Os::CURRENT, &Env::from_process()).map(|paths| paths.models_dir())
    }) else {
        eprintln!("no home folder to find the models folder in; give one with --dir");
        return ExitCode::FAILURE;
    };

    println!("models folder: {}", models_dir.display());
    for model in MODELS {
        let state = match &model.availability {
            Availability::Download(_) => format!("{:?}", model.status(&models_dir).unwrap()),
            other => format!("{other:?}"),
        };
        let beta = if model.beta { " (beta)" } else { "" };
        println!(
            "  {:<18} {}{beta}: {state}; {}, {}",
            model.id, model.name, model.licence, model.credit
        );
    }

    let Some(id) = download else {
        return ExitCode::SUCCESS;
    };
    let Some(model) = skald_models::find(&id) else {
        eprintln!("no model with the id {id}");
        return ExitCode::FAILURE;
    };
    let Some(file) = model.file() else {
        eprintln!("{} cannot be downloaded yet", model.name);
        return ExitCode::FAILURE;
    };

    let started = Instant::now();
    let mut first = None;
    let result =
        Downloader::new().download(file, &model.dir(&models_dir), &Cancel::new(), |event| {
            match event {
                Event::Progress { received, total } => {
                    let from = *first.get_or_insert(received);
                    let seconds = started.elapsed().as_secs_f64().max(0.001);
                    let rate = (received - from) as f64 / seconds / 1e6;
                    print!(
                        "\r{:5.1} %  {:.2} of {:.2} GB  {rate:.1} MB/s   ",
                        received as f64 * 100.0 / total as f64,
                        received as f64 / 1e9,
                        total as f64 / 1e9,
                    );
                    let _ = std::io::stdout().flush();
                }
                Event::Retrying { attempt, of } => {
                    println!("\nthe connection broke; trying again ({attempt} of {of})");
                }
            }
        });
    println!();
    match result {
        Ok(path) => {
            println!("checked and in place: {}", path.display());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
