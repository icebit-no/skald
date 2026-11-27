//! `skald bench record <folder> --microphone <label>`: records the test set's sentences, one take each,
//! into a folder `skald bench` reads (the `set` module): `<label>-<nn>.wav`, 16 kHz mono, its sentence in
//! `<label>-<nn>.txt` as the reference, and a line in `manifest.toml` naming the microphone. It plays
//! nothing back.
//!
//! The folder is all the state there is, so a run that stopped carries on where it stopped
//! (the `session` module has the order). The sentences are research/test-set/sentences.txt, built in.

use std::ffi::OsString;
use std::fmt;
use std::io::{self, IsTerminal};
use std::path::PathBuf;
use std::process::ExitCode;

use skald_audio::MicrophoneAccess;

use crate::{Code, Failure};

mod folder;
mod microphone;
mod session;
mod terminal;

use folder::Folder;
use session::Session;
use terminal::Terminal;

const SENTENCES: &str = include_str!("../../../../research/test-set/sentences.txt");

const USAGE: &str = "\
Bruk: skald bench record <mappe> --microphone <merkelapp> [valg]

Viser testsettets setninger én om gangen og tar opp hver mens du leser den. Opptaket lagres som
<merkelapp>-<nr>.wav i <mappe>, setningen i <merkelapp>-<nr>.txt, og manifest.toml får en linje som sier
hvilken mikrofon opptakene er gjort med. Ingenting spilles av. Kjøres kommandoen igjen, fortsetter den
etter det siste opptaket, og setningene som ble hoppet over, kommer til slutt.

Hold Enter mens du leser, og slipp når du er ferdig. I en terminal som ikke melder fra når en tast
slippes, varer opptaket fra ett trykk på Enter til det neste. Begynn å lese når «● Tar opp» står på
skjermen. n tar opp forrige setning på nytt, h hopper over setningen, og q avslutter.

Valg:
  --microphone MERKELAPP  usb (god USB-mikrofon), built-in (Macens innebygde mikrofon) eller
                          distance (på avstand)
  --device NAVN           Mikrofonen å ta opp fra, med navnet systemet gir den, eller en del av det.
                          Ellers systemets standardmikrofon.
  --sentences FIL         Setningene, én per linje. Ellers testsettets egne.
  --json                  Skriv sammendraget, eller feilen, som JSON.
";

/// The microphone a take was recorded with, as the manifest and the bench's tables name it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Label {
    Usb,
    BuiltIn,
    Distance,
}

impl Label {
    const ALL: [Self; 3] = [Self::Usb, Self::BuiltIn, Self::Distance];

    fn as_str(self) -> &'static str {
        match self {
            Self::Usb => "usb",
            Self::BuiltIn => "built-in",
            Self::Distance => "distance",
        }
    }

    fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|label| label.as_str() == text)
    }
}

impl fmt::Display for Label {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Runs `skald bench record` with the arguments after `record`. The sentences and keys go to standard
/// error, and the summary at the end to standard output. Exits with 0 when the user stops or every
/// sentence has been offered, 1 when the microphone, the folder or the terminal fails, and 2 for
/// arguments it does not take.
pub(super) fn run(args: impl IntoIterator<Item = OsString>) -> ExitCode {
    let args: Vec<OsString> = args.into_iter().collect();
    let json = args.iter().any(|arg| arg == "--json");
    let result = match Args::parse(args) {
        Ok(Some(args)) => record(&args).map(|()| true),
        Ok(None) => {
            print!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        Err(message) => Err(Failure::new(Code::Usage)(message)),
    };
    crate::exit(result, json, "skald bench record")
}

#[derive(Debug, PartialEq)]
struct Args {
    folder: PathBuf,
    microphone: Label,
    device: Option<String>,
    sentences: Option<PathBuf>,
    json: bool,
}

impl Args {
    /// The arguments, or `None` when help was asked for.
    fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Option<Self>, String> {
        let (mut folder, mut microphone, mut device, mut sentences) = (None, None, None, None);
        let mut json = false;
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            let mut value = |name: &str| {
                args.next()
                    .ok_or_else(|| format!("{name} trenger en verdi."))
            };
            let text = |value: OsString| {
                value
                    .into_string()
                    .map_err(|_| "Argumentene må være gyldig UTF-8.".to_owned())
            };
            match arg.to_str() {
                Some("-h" | "--help") => return Ok(None),
                Some("--microphone") => {
                    let label = text(value("--microphone")?)?;
                    microphone = Some(Label::parse(&label).ok_or_else(|| {
                        let labels: Vec<&str> = Label::ALL.iter().map(|l| l.as_str()).collect();
                        format!(
                            "--microphone: «{label}» er ikke en av {}.",
                            labels.join(", ")
                        )
                    })?);
                }
                Some("--device") => device = Some(text(value("--device")?)?),
                Some("--sentences") => sentences = Some(PathBuf::from(value("--sentences")?)),
                Some("--json") => json = true,
                Some(flag) if flag.starts_with('-') => {
                    return Err(format!("Ukjent valg «{flag}»."));
                }
                _ if folder.is_none() => folder = Some(PathBuf::from(arg)),
                _ => return Err("Én mappe om gangen.".to_owned()),
            }
        }
        Ok(Some(Self {
            folder: folder.ok_or("Hvilken mappe skal opptakene i?")?,
            microphone: microphone
                .ok_or("Hvilken mikrofon? Oppgi --microphone usb, built-in eller distance.")?,
            device,
            sentences,
            json,
        }))
    }
}

/// The sentences of a file with one on each line, blank lines left out.
fn sentences(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect()
}

fn record(args: &Args) -> Result<(), Failure> {
    let invalid = Failure::new(Code::Invalid);
    let failed = Failure::new(Code::Failed);
    if !io::stdin().is_terminal() {
        return Err(failed(
            "skald bench record leser tastene og må kjøres i en terminal.".into(),
        ));
    }
    let text = match &args.sentences {
        Some(path) => std::fs::read_to_string(path)
            .map_err(|e| invalid(format!("{}: {e}", path.display())))?,
        None => SENTENCES.to_owned(),
    };
    let sentences = sentences(&text);
    if sentences.is_empty() {
        return Err(invalid("Det er ingen setninger å ta opp.".into()));
    }
    std::fs::create_dir_all(&args.folder)
        .map_err(|e| failed(format!("{}: {e}", args.folder.display())))?;
    let folder = Folder::new(args.folder.clone(), args.microphone, sentences.len());
    if !folder.recorded(sentences.len()).is_empty() {
        folder.ensure_manifest().map_err(&invalid)?;
    }

    let settings = "Systeminnstillinger › Personvern og sikkerhet › Mikrofon";
    match skald_audio::microphone_access() {
        MicrophoneAccess::Granted => {}
        MicrophoneAccess::NotAsked => eprintln!(
            "macOS spør om tilgang til mikrofonen ved det første opptaket, og det opptaket blir stille. \
             Ta det opp på nytt med n."
        ),
        MicrophoneAccess::Denied => {
            return Err(failed(format!(
                "Terminalen har ikke tilgang til mikrofonen. Slå den på under {settings}, og prøv igjen."
            )));
        }
        MicrophoneAccess::Restricted => {
            return Err(failed(
                "En regel på denne Macen sperrer mikrofonen, og den kan ikke slås på i \
                 Systeminnstillinger."
                    .into(),
            ));
        }
    }
    let connected = skald_audio::connected_microphones().map_err(|e| {
        failed(format!(
            "Får ikke lest mikrofonene: {}.",
            microphone::reason(&e)
        ))
    })?;
    let chosen = microphone::find(&connected, args.device.as_deref()).map_err(&failed)?;
    eprintln!(
        "Tar opp «{}» fra {} til {}.",
        args.microphone,
        chosen.name,
        args.folder.display()
    );

    let terminal = Terminal::open().map_err(&failed)?;
    let take_end = terminal.take_end();
    // The session owns the terminal, so raw mode has ended by the time the summary is written.
    Session::new(
        &folder,
        &sentences,
        terminal,
        microphone::System(chosen),
        take_end,
        io::stderr(),
    )
    .run()
    .map_err(&failed)?;

    summarise(&folder, sentences.len(), args.json);
    Ok(())
}

fn summarise(folder: &Folder, count: usize, json: bool) {
    if json {
        let recorded = folder.recorded(count);
        let missing: Vec<usize> = (1..=count).filter(|n| !recorded.contains(n)).collect();
        let summary = serde_json::json!({
            "folder": folder.path(),
            "microphone": folder.label().as_str(),
            "sentences": count,
            "recorded": recorded.len(),
            "missing": missing,
        });
        println!("{summary}");
    } else {
        print!("{}", summary(folder, count));
    }
}

/// How far the folder has come with the label, the sentences skipped on the way, and how to measure it.
fn summary(folder: &Folder, count: usize) -> String {
    let recorded = folder.recorded(count);
    let label = folder.label();
    let mut text = if recorded.len() == count {
        format!("Alle {count} setningene er tatt opp med «{label}».")
    } else {
        let last = recorded.last().copied().unwrap_or(0);
        let skipped: Vec<String> = (1..last)
            .filter(|n| !recorded.contains(n))
            .map(|n| n.to_string())
            .collect();
        let skipped = if skipped.is_empty() {
            String::new()
        } else {
            format!(" Hoppet over: {}.", skipped.join(", "))
        };
        format!(
            "{} av {count} setninger er tatt opp med «{label}».{skipped} Kjør kommandoen igjen for å \
             fortsette.",
            recorded.len()
        )
    };
    text.push('\n');
    if !recorded.is_empty() {
        text.push_str(&format!(
            "Mål testsettet med «skald bench {}».\n",
            folder.path().display()
        ));
    }
    text
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::*;

    /// A folder of its own under the system's temporary folder, removed when dropped.
    pub(super) struct Scratch(PathBuf);

    impl Scratch {
        pub(super) fn new(name: &str) -> Self {
            let path = std::env::temp_dir()
                .join(format!("skald-bench-record-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        pub(super) fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn parse(args: &[&str]) -> Result<Option<Args>, String> {
        Args::parse(args.iter().map(OsString::from))
    }

    #[test]
    fn options_are_read() {
        let args = parse(&[
            "--microphone",
            "built-in",
            "set",
            "--device",
            "MacBook",
            "--sentences",
            "s.txt",
            "--json",
        ])
        .unwrap()
        .unwrap();
        assert_eq!(
            args,
            Args {
                folder: PathBuf::from("set"),
                microphone: Label::BuiltIn,
                device: Some("MacBook".into()),
                sentences: Some(PathBuf::from("s.txt")),
                json: true,
            }
        );
        let plain = parse(&["set", "--microphone", "distance"])
            .unwrap()
            .unwrap();
        assert_eq!(plain.microphone, Label::Distance);
        assert!(plain.device.is_none() && plain.sentences.is_none() && !plain.json);
    }

    #[test]
    fn mistakes_are_explained() {
        assert_eq!(
            parse(&["--microphone", "usb"]).unwrap_err(),
            "Hvilken mappe skal opptakene i?"
        );
        assert_eq!(
            parse(&["set"]).unwrap_err(),
            "Hvilken mikrofon? Oppgi --microphone usb, built-in eller distance."
        );
        assert_eq!(
            parse(&["set", "--microphone", "mac"]).unwrap_err(),
            "--microphone: «mac» er ikke en av usb, built-in, distance."
        );
        assert_eq!(
            parse(&["set", "--device"]).unwrap_err(),
            "--device trenger en verdi."
        );
        assert_eq!(parse(&["a", "b"]).unwrap_err(), "Én mappe om gangen.");
        assert_eq!(parse(&["a", "-x"]).unwrap_err(), "Ukjent valg «-x».");
        assert!(parse(&["--help"]).unwrap().is_none());
    }

    #[test]
    fn the_built_in_sentences_are_sixty_distinct_sentences() {
        let sentences = sentences(SENTENCES);
        assert_eq!(sentences.len(), 60);
        assert_eq!(SENTENCES.lines().count(), 60, "no blank lines");
        let distinct: std::collections::BTreeSet<&String> = sentences.iter().collect();
        assert_eq!(distinct.len(), sentences.len());
        for sentence in &sentences {
            assert!(sentence.ends_with(['.', '!', '?']), "{sentence}");
            assert!(sentence.chars().count() <= 100, "{sentence}");
        }
    }

    #[test]
    fn the_summary_names_the_skipped_sentences() {
        let scratch = Scratch::new("summary");
        let folder = Folder::new(scratch.path().to_owned(), Label::Usb, 5);
        assert_eq!(
            summary(&folder, 5),
            "0 av 5 setninger er tatt opp med «usb». Kjør kommandoen igjen for å fortsette.\n"
        );
        for number in [1, 4] {
            folder.write(number, "Hei.", &[0.1; 16_000]).unwrap();
        }
        let measure = format!(
            "Mål testsettet med «skald bench {}».\n",
            scratch.path().display()
        );
        assert_eq!(
            summary(&folder, 5),
            format!(
                "2 av 5 setninger er tatt opp med «usb». Hoppet over: 2, 3. Kjør kommandoen igjen for \
                 å fortsette.\n{measure}"
            )
        );
        for number in [2, 3, 5] {
            folder.write(number, "Hei.", &[0.1; 16_000]).unwrap();
        }
        assert_eq!(
            summary(&folder, 5),
            format!("Alle 5 setningene er tatt opp med «usb».\n{measure}")
        );
    }

    #[test]
    fn blank_lines_hold_no_sentence() {
        assert_eq!(sentences("  Én.\n\n To. \n"), ["Én.", "To."]);
    }
}
