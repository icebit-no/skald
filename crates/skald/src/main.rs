//! `skald`: with no command it takes dictation in the terminal; with a command it is the command line an
//! agent uses (D-25), which talks to the running Skald or works on the files.

// Off macOS only the parts without a key listener or a clipboard build, so most of the binary is unused.
#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

#[cfg(target_os = "macos")]
mod app;
mod cli;
mod control;
mod dictation;
mod files;
mod models;
mod pipeline;
mod say;
mod signals;
mod sounds;
mod wav;

use std::process::ExitCode;

use cli::Invocation;
use say::Exit;

fn main() -> ExitCode {
    let result = match cli::parse(std::env::args_os().skip(1)) {
        Ok(Invocation::Help) => {
            println!("{}", cli::HELP);
            Ok(())
        }
        Ok(Invocation::Version) => {
            println!("skald {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Ok(Invocation::Command { dir, args }) => {
            return match files::paths(dir.as_deref()) {
                Ok(paths) => {
                    skald_control::run(args, &paths, &control::LocalMachine::new(paths.clone()))
                }
                Err(exit) => {
                    eprintln!("{exit}");
                    ExitCode::from(exit.code())
                }
            };
        }
        Ok(Invocation::Dictate(options)) => dictate(options),
        Err(message) => Err(Exit::Usage(message)),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(exit) => {
            eprintln!("{exit}");
            ExitCode::from(exit.code())
        }
    }
}

#[cfg(target_os = "macos")]
fn dictate(options: cli::Options) -> Result<(), Exit> {
    app::run(options)
}

#[cfg(not(target_os = "macos"))]
fn dictate(_options: cli::Options) -> Result<(), Exit> {
    Err(Exit::UnsupportedSystem)
}
