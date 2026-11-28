//! Every line a user reads in the terminal, in bokmål. The words docs/design.md gives («rister …»,
//! «Hørte ingenting · lyttet på …», «… · koblet til, brukes nå») are used as given; the rest says plainly what
//! happened.

use std::fmt;
use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

use skald_audio::{CaptureError, TAKE_CAP};
use skald_core::silence_gate::{HEARD_NOTHING, MIN_SPEECH, Reason};
use skald_input::{HoldEvent, HoldKey, InsertError, Inserted, Seen};
use skald_models::Model;

use crate::files::{ConfigError, LockError};
use crate::pipeline::{Outcome, Report, Timings};

const SETTINGS: &str = "Systeminnstillinger › Personvern og sikkerhet";

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Line {
    Version,
    ConfigFile {
        path: PathBuf,
        exists: bool,
    },
    StateFile(PathBuf),
    ModelFiles {
        model: PathBuf,
        vad: PathBuf,
    },
    StateUnreadable {
        path: PathBuf,
        reason: String,
    },
    UnknownHoldKey(String),
    MicrophoneNotAsked,
    Microphone(Option<String>),
    CheckingModel,
    LoadingModel,
    ModelLoaded {
        device: String,
        took: Duration,
    },
    /// The model computes on the CPU because no GPU has a Vulkan driver.
    NoVulkan,
    Ready(HoldKey),
    HoldKeyChanged(HoldKey),
    MicrophoneChanged(Option<String>),
    Recording(String),
    RecordingHandsFree(String),
    Carving,
    Cancelled,
    Capped,
    MicrophoneLost(String),
    NoMicrophone,
    CaptureFailed {
        microphone: String,
        error: CaptureError,
    },
    Inserted(String),
    Truncated,
    PreviousUnreadable,
    PreviousMarkedPrivate,
    CopiedMeanwhile,
    PreviousNotRestored,
    NotInserted {
        text: String,
        error: InsertError,
    },
    HeardNothing(String),
    HeardNothingBecause(Reason),
    TranscriptionFailed(String),
    StateNotSaved(String),
    Timings(Timings),
    ConfigReloaded,
    ConfigBroken(ConfigError),
    ModelAtRestart,
    ListenerFailed(String),
    ListenerBack,
    /// The key listener's event tap is in place.
    Listening,
    /// What the key listener saw on the way to a hold.
    Seen(Seen),
    /// What a hold meant for the take, as the listener reported it.
    Hold(HoldEvent),
    NoSocket(String),
    LoginItemNeedsApp,
    LoginItemUnsupported,
    LoginItemNeedsApproval,
    LoginItemOn,
    LoginItemOff,
    LoginItemFailed(String),
    NoMenuBarItem(String),
    SettingNotChanged(String),
    DownloadFailed {
        model: &'static str,
        reason: String,
    },
    Downloaded(&'static str),
    /// The loop the first run's test started stopped, and why.
    LoopStopped(String),
    Stopping,
}

impl Line {
    /// Shown only with `--verbose`.
    pub(crate) fn is_verbose(&self) -> bool {
        matches!(
            self,
            Line::StateFile(_)
                | Line::ModelFiles { .. }
                | Line::ModelLoaded { .. }
                | Line::CopiedMeanwhile
                | Line::HeardNothingBecause(_)
                | Line::Timings(_)
                | Line::Listening
                | Line::Seen(_)
                | Line::Hold(_)
        )
    }
}

impl fmt::Display for Line {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Line::Version => write!(f, "Skald {}", env!("CARGO_PKG_VERSION")),
            Line::ConfigFile { path, exists: true } => {
                write!(f, "Innstillinger: {}", path.display())
            }
            Line::ConfigFile {
                path,
                exists: false,
            } => write!(
                f,
                "Innstillinger: {} (finnes ikke; standardverdiene gjelder)",
                path.display()
            ),
            Line::StateFile(path) => write!(f, "Historikk: {}", path.display()),
            Line::ModelFiles { model, vad } => write!(
                f,
                "Talemodell: {}\nSilero VAD: {}",
                model.display(),
                vad.display()
            ),
            Line::StateUnreadable { path, reason } => write!(
                f,
                "Kunne ikke lese {}: {reason}. Historikken begynner på nytt.",
                path.display()
            ),
            Line::UnknownHoldKey(name) => write!(
                f,
                "Kjenner ikke tasten «{name}» fra innstillingene; bruker {}.",
                key_name(HoldKey::PLATFORM_DEFAULT)
            ),
            Line::MicrophoneNotAsked => f.write_str(
                "macOS spør om tilgang til mikrofonen første gang du holder tasten. Svar ja, og hold \
                 tasten igjen.",
            ),
            Line::Microphone(Some(name)) => write!(f, "Mikrofon: {name}"),
            Line::Microphone(None) | Line::MicrophoneChanged(None) | Line::NoMicrophone => {
                f.write_str("Ingen mikrofon er koblet til.")
            }
            Line::CheckingModel => {
                f.write_str("Sjekker talemodellen mot sjekksummen, én gang for denne filen …")
            }
            Line::LoadingModel => f.write_str("Laster talemodellen …"),
            Line::ModelLoaded { device, took } => {
                write!(f, "Talemodellen lastet på {} ({device}).", duration(*took))
            }
            Line::NoVulkan => f.write_str(
                "Fant ingen skjermkortdriver med Vulkan, så talemodellen regner på prosessoren og bruker \
                 lengre tid.",
            ),
            Line::Ready(key) => write!(f, "Klar. Hold {} for å snakke.", key_name(*key)),
            Line::HoldKeyChanged(key) => write!(f, "Hold {} for å snakke.", key_name(*key)),
            Line::MicrophoneChanged(Some(name)) => write!(f, "{name} · koblet til, brukes nå"),
            Line::Recording(name) => write!(f, "Opptak · {name}"),
            Line::RecordingHandsFree(name) => {
                write!(f, "Håndfritt opptak · {name} · {HANDS_FREE_CHORD} stopper")
            }
            Line::Carving => f.write_str("rister …"),
            Line::Cancelled => f.write_str("Avbrutt."),
            Line::Capped => write!(
                f,
                "Opptaket stopper ved {} sekunder, grensen for ett opptak.",
                TAKE_CAP.as_secs()
            ),
            Line::MicrophoneLost(name) => {
                write!(f, "{name} ble koblet fra; opptaket slutter med det som kom inn.")
            }
            Line::CaptureFailed { microphone, error } => {
                write!(f, "Kunne ikke åpne {microphone}: {}.", capture_reason(error))
            }
            Line::Inserted(text) => write!(f, "Satt inn: {text}"),
            Line::Truncated => f.write_str("Teksten stoppet tidlig og kan mangle slutten."),
            Line::PreviousUnreadable => f.write_str(
                "Teksten ligger igjen på utklippstavlen; det som lå der før, kunne ikke leses.",
            ),
            Line::PreviousMarkedPrivate => f.write_str(
                "Teksten ligger igjen på utklippstavlen; det som lå der før, var merket som hemmelig \
                 og legges ikke tilbake.",
            ),
            Line::CopiedMeanwhile => {
                f.write_str("Noe ble kopiert i mellomtiden, og det får ligge på utklippstavlen.")
            }
            Line::PreviousNotRestored => {
                f.write_str("Det som lå på utklippstavlen før, kunne ikke legges tilbake.")
            }
            Line::NotInserted { text, error } => {
                write!(f, "Ikke satt inn fordi {}: {text}", insert_reason(error))
            }
            Line::HeardNothing(microphone) => {
                write!(f, "{HEARD_NOTHING} · lyttet på {microphone}")
            }
            Line::HeardNothingBecause(reason) => write!(f, "  ({})", silence_reason(*reason)),
            Line::TranscriptionFailed(reason) => write!(f, "Transkriberingen feilet: {reason}"),
            Line::StateNotSaved(reason) => {
                write!(f, "Kunne ikke lagre historikken i {reason}")
            }
            Line::Timings(timings) => write_timings(f, timings),
            Line::ConfigReloaded => f.write_str("Leste innstillingene på nytt."),
            Line::ConfigBroken(error) => {
                write!(f, "{error}\nBeholder innstillingene som gjaldt.")
            }
            Line::ModelAtRestart => {
                f.write_str("Den nye talemodellen tas i bruk neste gang Skald starter.")
            }
            Line::ListenerFailed(reason) => {
                write!(f, "Får ikke lyttet på tasten ({reason}); prøver igjen.")
            }
            Line::ListenerBack => f.write_str("Lytter på tasten igjen."),
            Line::Listening => f.write_str("Tastaturlytteren har laget sitt event tap."),
            Line::Seen(seen) => write_seen(f, seen),
            Line::Hold(event) => f.write_str(match event {
                HoldEvent::Pressed { .. } => "Holdetasten er trykket; opptaket starter om den holdes.",
                HoldEvent::NotArmed => {
                    "Ikke noe opptak: tasten ble sluppet for tidlig, en annen tast kom til, eller \
                     Skald leste tasten som oppe."
                }
                HoldEvent::Armed { .. } => "Opptaket starter.",
                HoldEvent::Released => "Holdetasten er sluppet.",
                HoldEvent::ReleaseMissed => "Maskinvaren sier at holdetasten er oppe; opptaket slutter.",
                HoldEvent::Cancelled => "En annen tast avbrøt opptaket.",
                HoldEvent::HandsFreeStarted { .. } => "Håndfritt opptak starter.",
                HoldEvent::HandsFreeStopped => "Håndfritt opptak stopper.",
            }),
            Line::NoSocket(reason) => write!(
                f,
                "Kommandolinjen når ikke denne Skald ({reason}); skald-kommandoer skriver rett i filene."
            ),
            Line::LoginItemNeedsApp => {
                f.write_str("Start ved innlogging slås på når Skald kjører som Skald.app.")
            }
            Line::LoginItemUnsupported => {
                f.write_str("Start ved innlogging krever macOS 13 eller nyere.")
            }
            Line::LoginItemNeedsApproval => f.write_str(
                "Tillat Skald under Systeminnstillinger › Generelt › Påloggingsobjekter, så starter \
                 Skald ved innlogging.",
            ),
            Line::LoginItemOn => f.write_str("Skald starter ved innlogging."),
            Line::LoginItemOff => f.write_str("Skald starter ikke ved innlogging."),
            Line::LoginItemFailed(reason) => {
                write!(f, "Kunne ikke endre start ved innlogging: {reason}")
            }
            Line::NoMenuBarItem(reason) => write!(f, "Skald fikk ikke plass i {TRAY_PLACE}: {reason}."),
            Line::SettingNotChanged(reason) => {
                write!(f, "Innstillingen ble ikke endret: {reason}")
            }
            Line::DownloadFailed { model, reason } => {
                write!(f, "Nedlastingen av {model} stoppet: {reason}")
            }
            Line::Downloaded(model) => write!(f, "{model} er lastet ned og sjekket."),
            Line::LoopStopped(reason) => write!(f, "Dikteringen stoppet: {reason}"),
            Line::Stopping => f.write_str("Avslutter …"),
        }
    }
}

/// The lines for one take's report, in the order they are shown.
pub(crate) fn report_lines(report: &Report) -> Vec<Line> {
    let mut lines = Vec::new();
    match &report.outcome {
        Outcome::Inserted {
            text,
            inserted,
            complete,
        } => {
            lines.push(Line::Inserted(text.clone()));
            if !complete {
                lines.push(Line::Truncated);
            }
            match inserted {
                Inserted::PreviousUnreadable => lines.push(Line::PreviousUnreadable),
                Inserted::PreviousMarkedPrivate => lines.push(Line::PreviousMarkedPrivate),
                Inserted::CopiedMeanwhile => lines.push(Line::CopiedMeanwhile),
                Inserted::PreviousNotRestored => lines.push(Line::PreviousNotRestored),
                Inserted::Nothing | Inserted::PreviousRestored | Inserted::TextKept => {}
            }
        }
        Outcome::NotInserted {
            text,
            error,
            complete,
        } => {
            lines.push(Line::NotInserted {
                text: text.clone(),
                error: error.clone(),
            });
            if !complete {
                lines.push(Line::Truncated);
            }
        }
        Outcome::HeardNothing(reason) => {
            lines.push(Line::HeardNothing(report.microphone.clone()));
            lines.push(Line::HeardNothingBecause(*reason));
        }
        Outcome::Failed(reason) => lines.push(Line::TranscriptionFailed(reason.clone())),
    }
    if let Some(reason) = &report.state_error {
        lines.push(Line::StateNotSaved(reason.clone()));
    }
    lines.push(Line::Timings(report.timings.clone()));
    lines
}

/// Why Skald stopped, or would not start; shown on standard error.
#[derive(Debug, PartialEq)]
pub(crate) enum Exit {
    Usage(String),
    NoHome,
    AlreadyRunning,
    Lock(String),
    Config(ConfigError),
    UnknownModel(String),
    /// A model in the registry with no file Skald can fetch yet, by its name.
    ModelNotOffered(&'static str),
    /// `pinned` is the registry's entry when the file is the one it pins, so where to get it can be said.
    ModelMissing {
        path: PathBuf,
        pinned: Option<&'static Model>,
    },
    ModelWrongFile {
        path: PathBuf,
        pinned: &'static Model,
    },
    ModelUnreadable(String),
    /// A file given with `--vad` that is not there.
    VadMissing {
        path: PathBuf,
    },
    VadUnwritable {
        path: PathBuf,
        reason: String,
    },
    ModelLoad(String),
    NoAccessibility,
    MicrophoneDenied,
    MicrophoneRestricted,
    Microphones(CaptureError),
    CannotStart(String),
    KeysStopped,
    /// The dictation's thread ended without saying why.
    Stopped,
    #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
    UnsupportedSystem,
}

impl Exit {
    /// 2 for a command line Skald does not understand, 1 for everything else.
    pub(crate) fn code(&self) -> u8 {
        match self {
            Exit::Usage(_) => 2,
            _ => 1,
        }
    }
}

impl From<LockError> for Exit {
    fn from(error: LockError) -> Self {
        match error {
            LockError::Held => Exit::AlreadyRunning,
            LockError::Failed(reason) => Exit::Lock(reason),
        }
    }
}

impl fmt::Display for Exit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Exit::Usage(message) => write!(f, "{message}\nSe skald --help."),
            Exit::NoHome => f.write_str(
                "Fant ikke hjemmemappen, så Skald vet ikke hvor filene skal ligge. Gi en mappe med --dir.",
            ),
            Exit::AlreadyRunning => f.write_str(
                "Skald kjører allerede, kanskje i en annen terminal. Bare én Skald tar imot diktering om \
                 gangen.",
            ),
            Exit::Lock(reason) => write!(f, "Kunne ikke låse {reason}"),
            Exit::Config(error) => write!(f, "{error}\nRett filen og start Skald igjen."),
            Exit::UnknownModel(id) => write!(
                f,
                "Kjenner ikke talemodellen «{id}» fra innstillingene. Gi en modellfil med --model."
            ),
            Exit::ModelNotOffered(name) => write!(
                f,
                "{name} har ingen fil Skald kan hente ennå. Gi en modellfil med --model."
            ),
            Exit::ModelMissing {
                path,
                pinned: Some(pinned),
            } => {
                writeln!(f, "Fant ikke talemodellen: {}", path.display())?;
                write_fetch(f, pinned, path)
            }
            Exit::ModelMissing { path, pinned: None } => {
                write!(f, "Fant ikke talemodellen: {}", path.display())
            }
            Exit::ModelWrongFile { path, pinned } => {
                writeln!(
                    f,
                    "{} er ikke filen Skald venter for {}: sjekksummen stemmer ikke.",
                    path.display(),
                    pinned.name
                )?;
                write_fetch(f, pinned, path)
            }
            Exit::ModelUnreadable(reason) => write!(f, "Kunne ikke lese talemodellen: {reason}"),
            Exit::VadMissing { path } => write!(
                f,
                "Fant ikke Silero VAD: {}\nUten --vad bruker Skald sin egen.",
                path.display()
            ),
            Exit::VadUnwritable { path, reason } => {
                write!(f, "Kunne ikke legge Silero VAD i {}: {reason}", path.display())
            }
            Exit::ModelLoad(reason) => write!(f, "Kunne ikke laste talemodellen: {reason}"),
            Exit::NoAccessibility => write!(
                f,
                "Skald trenger Tilgjengelighet for tasten og innsettingen.\nSlå på terminalen du kjører \
                 Skald i under {SETTINGS} › Tilgjengelighet (legg den til med + om den mangler), og \
                 start Skald igjen."
            ),
            Exit::MicrophoneDenied => write!(
                f,
                "Skald har ikke tilgang til mikrofonen.\nSlå på terminalen du kjører Skald i under \
                 {SETTINGS} › Mikrofon, og start Skald igjen."
            ),
            Exit::MicrophoneRestricted => f.write_str(
                "En regel på denne Macen sperrer mikrofonen, og den kan ikke slås på i \
                 Systeminnstillinger.",
            ),
            Exit::Microphones(error) => {
                write!(f, "Får ikke lest mikrofonene: {}.", capture_reason(error))
            }
            Exit::CannotStart(reason) => write!(f, "Kunne ikke starte: {reason}"),
            Exit::KeysStopped => f.write_str("Tastelytteren stoppet uventet."),
            Exit::Stopped => f.write_str("Dikteringen stoppet uventet."),
            #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
            Exit::UnsupportedSystem => {
                f.write_str("Skald kjører foreløpig bare på macOS, Windows og Linux.")
            }
        }
    }
}

/// Where to get the pinned file and how to put it at `path`, then the way round it.
fn write_fetch(f: &mut fmt::Formatter<'_>, pinned: &Model, path: &std::path::Path) -> fmt::Result {
    let Some(file) = pinned.file() else {
        return f.write_str("Gi en modellfil med --model.");
    };
    write!(
        f,
        "Last ned {} ({}) og legg den der:\n  {}\nEller gi en annen fil med --model.",
        pinned.name,
        gigabytes(file.size),
        fetch_command(path, &file.url),
    )
}

/// curl with the path quoted for the system's shell. On Windows it is `curl.exe`, which PowerShell's own
/// `curl` would otherwise stand in for, and the path takes double quotes, the only ones cmd.exe reads.
fn fetch_command(path: &std::path::Path, url: &str) -> String {
    if cfg!(windows) {
        format!("curl.exe -L --create-dirs -o \"{}\" {url}", path.display())
    } else {
        format!("curl -L --create-dirs -o '{}' {url}", path.display())
    }
}

/// Decimal gigabytes with one decimal and a decimal comma: «3,1 GB».
fn gigabytes(bytes: u64) -> String {
    format!("{:.1} GB", bytes as f64 / 1e9).replace('.', ",")
}

/// Prints each line as it happens, the verbose ones only when asked for.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Printer {
    pub(crate) verbose: bool,
}

impl Printer {
    pub(crate) fn say(&self, line: Line) {
        if self.verbose || !line.is_verbose() {
            // A terminal that has gone away leaves no one to tell.
            let _ = writeln!(std::io::stdout().lock(), "{line}");
        }
    }
}

/// Where this system shows Skald's item, as its users call the place.
const TRAY_PLACE: &str = if cfg!(target_os = "macos") {
    "menylinjen"
} else if cfg!(windows) {
    "systemstatusfeltet"
} else {
    "panelet"
};

/// The hands-free chord as this system's users press it.
const HANDS_FREE_CHORD: &str = if cfg!(target_os = "macos") {
    "⌃⌘X"
} else if cfg!(windows) {
    "Ctrl+Win+X"
} else {
    "Ctrl+Super+X"
};

/// Why the item in the menu bar or the panel was not made, in the words of the line that says so.
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
pub(crate) fn tray_reason(error: &skald_tray::TrayError) -> String {
    match error {
        skald_tray::TrayError::NotMainThread => "den kan bare lages på hovedtråden".into(),
        skald_tray::TrayError::Unsupported => "dette systemet har ingen plass til den ennå".into(),
        skald_tray::TrayError::Refused => "systemet ville ikke vise den".into(),
        skald_tray::TrayError::NoService(reason) => {
            format!("tjenesten for den startet ikke ({reason})")
        }
    }
}

pub(crate) fn key_name(key: HoldKey) -> String {
    crate::keys::name(key)
}

pub(crate) fn capture_reason(error: &CaptureError) -> String {
    match error {
        CaptureError::NotConnected => "mikrofonen er ikke koblet til".into(),
        CaptureError::PermissionDenied => "Skald har ikke tilgang til mikrofonen".into(),
        CaptureError::Busy => "et annet program bruker den".into(),
        CaptureError::UnsupportedFormat(detail) => format!("lydformatet støttes ikke ({detail})"),
        CaptureError::System(detail) => format!("lydsystemet feilet ({detail})"),
    }
}

fn insert_reason(error: &InsertError) -> &'static str {
    match error {
        InsertError::NoAccessibility => "Skald mangler Tilgjengelighet",
        InsertError::KeysHeld => "en tast ble holdt nede",
        InsertError::ElevatedWindow => "vinduet foran kjører som administrator",
        InsertError::Clipboard => "utklippstavlen tok ikke imot teksten",
        InsertError::Paste if cfg!(target_os = "macos") => "⌘V kunne ikke sendes",
        InsertError::Paste if cfg!(target_os = "linux") => {
            "Ctrl+V kunne ikke sendes (er wtype installert?)"
        }
        InsertError::Paste => "Ctrl+V kunne ikke sendes",
    }
}

fn silence_reason(reason: Reason) -> String {
    match reason {
        Reason::BelowSpeechLevel => "under talenivå".into(),
        Reason::TooLittleSpeech => format!("mindre enn {} ms tale", MIN_SPEECH.as_millis()),
        Reason::NoWords => "ingen ord i teksten".into(),
        Reason::KnownPhrase => "en kjent frase for stillhet".into(),
    }
}

fn write_timings(f: &mut fmt::Formatter<'_>, timings: &Timings) -> fmt::Result {
    write!(
        f,
        "  lyd {} · tale {} · VAD {}",
        duration(timings.audio),
        duration(timings.speech),
        duration(timings.vad)
    )?;
    if let Some(model) = &timings.model {
        write!(
            f,
            " · modell {} (koder {}, dekoder {})",
            duration(model.total),
            duration(model.encode),
            duration(model.decode)
        )?;
    }
    if let Some(tokens) = timings.prompt_tokens {
        write!(f, " · ordskatt {tokens} tokens")?;
        match timings.prompt_dropped {
            0 => {}
            1 => f.write_str(", 1 del kuttet foran")?,
            dropped => write!(f, ", {dropped} deler kuttet foran")?,
        }
    }
    if let Some(ready) = timings.ready {
        write!(f, " · teksten klar {} etter slipp", duration(ready))?;
    }
    Ok(())
}

/// Milliseconds below a second, else seconds with one decimal and a decimal comma: «820 ms», «4,2 s».
fn duration(duration: Duration) -> String {
    if duration < Duration::from_secs(1) {
        format!("{} ms", duration.as_millis())
    } else {
        format!("{:.1} s", duration.as_secs_f64()).replace('.', ",")
    }
}

fn write_seen(f: &mut fmt::Formatter<'_>, seen: &Seen) -> fmt::Result {
    let down = |down: bool| if down { "ned" } else { "opp" };
    match seen {
        Seen::FirstKey { key, down: pressed } => {
            write!(f, "Første tast lytteren fikk: {key} {}.", down(*pressed))
        }
        Seen::HoldKey {
            down: pressed,
            sample,
        } => write!(
            f,
            "Holdetasten {} ifølge hendelsen; Skald leser holdetasten som {}, {}.",
            down(*pressed),
            if sample.hold_key_down { "nede" } else { "oppe" },
            if sample.other_modifiers_down {
                "og en annen modifikator er nede"
            } else {
                "ingen andre modifikatorer"
            }
        ),
        Seen::Taps {
            taps,
            keyboard_allowed,
        } => {
            let allowed = if *keyboard_allowed {
                "tillatt"
            } else {
                "ikke tillatt"
            };
            if taps.is_empty() {
                return write!(
                    f,
                    "Systemet har ingen event tap fra Skald; lytting på tastaturet er {allowed}."
                );
            }
            write!(f, "Systemet har {} event tap fra Skald:", taps.len())?;
            for tap in taps {
                write!(
                    f,
                    " {}, {}, lengste ventetid {};",
                    if tap.enabled { "på" } else { "av" },
                    if tap.listen_only {
                        "lytter bare"
                    } else {
                        "kan holde igjen hendelser"
                    },
                    duration(Duration::from_micros(u64::from(tap.max_latency_us)))
                )?;
            }
            write!(f, " lytting på tastaturet er {allowed}.")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(outcome: Outcome) -> Report {
        Report {
            take: 1,
            microphone: "Shure MV7".into(),
            outcome,
            timings: Timings::default(),
            state_error: None,
            state: skald_core::state::State::default(),
        }
    }

    #[test]
    fn the_words_of_design_md_are_used_as_given() {
        assert_eq!(
            Line::HeardNothing("Shure MV7".into()).to_string(),
            "Hørte ingenting · lyttet på Shure MV7"
        );
        assert_eq!(
            Line::MicrophoneChanged(Some("Yeti".into())).to_string(),
            "Yeti · koblet til, brukes nå"
        );
        assert_eq!(Line::Carving.to_string(), "rister …");
        #[cfg(target_os = "macos")]
        assert_eq!(
            Line::Ready(HoldKey::RightCommand).to_string(),
            "Klar. Hold høyre ⌘ for å snakke."
        );
        assert_eq!(
            Line::Ready(HoldKey::Function(13)).to_string(),
            "Klar. Hold F13 for å snakke."
        );
        let control = if cfg!(target_os = "macos") {
            "Hold høyre Control for å snakke."
        } else {
            "Hold høyre Ctrl for å snakke."
        };
        assert_eq!(
            Line::HoldKeyChanged(HoldKey::RightControl).to_string(),
            control
        );
        assert!(
            Exit::NoAccessibility
                .to_string()
                .contains("for tasten og innsettingen")
        );
    }

    #[test]
    fn the_text_is_satt_inn_never_limt_inn() {
        let shown = Line::Inserted("Vi merget den.".into()).to_string();
        assert_eq!(shown, "Satt inn: Vi merget den.");
        for line in [
            Line::Inserted("x".into()),
            Line::NotInserted {
                text: "x".into(),
                error: InsertError::KeysHeld,
            },
        ] {
            assert!(!line.to_string().contains("lim"), "{line}");
        }
    }

    #[test]
    fn durations_use_a_decimal_comma() {
        assert_eq!(duration(Duration::from_millis(820)), "820 ms");
        assert_eq!(duration(Duration::from_millis(4249)), "4,2 s");
        assert_eq!(duration(Duration::from_secs(120)), "120,0 s");
    }

    #[test]
    fn an_inserted_take_shows_its_text_and_any_note_about_the_clipboard() {
        let lines = report_lines(&report(Outcome::Inserted {
            text: "Hei.".into(),
            inserted: Inserted::PreviousMarkedPrivate,
            complete: false,
        }));
        assert_eq!(
            lines,
            [
                Line::Inserted("Hei.".into()),
                Line::Truncated,
                Line::PreviousMarkedPrivate,
                Line::Timings(Timings::default()),
            ]
        );
        let lines = report_lines(&report(Outcome::Inserted {
            text: "Hei.".into(),
            inserted: Inserted::PreviousRestored,
            complete: true,
        }));
        assert_eq!(lines.len(), 2);
        let lines = report_lines(&report(Outcome::Inserted {
            text: "Hei.".into(),
            inserted: Inserted::PreviousNotRestored,
            complete: true,
        }));
        assert_eq!(lines[1], Line::PreviousNotRestored);
        assert!(!lines[1].is_verbose(), "the user's clipboard is gone");
    }

    #[test]
    fn a_silent_take_names_the_microphone_and_why_in_verbose() {
        let lines = report_lines(&report(Outcome::HeardNothing(Reason::TooLittleSpeech)));
        assert_eq!(
            lines[0].to_string(),
            "Hørte ingenting · lyttet på Shure MV7"
        );
        assert!(lines[1].is_verbose());
        assert_eq!(lines[1].to_string(), "  (mindre enn 250 ms tale)");
    }

    #[test]
    fn a_take_not_inserted_still_shows_the_text() {
        let mut report = report(Outcome::NotInserted {
            text: "Hei.".into(),
            error: InsertError::KeysHeld,
            complete: true,
        });
        report.state_error = Some("/x/state.toml: disk full".into());
        let shown: Vec<String> = report_lines(&report)
            .iter()
            .filter(|line| !line.is_verbose())
            .map(ToString::to_string)
            .collect();
        assert_eq!(
            shown,
            [
                "Ikke satt inn fordi en tast ble holdt nede: Hei.",
                "Kunne ikke lagre historikken i /x/state.toml: disk full",
            ]
        );
    }

    #[test]
    fn timings_read_as_one_line() {
        let timings = Timings {
            audio: Duration::from_millis(4200),
            speech: Duration::from_millis(3100),
            vad: Duration::from_millis(12),
            model: Some(skald_engine::Timings {
                total: Duration::from_millis(820),
                encode: Duration::from_millis(300),
                decode: Duration::from_millis(480),
                ..skald_engine::Timings::default()
            }),
            prompt_tokens: Some(213),
            prompt_dropped: 1,
            ready: Some(Duration::from_millis(900)),
        };
        assert_eq!(
            Line::Timings(timings).to_string(),
            "  lyd 4,2 s · tale 3,1 s · VAD 12 ms · modell 820 ms (koder 300 ms, dekoder 480 ms) · \
             ordskatt 213 tokens, 1 del kuttet foran · teksten klar 900 ms etter slipp"
        );
    }

    #[test]
    fn a_missing_model_says_where_it_goes_and_where_to_get_it() {
        let pinned = skald_models::find("nb-whisper-large").unwrap();
        let path = PathBuf::from(
            "/Users/a/Library/Application Support/Skald/models/nb-whisper-large/ggml-model.bin",
        );
        let fetch = fetch_command(&path, &pinned.file().unwrap().url);
        assert!(fetch.contains(&path.display().to_string()), "{fetch}");
        let shown = Exit::ModelMissing {
            path: path.clone(),
            pinned: Some(pinned),
        }
        .to_string();
        assert!(shown.contains(&fetch), "{shown}");
        assert!(shown.contains("NB-Whisper Large (3,1 GB)"), "{shown}");
        assert!(shown.contains("--model"), "{shown}");

        let wrong = Exit::ModelWrongFile {
            path: path.clone(),
            pinned,
        }
        .to_string();
        assert!(wrong.contains("sjekksummen stemmer ikke"), "{wrong}");
        assert!(wrong.contains(&fetch), "{wrong}");

        let given = Exit::ModelMissing {
            path: PathBuf::from("/m/x.bin"),
            pinned: None,
        }
        .to_string();
        assert_eq!(given, "Fant ikke talemodellen: /m/x.bin");

        let vad = Exit::VadMissing {
            path: PathBuf::from("/m/silero-vad-v6.2-F32.gguf"),
        }
        .to_string();
        assert_eq!(
            vad,
            "Fant ikke Silero VAD: /m/silero-vad-v6.2-F32.gguf\nUten --vad bruker Skald sin egen."
        );
    }

    #[test]
    fn permissions_say_which_and_where() {
        assert!(
            Exit::NoAccessibility
                .to_string()
                .contains("Systeminnstillinger › Personvern og sikkerhet › Tilgjengelighet")
        );
        assert!(
            Exit::MicrophoneDenied
                .to_string()
                .contains("Systeminnstillinger › Personvern og sikkerhet › Mikrofon")
        );
    }

    #[test]
    fn the_listeners_traces_show_in_verbose_and_say_what_the_hardware_said() {
        use skald_input::{KeySample, Tap};

        let held = Line::Seen(Seen::HoldKey {
            down: true,
            sample: KeySample {
                hold_key_down: false,
                other_modifiers_down: false,
            },
        });
        assert!(held.is_verbose());
        assert!(Line::Listening.is_verbose());
        assert!(Line::Hold(HoldEvent::NotArmed).is_verbose());
        assert!(
            Line::Hold(HoldEvent::NotArmed)
                .to_string()
                .contains("Skald leste tasten som oppe")
        );
        assert_eq!(
            held.to_string(),
            "Holdetasten ned ifølge hendelsen; Skald leser holdetasten som oppe, ingen andre \
             modifikatorer."
        );
        assert_eq!(
            Line::Seen(Seen::FirstKey {
                key: "CMD_RIGHT".into(),
                down: true
            })
            .to_string(),
            "Første tast lytteren fikk: CMD_RIGHT ned."
        );
        let taps = Line::Seen(Seen::Taps {
            taps: vec![Tap {
                enabled: false,
                listen_only: false,
                max_latency_us: 12_500,
            }],
            keyboard_allowed: true,
        });
        assert_eq!(
            taps.to_string(),
            "Systemet har 1 event tap fra Skald: av, kan holde igjen hendelser, lengste ventetid \
             12 ms; lytting på tastaturet er tillatt."
        );
        assert_eq!(
            Line::Seen(Seen::Taps {
                taps: Vec::new(),
                keyboard_allowed: false
            })
            .to_string(),
            "Systemet har ingen event tap fra Skald; lytting på tastaturet er ikke tillatt."
        );
    }

    #[test]
    fn only_a_command_line_mistake_exits_with_2() {
        assert_eq!(Exit::Usage("x".into()).code(), 2);
        assert_eq!(Exit::NoAccessibility.code(), 1);
    }
}
