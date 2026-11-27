//! `skald`: with no command it is the app, in the menu bar with its overlay and settings window, or with
//! `--terminal` dictation in the terminal alone; with a command it is the command line an agent uses
//! (D-25), which talks to the running Skald or works on the files; `skald bench` runs a test set.

// Off macOS parts of the binary are unused: Linux and Windows stand in for the system's permissions and
// start at login, and elsewhere only the parts without a key listener or a clipboard build.
#![cfg_attr(not(target_os = "macos"), allow(dead_code))]
// On Windows a windowed program, so no console opens beside the app; a command attaches to the console it
// was run from instead (D-49).
#![cfg_attr(all(windows, not(test)), windows_subsystem = "windows")]

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
mod app;
mod cli;
mod control;
mod dictation;
mod events;
mod files;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod login;
mod models;
#[cfg(target_os = "macos")]
mod permissions;
mod pipeline;
mod say;
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
mod screen;
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
mod shell;
mod signals;
mod sounds;
mod vad;
mod wav;
#[cfg(windows)]
mod windows;

use std::process::ExitCode;

use cli::Invocation;
#[cfg(target_os = "linux")]
use linux::{login, permissions};
use say::Exit;
#[cfg(windows)]
use windows::{login, permissions};

fn main() -> ExitCode {
    let invocation = cli::parse(std::env::args_os().skip(1));
    #[cfg(windows)]
    if !matches!(&invocation, Ok(Invocation::Dictate(options)) if !options.terminal) {
        windows::console::attach_to_parent();
    }
    let result = match invocation {
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
        Ok(Invocation::Bench(args)) => return bench(args),
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

/// Runs skald-bench, with the built-in Silero VAD put in the models folder first, where it reads it unless
/// given `--vad`.
fn bench(args: Vec<std::ffi::OsString>) -> ExitCode {
    if !args.iter().any(|arg| arg == "--vad")
        && let Ok(paths) = files::paths(None)
        && let Err(error) = vad::put_in_place(&skald_models::vad_path(&paths.models_dir()))
    {
        eprintln!(
            "{}",
            Exit::VadUnwritable {
                path: skald_models::vad_path(&paths.models_dir()),
                reason: error.to_string(),
            }
        );
    }
    skald_bench::run(args)
}

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
fn dictate(options: cli::Options) -> Result<(), Exit> {
    app::run(options)
}

#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
fn dictate(_options: cli::Options) -> Result<(), Exit> {
    Err(Exit::UnsupportedSystem)
}
