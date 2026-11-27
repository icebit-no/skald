//! The command line: arguments to a [`Request`], sent to the running Skald or carried out on the files,
//! and the answer written for a person in bokmål or, with `--json`, as one JSON object.

use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use skald_core::paths::Paths;

use crate::commands::{self, SettingKey};
use crate::offline::Offline;
use crate::protocol::{
    Error, ErrorCode, HeardNothing, MicrophoneList, Output, Request, Text, Tokens, VocabularyList,
};
use crate::skald::Machine;
use crate::skill::{self, Status};

/// Every command with its arguments and what it does, for the help and for a command given the wrong
/// arguments.
pub(crate) const COMMANDS: [(&str, &str); 24] = [
    ("last", "Den siste teksten"),
    ("history", "De fem siste tekstene"),
    ("fix list", "Rettingene, og hvor ofte hver er brukt"),
    (
        "fix add <fra> <til>",
        "Skriv <til> der modellen skriver <fra>",
    ),
    ("fix remove <fra>", "Fjern rettingen for <fra>"),
    ("vocab list", "Ordskatten, med antall tokens"),
    (
        "vocab add <ord>",
        "Legg et ord sist i ordskatten, der det veier mest",
    ),
    ("vocab remove <ord>", "Ta et ord ut av ordskatten"),
    (
        "vocab sentence <setning>",
        "Sett setningen ordskatten begynner med",
    ),
    ("config get [<nøkkel>]", "Innstillingene, eller én av dem"),
    ("config set <nøkkel> <verdi>", "Endre en innstilling"),
    ("mic list", "Mikrofonene og rekkefølgen deres"),
    ("mic use <navn>", "Bruk denne mikrofonen foran rekkefølgen"),
    (
        "mic use --first",
        "Bruk den første i rekkefølgen som er koblet til",
    ),
    (
        "mic order <navn> ...",
        "Sett rekkefølgen mikrofonene brukes i",
    ),
    ("model list", "Talemodellene, og hvilke som er lastet ned"),
    ("model use <id>", "Bytt talemodell"),
    (
        "transcribe <fil.wav>",
        "Skriv ut teksten i en lydfil, med rettingene brukt",
    ),
    (
        "start",
        "Trykk ned tasten for å snakke, fra en hurtigtast på Linux",
    ),
    ("stop", "Slipp tasten for å snakke"),
    (
        "bench <mappe> [valg]",
        "Kjør et testsett gjennom konfigurasjonene og tell feilene",
    ),
    (
        "skill install [--dir <mappe>]",
        "Installer Claude-skillen som lærer en agent disse kommandoene",
    ),
    ("skill uninstall [--dir <mappe>]", "Fjern Claude-skillen"),
    (
        "mcp",
        "De samme kommandoene som MCP-verktøy, over standard inn og ut",
    ),
];

fn help() -> String {
    let width = COMMANDS
        .iter()
        .map(|(usage, _)| usage.chars().count())
        .max()
        .unwrap_or(0);
    let mut text = String::from("Bruk: skald <kommando> [--json]\n\n");
    for (usage, does) in COMMANDS {
        text.push_str(&format!("  {usage:width$}  {does}\n"));
    }
    text.push_str(
        "\nSett ord og navn med mellomrom i anførselstegn: skald fix add \"Post gres SQL\" \
         \"PostgreSQL\"\nMed --json skriver hver kommando ett JSON-objekt.\nNår Skald ikke kjører, \
         skrives endringene rett i innstillingsfilen.\n\nInnstillinger for «skald config»:\n",
    );
    let width = SettingKey::ALL
        .iter()
        .map(|key| key.name().len())
        .max()
        .unwrap_or(0);
    for key in SettingKey::ALL {
        text.push_str(&format!("  {:width$}  {}\n", key.name(), key.values()));
    }
    text
}

/// What to say about words that are no command: how the command they start is used, or that there is no
/// such command.
fn misuse(words: &[&str]) -> Error {
    let usages = |prefix: &str| -> Vec<String> {
        COMMANDS
            .iter()
            .map(|(usage, _)| *usage)
            .filter(|usage| *usage == prefix || usage.starts_with(&format!("{prefix} ")))
            .map(|usage| format!("skald {usage}"))
            .collect()
    };
    let command = words.first().copied().unwrap_or_default();
    let mut lines = words
        .get(1)
        .map(|sub| usages(&format!("{command} {sub}")))
        .unwrap_or_default();
    if lines.is_empty() {
        lines = usages(command);
        if lines.len() > 1
            && let Some(sub) = words.get(1)
        {
            return usage(format!("«skald {command}» har ingen kommando «{sub}»."));
        }
    }
    if lines.is_empty() {
        return usage(format!("Ukjent kommando «{command}»."));
    }
    usage(format!("Bruk: {}", lines.join("\n      ")))
}

/// Carries out the command in `args`, the arguments after the program's name, and writes the answer to
/// standard output and errors to standard error.
pub fn run(
    args: impl IntoIterator<Item = OsString>,
    paths: &Paths,
    machine: &dyn Machine,
) -> std::process::ExitCode {
    let code = run_with(
        args,
        paths,
        machine,
        &mut std::io::stdout().lock(),
        &mut std::io::stderr().lock(),
    );
    std::process::ExitCode::from(code)
}

/// [`run`] with the output and error streams given, returning the exit code: 0 when the command did what
/// it was asked, 1 when it failed, 2 for arguments it does not take.
pub fn run_with(
    args: impl IntoIterator<Item = OsString>,
    paths: &Paths,
    machine: &dyn Machine,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> u8 {
    let args: Vec<OsString> = args.into_iter().collect();
    let json = args.iter().any(|arg| arg == "--json");
    let invocation = match parse(&args) {
        Ok(invocation) => invocation,
        Err(error) => return fail(&error, json, out, err),
    };
    let request = match invocation {
        Invocation::Help => {
            let _ = write!(out, "{}", help());
            return 0;
        }
        Invocation::Skill { install, dir } => {
            return match skill_command(install, dir.as_deref()) {
                Ok(report) => {
                    let text = if json {
                        format!(
                            "{}\n",
                            serde_json::to_value(&report).expect("a report always serialises")
                        )
                    } else {
                        skill_text(&report)
                    };
                    let _ = out.write_all(text.as_bytes());
                    0
                }
                Err(error) => fail(&error, json, out, err),
            };
        }
        Invocation::Mcp => {
            return match crate::mcp::serve(std::io::stdin().lock(), out, paths, machine) {
                Ok(()) => 0,
                Err(error) => {
                    let _ = writeln!(err, "skald mcp: {error}");
                    1
                }
            };
        }
        Invocation::Command(request) => request,
    };
    let request = match absolute(request) {
        Ok(request) => request,
        Err(error) => return fail(&error, json, out, err),
    };

    let (result, running) = send(&request, paths, machine, json, err);
    match result {
        Ok(output) => {
            let text = if json {
                format!("{}\n", to_json(&output, running))
            } else {
                render(&output, &request, running)
            };
            let _ = out.write_all(text.as_bytes());
            0
        }
        Err(error) => fail(&error, json, out, err),
    }
}

/// Sends the request to the running Skald, or carries it out here when Skald is not running. Returns
/// whether Skald was running.
pub(crate) fn send(
    request: &Request,
    paths: &Paths,
    machine: &dyn Machine,
    json: bool,
    err: &mut dyn Write,
) -> (Result<Output, Error>, bool) {
    #[cfg(unix)]
    match crate::socket::Client::connect(&paths.socket()) {
        Ok(Some(mut client)) => return (client.send(request), true),
        Ok(None) => {}
        Err(error) => return (Err(error), false),
    }
    if matches!(request, Request::Transcribe { .. }) && !json {
        let _ = writeln!(
            err,
            "Skald kjører ikke, så talemodellen lastes her. Det tar noen sekunder."
        );
    }
    (
        commands::execute(request, &Offline::new(paths, machine)),
        false,
    )
}

fn fail(error: &Error, json: bool, out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    if json {
        let _ = writeln!(out, "{}", serde_json::json!({ "error": error }));
    } else {
        let _ = writeln!(err, "{}", error.message);
        if error.code == ErrorCode::Usage {
            let _ = writeln!(err, "Se «skald help».");
        }
    }
    if error.code == ErrorCode::Usage { 2 } else { 1 }
}

#[derive(Debug, PartialEq, Eq)]
enum Invocation {
    Help,
    Command(Request),
    /// `skald skill install` when `install`, else `skald skill uninstall`.
    Skill {
        install: bool,
        dir: Option<PathBuf>,
    },
    Mcp,
}

fn parse(args: &[OsString]) -> Result<Invocation, Error> {
    let mut words: Vec<String> = Vec::new();
    let mut help = false;
    let mut first = false;
    let mut dir: Option<PathBuf> = None;
    let mut options_end = false;
    let mut args = args.iter();
    let text = |arg: &OsString| -> Result<String, Error> {
        arg.to_str()
            .map(str::to_owned)
            .ok_or_else(|| usage("Argumentene må være gyldig UTF-8."))
    };
    while let Some(arg) = args.next() {
        if options_end || !arg.to_string_lossy().starts_with('-') || arg == "-" {
            words.push(text(arg)?);
            continue;
        }
        match text(arg)?.as_str() {
            "--" => options_end = true,
            "--json" => {}
            "-h" | "--help" => help = true,
            "--first" => first = true,
            // A folder's name need not be UTF-8.
            "--dir" => match args.next() {
                Some(folder) => dir = Some(PathBuf::from(folder)),
                None => return Err(usage("--dir trenger en mappe.")),
            },
            other => return Err(usage(format!("Ukjent valg «{other}»."))),
        }
    }
    let words: Vec<&str> = words.iter().map(String::as_str).collect();
    if help || matches!(words.first(), None | Some(&"help")) {
        return Ok(Invocation::Help);
    }
    if first && !matches!(words.as_slice(), ["mic", "use", ..]) {
        return Err(usage("--first hører til «skald mic use»."));
    }
    let invocation = match words.as_slice() {
        ["skill", verb @ ("install" | "uninstall")] => Some(Invocation::Skill {
            install: *verb == "install",
            dir: dir.take(),
        }),
        ["mcp"] => Some(Invocation::Mcp),
        _ => None,
    };
    if dir.is_some() && words.first() != Some(&"skill") {
        return Err(usage(
            "--dir hører til «skald skill install» og «skald skill uninstall».",
        ));
    }
    if let Some(invocation) = invocation {
        return Ok(invocation);
    }
    let request = match words.as_slice() {
        ["last"] => Request::Last,
        ["history"] => Request::History,

        ["fix"] | ["fix", "list"] => Request::FixList,
        ["fix", "add", from, to] => Request::FixAdd {
            from: (*from).to_owned(),
            to: (*to).to_owned(),
        },
        ["fix", "remove", from] => Request::FixRemove {
            from: (*from).to_owned(),
        },

        ["vocab"] | ["vocab", "list"] => Request::VocabList,
        ["vocab", "add", word] => Request::VocabAdd {
            word: (*word).to_owned(),
        },
        ["vocab", "remove", word] => Request::VocabRemove {
            word: (*word).to_owned(),
        },
        ["vocab", "add" | "remove", _, _, ..] => {
            return Err(usage(
                "Ett ord om gangen. Sett et navn med mellomrom i anførselstegn: skald vocab add \
                 \"Ola Nordmann\"",
            ));
        }
        ["vocab", "sentence", sentence] => Request::VocabSentence {
            sentence: (*sentence).to_owned(),
        },
        ["vocab", "sentence", _, _, ..] => {
            return Err(usage(
                "Sett setningen i anførselstegn: skald vocab sentence \"Vi merget pull-requesten i \
                 går.\"",
            ));
        }

        ["config"] | ["config", "get"] => Request::ConfigGet { key: None },
        ["config", "get", key] => Request::ConfigGet {
            key: Some((*key).to_owned()),
        },
        ["config", "set", key, value] => Request::ConfigSet {
            key: (*key).to_owned(),
            value: (*value).to_owned(),
        },

        ["mic"] | ["mic", "list"] => Request::MicList,
        ["mic", "use"] if first => Request::MicUse { name: None },
        ["mic", "use", name] if !first => Request::MicUse {
            name: Some((*name).to_owned()),
        },
        ["mic", "order", names @ ..] => Request::MicOrder {
            names: names.iter().map(|name| (*name).to_owned()).collect(),
        },

        ["model"] | ["model", "list"] => Request::ModelList,
        ["model", "use", id] => Request::ModelUse {
            id: (*id).to_owned(),
        },

        ["transcribe", file] => Request::Transcribe {
            path: PathBuf::from(file),
        },
        ["start"] => Request::Start,
        ["stop"] => Request::Stop,
        _ => return Err(misuse(&words)),
    };
    Ok(Invocation::Command(request))
}

/// Gives a file to transcribe its full path, which the running Skald needs, and finds out here whether
/// it exists.
pub(crate) fn absolute(request: Request) -> Result<Request, Error> {
    match request {
        Request::Transcribe { path } => match std::fs::canonicalize(&path) {
            Ok(path) => Ok(Request::Transcribe { path }),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Err(Error::new(
                ErrorCode::NotFound,
                format!("Finner ikke lydfilen {}.", path.display()),
            )),
            Err(error) => Err(Error::new(
                ErrorCode::Io,
                format!("Kan ikke åpne lydfilen {}: {error}", path.display()),
            )),
        },
        request => Ok(request),
    }
}

fn usage(message: impl Into<String>) -> Error {
    Error::new(ErrorCode::Usage, message)
}

/// The output's fields with `running`, as one JSON object.
pub(crate) fn to_json(output: &Output, running: bool) -> serde_json::Value {
    let mut value = serde_json::to_value(output).expect("an output always serialises");
    let object = value.as_object_mut().expect("an output is an object");
    object.remove("output");
    object.insert("running".to_owned(), serde_json::Value::Bool(running));
    value
}

fn render(output: &Output, request: &Request, running: bool) -> String {
    let mut text = String::new();
    let mut line = |line: &str| {
        text.push_str(line);
        text.push('\n');
    };
    let mut changed = request.changes_settings();
    match output {
        Output::Last(last) => match &last.last {
            Some(entry) => line(&entry.text),
            None => line("Ingen tekster ennå."),
        },
        Output::History(history) => {
            if history.texts.is_empty() {
                line("Ingen tekster ennå.");
            }
            let now = now_ms();
            for (index, entry) in history.texts.iter().enumerate() {
                line(&history_entry(index + 1, entry, now));
            }
        }
        Output::Corrections(list) => {
            if list.corrections.is_empty() {
                line("Ingen rettinger ennå.");
            }
            for rule in &list.corrections {
                let applied = match rule.applied {
                    0 => "ikke brukt ennå".to_owned(),
                    1 => "brukt 1 gang".to_owned(),
                    n => format!("brukt {n} ganger"),
                };
                line(&format!("{} → {} · {applied}", rule.from, rule.to));
            }
        }
        Output::CorrectionAdded(added) => match &added.previous {
            None => line(&format!(
                "Skriver nå «{}» der modellen skriver «{}».",
                added.to, added.from
            )),
            Some(previous) if *previous == added.to => {
                changed = false;
                line(&format!(
                    "Rettingen «{}» → «{}» finnes allerede.",
                    added.from, added.to
                ));
            }
            Some(previous) => line(&format!(
                "Skriver nå «{}» der modellen skriver «{}» (før: «{previous}»).",
                added.to, added.from
            )),
        },
        Output::CorrectionRemoved(removed) => line(&format!(
            "Fjernet rettingen «{}» → «{}».",
            removed.from, removed.to
        )),
        Output::Vocabulary(vocabulary) => text.push_str(&vocabulary_text(vocabulary)),
        Output::WordAdded(change) => {
            if change.changed {
                line(&format!("La til «{}» sist i ordskatten.", change.word));
            } else {
                changed = false;
                line(&format!("«{}» står allerede i ordskatten.", change.word));
            }
            if let Some(tokens) = change.tokens {
                line(&tokens_text(Some(tokens)));
            }
        }
        Output::WordRemoved(change) => {
            line(&format!("Tok «{}» ut av ordskatten.", change.word));
            if let Some(tokens) = change.tokens {
                line(&tokens_text(Some(tokens)));
            }
        }
        Output::Settings(settings) => {
            for key in SettingKey::ALL {
                if let Some(value) = settings.settings.get(key.name()) {
                    line(&format!("{} = {value}", key.name()));
                }
            }
        }
        Output::Setting(setting) => {
            if changed {
                line(&format!("{} er nå {}.", setting.key, setting.value));
            } else {
                line(&setting.value.to_string());
            }
        }
        Output::Microphones(list) => text.push_str(&microphones_text(list)),
        Output::MicrophonePinned(pinned) => match &pinned.pinned {
            Some(name) => line(&format!(
                "Bruker «{name}» foran rekkefølgen så lenge den er koblet til."
            )),
            None => line("Bruker den første i rekkefølgen som er koblet til."),
        },
        Output::MicrophoneOrder(order) => {
            line("Rekkefølgen er nå:");
            for (index, name) in order.order.iter().enumerate() {
                line(&format!("{}. {name}", index + 1));
            }
        }
        Output::Models(list) => {
            if list.models.is_empty() {
                line("Ingen talemodeller.");
            }
            for model in &list.models {
                let mut parts = vec![format!("{} ({})", model.name, model.id)];
                parts.push(
                    if model.downloaded {
                        "lastet ned"
                    } else {
                        "ikke lastet ned"
                    }
                    .to_owned(),
                );
                if model.active {
                    parts.push("i bruk".to_owned());
                }
                line(&parts.join(" · "));
            }
        }
        Output::ModelUsed(model) => line(&format!("Bruker nå {}.", model.name)),
        Output::Transcription(transcription) => {
            match (&transcription.text, transcription.heard_nothing) {
                (Some(corrected), _) => {
                    line(corrected);
                    if let Some(model_text) = transcription
                        .model_text
                        .as_ref()
                        .filter(|model_text| *model_text != corrected)
                    {
                        line(&format!("Før rettinger: {model_text}"));
                    }
                }
                (None, reason) => line(&format!(
                    "Hørte ingenting: {}.",
                    heard_nothing_text(reason.unwrap_or(HeardNothing::NoWords))
                )),
            }
        }
        Output::Pressed => line("Tasten er nede til «skald stop»."),
        Output::Released => line("Tasten er sluppet."),
    }
    if changed && !running {
        text.push_str("Skald kjører ikke. Endringen gjelder når Skald starter.\n");
    }
    text
}

fn skill_command(install: bool, named: Option<&Path>) -> Result<skill::Report, Error> {
    let dir = skill::dir(named)?;
    if install {
        skill::install(&dir)
    } else {
        skill::uninstall(&dir)
    }
}

fn skill_text(report: &skill::Report) -> String {
    let mut text = String::new();
    let mut line = |line: &str| {
        text.push_str(line);
        text.push('\n');
    };
    let dir = report.dir.display();
    for file in &report.files {
        let path = report.dir.join(file.name);
        let path = path.display();
        match file.status {
            Status::Written => line(&format!("Skrev {path}")),
            Status::Unchanged => line(&format!("{path} var oppdatert fra før")),
            Status::Removed => line(&format!("Fjernet {path}")),
            Status::Missing => {}
        }
    }
    match report.dir_removed {
        None => {
            line(&format!("Claude-skillen «{}» ligger i {dir}.", skill::NAME));
            line("En agent som bruker MCP, får de samme kommandoene som verktøy med:");
            line("  claude mcp add --scope user skald -- skald mcp");
        }
        Some(_)
            if report
                .files
                .iter()
                .all(|file| file.status == Status::Missing) =>
        {
            line(&format!(
                "Fant ingen Claude-skill fra Skald i {dir}. Ingenting ble fjernet."
            ));
        }
        Some(false) => line(&format!(
            "{dir} har andre filer også, så mappen står igjen."
        )),
        Some(true) => {}
    }
    text
}

fn history_entry(number: usize, entry: &Text, now_ms: i64) -> String {
    let indent = " ".repeat(number.to_string().len() + 2);
    let mut text = format!(
        "{number}. {}",
        entry.text.replace('\n', &format!("\n{indent}"))
    );
    let mut about = Vec::new();
    if let Some(ago) = ago(entry.unix_ms, now_ms) {
        about.push(ago);
    }
    if !entry.microphone.is_empty() {
        about.push(entry.microphone.clone());
    }
    if !about.is_empty() {
        text.push_str(&format!("\n{indent}{}", about.join(" · ")));
    }
    text
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_millis().try_into().unwrap_or(i64::MAX))
}

/// How long ago a take ended, in words; `None` when the time is unknown.
fn ago(unix_ms: i64, now_ms: i64) -> Option<String> {
    if unix_ms <= 0 {
        return None;
    }
    let minutes = (now_ms - unix_ms).max(0) / 60_000;
    let (hours, days) = (minutes / 60, minutes / (60 * 24));
    Some(match (days, hours, minutes) {
        (0, 0, 0) => "nå nettopp".to_owned(),
        (0, 0, 1) => "for 1 minutt siden".to_owned(),
        (0, 0, n) => format!("for {n} minutter siden"),
        (0, 1, _) => "for 1 time siden".to_owned(),
        (0, n, _) => format!("for {n} timer siden"),
        (1, _, _) => "for 1 dag siden".to_owned(),
        (n, _, _) => format!("for {n} dager siden"),
    })
}

fn vocabulary_text(vocabulary: &VocabularyList) -> String {
    let mut text = String::new();
    if vocabulary.sentence.is_empty() {
        text.push_str("Setning: (ingen)\n");
    } else {
        text.push_str(&format!("Setning: {}\n", vocabulary.sentence));
    }
    if vocabulary.words.is_empty() {
        text.push_str("Ord: (ingen)\n");
    } else {
        text.push_str("Ord, det viktigste sist:\n");
        for word in &vocabulary.words {
            text.push_str(&format!("  {word}\n"));
        }
    }
    text.push_str(&tokens_text(vocabulary.tokens));
    text.push('\n');
    text
}

fn tokens_text(tokens: Option<Tokens>) -> String {
    match tokens {
        None => "Tokens: telles når Skald kjører, med talemodellen.".to_owned(),
        Some(Tokens { used, limit }) if used > limit => format!(
            "Tokens: {used} av {limit}. Modellen beholder bare slutten, så starten faller bort."
        ),
        Some(Tokens { used, limit }) => format!("Tokens: {used} av {limit}"),
    }
}

fn microphones_text(list: &MicrophoneList) -> String {
    if list.microphones.is_empty() {
        return "Ingen mikrofon er koblet til, og rekkefølgen er tom.\n".to_owned();
    }
    let mut text = String::new();
    for microphone in &list.microphones {
        let mut parts = vec![microphone.name.clone()];
        parts.push(
            if microphone.connected {
                "koblet til"
            } else {
                "ikke koblet til"
            }
            .to_owned(),
        );
        if microphone.pinned {
            parts.push("valgt".to_owned());
        }
        if microphone.active {
            parts.push("brukes nå".to_owned());
        }
        let line = match microphone.position {
            Some(position) => format!("{position}. {}", parts.join(" · ")),
            None => {
                parts.push("ikke i rekkefølgen".to_owned());
                format!("   {}", parts.join(" · "))
            }
        };
        text.push_str(&line);
        text.push('\n');
    }
    text
}

fn heard_nothing_text(reason: HeardNothing) -> &'static str {
    match reason {
        HeardNothing::BelowSpeechLevel => "lyden nådde aldri talenivå",
        HeardNothing::TooLittleSpeech => "fant for lite tale",
        HeardNothing::NoWords => "modellen skrev ingen ord",
        HeardNothing::KnownPhrase => "teksten var en kjent frase for stillhet",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_words(words: &[&str]) -> Result<Invocation, Error> {
        let args: Vec<OsString> = words.iter().map(OsString::from).collect();
        parse(&args)
    }

    fn command(words: &[&str]) -> Request {
        match parse_words(words) {
            Ok(Invocation::Command(request)) => request,
            other => panic!("{words:?}: {other:?}"),
        }
    }

    fn usage_error(words: &[&str]) -> String {
        match parse_words(words) {
            Err(error) => {
                assert_eq!(error.code, ErrorCode::Usage, "{words:?}");
                error.message
            }
            other => panic!("{words:?}: {other:?}"),
        }
    }

    #[test]
    fn parses_every_command() {
        assert_eq!(command(&["last"]), Request::Last);
        assert_eq!(command(&["history", "--json"]), Request::History);
        assert_eq!(command(&["fix"]), Request::FixList);
        assert_eq!(
            command(&["fix", "add", "Post gres SQL", "PostgreSQL"]),
            Request::FixAdd {
                from: "Post gres SQL".into(),
                to: "PostgreSQL".into()
            }
        );
        assert_eq!(
            command(&["--json", "fix", "remove", "mercha"]),
            Request::FixRemove {
                from: "mercha".into()
            }
        );
        assert_eq!(command(&["vocab", "list"]), Request::VocabList);
        assert_eq!(
            command(&["vocab", "add", "merget"]),
            Request::VocabAdd {
                word: "merget".into()
            }
        );
        assert_eq!(
            command(&["vocab", "sentence", ""]),
            Request::VocabSentence {
                sentence: String::new()
            }
        );
        assert_eq!(command(&["config"]), Request::ConfigGet { key: None });
        assert_eq!(
            command(&["config", "set", "language", "nynorsk"]),
            Request::ConfigSet {
                key: "language".into(),
                value: "nynorsk".into()
            }
        );
        assert_eq!(command(&["mic"]), Request::MicList);
        assert_eq!(
            command(&["mic", "use", "--first"]),
            Request::MicUse { name: None }
        );
        assert_eq!(
            command(&["mic", "use", "Shure MV7"]),
            Request::MicUse {
                name: Some("Shure MV7".into())
            }
        );
        assert_eq!(
            command(&["mic", "order", "A", "B"]),
            Request::MicOrder {
                names: vec!["A".into(), "B".into()]
            }
        );
        assert_eq!(command(&["model"]), Request::ModelList);
        assert_eq!(
            command(&["model", "use", "nb-whisper-large"]),
            Request::ModelUse {
                id: "nb-whisper-large".into()
            }
        );
        assert_eq!(
            command(&["transcribe", "opptak.wav"]),
            Request::Transcribe {
                path: "opptak.wav".into()
            }
        );
    }

    #[test]
    fn a_word_after_two_dashes_may_start_with_a_dash() {
        assert_eq!(
            command(&["fix", "add", "--", "-ish", "aktig"]),
            Request::FixAdd {
                from: "-ish".into(),
                to: "aktig".into()
            }
        );
    }

    #[test]
    fn help_is_asked_for_in_three_ways() {
        for words in [&["help"][..], &["--help"], &["fix", "-h"], &[]] {
            assert_eq!(parse_words(words).unwrap(), Invocation::Help, "{words:?}");
        }
    }

    #[test]
    fn wrong_arguments_say_how_the_command_is_used() {
        assert_eq!(usage_error(&["nope"]), "Ukjent kommando «nope».");
        assert_eq!(
            usage_error(&["fix", "rename"]),
            "«skald fix» har ingen kommando «rename»."
        );
        assert_eq!(
            usage_error(&["fix", "add", "mercha"]),
            "Bruk: skald fix add <fra> <til>"
        );
        assert!(usage_error(&["vocab", "add", "Ola", "Nordmann"]).starts_with("Ett ord om gangen"));
        assert_eq!(
            usage_error(&["last", "--verbose"]),
            "Ukjent valg «--verbose»."
        );
        assert_eq!(
            usage_error(&["last", "--first"]),
            "--first hører til «skald mic use»."
        );
        assert_eq!(
            usage_error(&["mic", "use", "A", "--first"]),
            "Bruk: skald mic use <navn>\n      skald mic use --first"
        );
        assert_eq!(usage_error(&["last", "extra"]), "Bruk: skald last");
        assert_eq!(
            usage_error(&["mic", "list", "extra"]),
            "Bruk: skald mic list"
        );
        assert_eq!(
            usage_error(&["transcribe"]),
            "Bruk: skald transcribe <fil.wav>"
        );
    }

    #[test]
    fn says_how_long_ago_in_words() {
        let now = 1_800_000_000_000;
        let minute = 60_000;
        assert_eq!(ago(0, now), None);
        assert_eq!(ago(now - 5_000, now).unwrap(), "nå nettopp");
        assert_eq!(ago(now - minute, now).unwrap(), "for 1 minutt siden");
        assert_eq!(
            ago(now - 59 * minute, now).unwrap(),
            "for 59 minutter siden"
        );
        assert_eq!(ago(now - 60 * minute, now).unwrap(), "for 1 time siden");
        assert_eq!(
            ago(now - 23 * 60 * minute, now).unwrap(),
            "for 23 timer siden"
        );
        assert_eq!(ago(now - 24 * 60 * minute, now).unwrap(), "for 1 dag siden");
        assert_eq!(
            ago(now - 72 * 60 * minute, now).unwrap(),
            "for 3 dager siden"
        );
        assert_eq!(ago(now + minute, now).unwrap(), "nå nettopp");
    }

    #[test]
    fn a_history_entry_indents_its_second_line() {
        let entry = Text {
            text: "Første linje.\nAndre linje.".into(),
            unix_ms: 1_800_000_000_000 - 120_000,
            microphone: "Shure MV7".into(),
        };
        assert_eq!(
            history_entry(1, &entry, 1_800_000_000_000),
            "1. Første linje.\n   Andre linje.\n   for 2 minutter siden · Shure MV7"
        );
    }
}
