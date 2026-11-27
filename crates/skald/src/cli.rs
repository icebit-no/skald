//! The command line. With no command, `skald` runs the app, or takes dictation in the terminal with
//! `--terminal`; with one (`skald last`, `skald vocab add`, …) the arguments go to skald-control (D-25),
//! with the folder `--dir` names, and `skald bench …` goes to skald-bench.

use std::ffi::OsString;
use std::path::PathBuf;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Options {
    /// Every file in this one folder instead of the system's own (`Paths::in_one_folder`), as for tests.
    pub(crate) dir: Option<PathBuf>,
    pub(crate) model: Option<PathBuf>,
    pub(crate) vad: Option<PathBuf>,
    /// Timings for every take.
    pub(crate) verbose: bool,
    /// Dictation in the terminal alone, without the overlay and the menu bar item, as for debugging.
    pub(crate) terminal: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Invocation {
    Dictate(Options),
    Help,
    Version,
    /// The command and everything after it, and the options before it this binary does not take, such as
    /// `--json`.
    Command {
        dir: Option<PathBuf>,
        args: Vec<OsString>,
    },
    /// The arguments after `bench`, and the options before it this binary does not take.
    Bench(Vec<OsString>),
}

/// What macOS adds when it starts the app from Finder or at login on some versions: the process serial
/// number, `-psn_0_12345`. It says nothing to Skald.
fn from_launch_services(arg: &OsString) -> bool {
    arg.to_str().is_some_and(|arg| arg.starts_with("-psn_"))
}

/// Reads the arguments after the program's name. The error is the line to show the user.
pub(crate) fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Invocation, String> {
    let mut options = Options::default();
    let mut theirs = Vec::new();
    let mut args = args.into_iter().filter(|arg| !from_launch_services(arg));
    while let Some(arg) = args.next() {
        let Some(flag) = arg.to_str() else {
            return Err(format!("Forstår ikke argumentet {}.", arg.display()));
        };
        let mut path = |name: &str| {
            args.next()
                .map(PathBuf::from)
                .ok_or_else(|| format!("{name} trenger en sti."))
        };
        match flag {
            "--dir" => options.dir = Some(path("--dir")?),
            "--model" => options.model = Some(path("--model")?),
            "--vad" => options.vad = Some(path("--vad")?),
            "-v" | "--verbose" => options.verbose = true,
            "--terminal" => options.terminal = true,
            "-h" | "--help" => return Ok(Invocation::Help),
            "-V" | "--version" => return Ok(Invocation::Version),
            flag if flag.starts_with('-') => theirs.push(arg),
            "bench" => {
                // skald-bench reads Skald's own folders and takes `--model` and `--vad` in its own form.
                if options != Options::default() {
                    return Err("Valgene til skald bench står etter «bench».".into());
                }
                theirs.extend(args);
                return Ok(Invocation::Bench(theirs));
            }
            _ => {
                theirs.push(arg);
                theirs.extend(args);
                return Ok(Invocation::Command {
                    dir: options.dir,
                    args: theirs,
                });
            }
        }
    }
    match theirs.first() {
        Some(flag) => Err(format!("Ukjent valg: {}.", flag.display())),
        None => Ok(Invocation::Dictate(options)),
    }
}

pub(crate) const HELP: &str = "\
Skald: norsk tale til tekst, rett på maskinen.

Bruk: skald [valg]

Uten kommando starter skald, med ᚨ i menylinjen: hold høyre ⌘, snakk, slipp, og
teksten settes inn der markøren er. ⌃⌘X starter og stopper et håndfritt opptak.
Avslutt Skald fra menyen, eller med Ctrl-C i terminalen.

Valg:
  --terminal       ta imot diktering i terminalen, uten overlegg og meny
  --dir <mappe>    hold innstillinger, historikk og modeller i denne ene mappen
  --model <fil>    talemodellen (ellers <modeller>/<modell>/ggml-model.bin)
  --vad <fil>      Silero VAD (ellers Skalds egen, i <modeller>)
  -v, --verbose    vis tider for hvert opptak
  -h, --help       vis denne hjelpen
  -V, --version    vis versjonen

Kommandoene for ordskatt, rettinger, innstillinger og historikk: skald help
Testsett gjennom konfigurasjonene: skald bench --help";

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Invocation, String> {
        super::parse(args.iter().map(OsString::from))
    }

    #[test]
    fn no_arguments_take_dictation_with_the_system_folders() {
        assert_eq!(parse(&[]), Ok(Invocation::Dictate(Options::default())));
    }

    #[test]
    fn every_option_is_read() {
        let options = Options {
            dir: Some(PathBuf::from("/tmp/skald test")),
            model: Some(PathBuf::from("m.bin")),
            vad: Some(PathBuf::from("v.gguf")),
            verbose: true,
            terminal: true,
        };
        let args = [
            "--dir",
            "/tmp/skald test",
            "--model",
            "m.bin",
            "-v",
            "--terminal",
            "--vad",
            "v.gguf",
        ];
        assert_eq!(parse(&args), Ok(Invocation::Dictate(options)));
        assert_eq!(
            parse(&["--verbose"]),
            Ok(Invocation::Dictate(Options {
                verbose: true,
                ..Options::default()
            }))
        );
    }

    #[test]
    fn help_and_version_win_over_the_rest() {
        assert_eq!(parse(&["-v", "--help"]), Ok(Invocation::Help));
        assert_eq!(parse(&["-h"]), Ok(Invocation::Help));
        assert_eq!(parse(&["--version"]), Ok(Invocation::Version));
        assert_eq!(parse(&["-V"]), Ok(Invocation::Version));
    }

    fn command(dir: Option<&str>, args: &[&str]) -> Result<Invocation, String> {
        Ok(Invocation::Command {
            dir: dir.map(PathBuf::from),
            args: args.iter().map(OsString::from).collect(),
        })
    }

    #[test]
    fn a_command_goes_on_with_everything_but_the_folder() {
        assert_eq!(
            parse(&["last", "--json"]),
            command(None, &["last", "--json"])
        );
        assert_eq!(
            parse(&["--dir", "/tmp/s", "--json", "vocab", "add", "--dir"]),
            command(Some("/tmp/s"), &["--json", "vocab", "add", "--dir"])
        );
        assert_eq!(
            parse(&["help"]),
            command(None, &["help"]),
            "the commands' own help"
        );
    }

    #[test]
    fn the_process_serial_number_macos_adds_is_ignored() {
        assert_eq!(
            parse(&["-psn_0_1234567"]),
            Ok(Invocation::Dictate(Options::default()))
        );
        assert_eq!(
            parse(&["--terminal", "-psn_0_42"]),
            Ok(Invocation::Dictate(Options {
                terminal: true,
                ..Options::default()
            }))
        );
        assert_eq!(
            parse(&["-psn_0_42", "last", "--json"]),
            command(None, &["last", "--json"])
        );
    }

    #[test]
    fn bench_goes_to_skald_bench_with_its_own_options() {
        let bench =
            |args: &[&str]| Ok(Invocation::Bench(args.iter().map(OsString::from).collect()));
        assert_eq!(
            parse(&["bench", "opptak", "--config", "greedy,no-trim"]),
            bench(&["opptak", "--config", "greedy,no-trim"])
        );
        assert_eq!(
            parse(&["--json", "bench", "opptak", "--vad", "v.gguf"]),
            bench(&["--json", "opptak", "--vad", "v.gguf"])
        );
        assert_eq!(parse(&["bench"]), bench(&[]));
        for before in [&["--dir", "/tmp/s"][..], &["--vad", "v.gguf"], &["-v"]] {
            let args: Vec<&str> = before.iter().chain(&["bench", "opptak"]).copied().collect();
            assert_eq!(
                parse(&args),
                Err("Valgene til skald bench står etter «bench».".into())
            );
        }
    }

    #[test]
    fn mistakes_say_what_is_wrong_in_bokmål() {
        assert_eq!(parse(&["--model"]), Err("--model trenger en sti.".into()));
        assert_eq!(parse(&["--fast"]), Err("Ukjent valg: --fast.".into()));
        assert_eq!(parse(&["--json"]), Err("Ukjent valg: --json.".into()));
    }
}
