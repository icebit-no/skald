//! The command line. With no command, `skald` takes dictation. A command (`skald last`, `skald vocab add`,
//! …) is recognised and refused, since this build has none.

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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Invocation {
    Dictate(Options),
    Help,
    Version,
    /// A command this build does not have yet.
    Command(String),
}

/// Reads the arguments after the program's name. The error is the line to show the user.
pub(crate) fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Invocation, String> {
    let mut options = Options::default();
    let mut args = args.into_iter();
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
            "-h" | "--help" => return Ok(Invocation::Help),
            "-V" | "--version" => return Ok(Invocation::Version),
            flag if flag.starts_with('-') => return Err(format!("Ukjent valg: {flag}.")),
            command => return Ok(Invocation::Command(command.to_owned())),
        }
    }
    Ok(Invocation::Dictate(options))
}

pub(crate) const HELP: &str = "\
Skald: norsk tale til tekst, rett på maskinen.

Bruk: skald [valg]

Uten kommando tar skald imot diktering: hold høyre ⌘, snakk, slipp, og teksten
settes inn der markøren er. Ctrl-C avslutter.

Valg:
  --dir <mappe>    hold innstillinger, historikk og modeller i denne ene mappen
  --model <fil>    talemodellen (ellers <modeller>/<modell>/ggml-model.bin)
  --vad <fil>      Silero VAD (ellers <modeller>/silero-vad-v6.2-F32.gguf)
  -v, --verbose    vis tider for hvert opptak
  -h, --help       vis denne hjelpen
  -V, --version    vis versjonen";

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
        };
        let args = [
            "--dir",
            "/tmp/skald test",
            "--model",
            "m.bin",
            "-v",
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

    #[test]
    fn a_command_is_named_and_not_run() {
        assert_eq!(
            parse(&["last", "--json"]),
            Ok(Invocation::Command("last".into()))
        );
        assert_eq!(
            parse(&["-v", "vocab", "add", "merget"]),
            Ok(Invocation::Command("vocab".into()))
        );
    }

    #[test]
    fn mistakes_say_what_is_wrong_in_bokmål() {
        assert_eq!(parse(&["--model"]), Err("--model trenger en sti.".into()));
        assert_eq!(parse(&["--fast"]), Err("Ukjent valg: --fast.".into()));
    }
}
