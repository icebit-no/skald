//! `skald bench <folder>`: runs a test set through configurations of model, decoding, silence cutting and
//! filters, and counts word errors and spacing errors (compounds split, words joined) per configuration
//! and per microphone, so docs/design.md, Audio in, keeps only what helps (M3).
//!
//! The test set's layout is in the `set` module, the configurations in `configuration`, how words are
//! compared in `normalise` and `score`. A run writes into its own folder: `takes.jsonl`, one JSON line
//! per take and configuration, and `summary.md`, the tables. `skald bench record` (the `record` module)
//! records a test set.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;

use skald_core::config::Language;
use skald_core::paths::{Env, Os, Paths};
use skald_core::silence::Silence;
use skald_core::vocabulary::Vocabulary;

mod audio;
mod configuration;
mod normalise;
mod record;
mod report;
mod runner;
mod score;
mod set;

use configuration::{Configuration, Model};
use runner::{Outcome, Setup};

const USAGE: &str = "\
Bruk: skald bench <mappe> [valg]

Kjører hvert opptak i <mappe> gjennom hver konfigurasjon og teller ordfeil, særskrivinger og
sammenskrivinger. Mappen har WAV-filer, hver med referanseteksten i en .txt-fil med samme navn, og kan ha
en manifest.toml som sier hvilken mikrofon hvert opptak er gjort med.

Valg:
  --config ORD     En konfigurasjon å kjøre, som ord skilt med komma. Gjenta for flere. Uten den kjøres
                   Skalds egen konfigurasjon, og så hver innstilling endret for seg. Ordene:
                     nb-whisper-large | bokfink-bf16 | bokfink-q8_0   modellen
                     greedy | beam5                                   dekodingen
                     trim | no-trim       kutt stillheten rundt talen, eller behold den
                     highpass | no-highpass                           høypassfilter på 80 Hz
                     rnnoise | no-rnnoise                             RNNoise
                     normalise | no-normalise                         nivånormalisering
                   Det som ikke nevnes, er som i Skald: nb-whisper-large,greedy,trim.
  --model ID=FIL   Filen til en modell. NB-Whisper leses ellers fra Skalds modellmappe.
  --vad FIL        Silero VAD. Leses ellers fra Skalds modellmappe.
  --prompt FIL     En tekst som sendes til modellen som ordskattens setning. Ellers ingen.
  --nynorsk        Transkriber til nynorsk i stedet for bokmål.
  --out MAPPE      Hvor resultatene skrives. Ellers <mappe>/bench-results/<UTC-tid>.
  --json           Skriv sammendraget, eller feilen, som JSON i stedet for Markdown.

Et testsett tas opp med «skald bench record <mappe> --microphone <merkelapp>»; se
«skald bench record --help». Heter mappen record, skriv ./record.
";

/// What kind of failure ended a run, as the command line's other commands name them (D-28).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum Code {
    /// Arguments the command does not take.
    Usage,
    /// A test set that is not as the `set` module describes.
    Invalid,
    Failed,
}

#[derive(Debug, Serialize)]
struct Failure {
    code: Code,
    message: String,
}

impl Failure {
    fn new(code: Code) -> impl Fn(String) -> Self {
        move |message| Self { code, message }
    }
}

/// Runs `skald bench` with the arguments after `bench`. Progress goes to standard error and the summary
/// to standard output. Exits with 0 when every configuration ran or is not available, 1 when one failed or
/// the run could not start, and 2 for arguments it does not take; with `--json`, a failure is
/// `{"error": {"code", "message"}}` on standard output.
pub fn run(args: impl IntoIterator<Item = OsString>) -> ExitCode {
    let args: Vec<OsString> = args.into_iter().collect();
    if args.first().is_some_and(|arg| arg == "record") {
        return record::run(args.into_iter().skip(1));
    }
    let json = args.iter().any(|arg| arg == "--json");
    let result = match Args::parse(args) {
        Ok(Some(args)) => bench(args),
        Ok(None) => {
            print!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        Err(message) => Err(Failure::new(Code::Usage)(message)),
    };
    exit(result, json, "skald bench")
}

/// The exit code for how a command ended, with its failure written out: on standard error, followed by a
/// pointer to `<command> --help` for arguments it does not take, or as JSON on standard output.
fn exit(result: Result<bool, Failure>, json: bool, command: &str) -> ExitCode {
    match result {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(failure) => {
            if json {
                println!("{}", serde_json::json!({ "error": failure }));
            } else {
                eprintln!("{}", failure.message);
                if failure.code == Code::Usage {
                    eprintln!("Se «{command} --help».");
                }
            }
            ExitCode::from(if failure.code == Code::Usage { 2 } else { 1 })
        }
    }
}

#[derive(Debug, PartialEq)]
struct Args {
    folder: PathBuf,
    configurations: Vec<Configuration>,
    models: BTreeMap<Model, PathBuf>,
    vad: Option<PathBuf>,
    prompt: Option<PathBuf>,
    nynorsk: bool,
    out: Option<PathBuf>,
    json: bool,
}

impl Args {
    /// The arguments, or `None` when help was asked for.
    fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Option<Self>, String> {
        let mut folder = None;
        let mut configurations: Vec<Configuration> = Vec::new();
        let mut models = BTreeMap::new();
        let (mut vad, mut prompt, mut out) = (None, None, None);
        let (mut nynorsk, mut json) = (false, false);
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            let mut value = |name: &str| {
                args.next()
                    .ok_or_else(|| format!("{name} trenger en verdi."))
            };
            match arg.to_str() {
                Some("-h" | "--help") => return Ok(None),
                Some("--config") => {
                    let spec = value("--config")?;
                    let spec = spec.to_str().ok_or("Argumentene må være gyldig UTF-8.")?;
                    let configuration = Configuration::parse(spec)?;
                    if !configurations.contains(&configuration) {
                        configurations.push(configuration);
                    }
                }
                Some("--model") => {
                    let pair = value("--model")?;
                    let pair = pair.to_str().ok_or("Argumentene må være gyldig UTF-8.")?;
                    let (id, file) = pair.split_once('=').ok_or("--model tar ID=FIL.")?;
                    let model = Model::from_id(id).ok_or_else(|| {
                        let ids: Vec<&str> = Model::ALL.iter().map(|m| m.id()).collect();
                        format!("--model: «{id}» er ikke en av {}.", ids.join(", "))
                    })?;
                    models.insert(model, PathBuf::from(file));
                }
                Some("--vad") => vad = Some(PathBuf::from(value("--vad")?)),
                Some("--prompt") => prompt = Some(PathBuf::from(value("--prompt")?)),
                Some("--out") => out = Some(PathBuf::from(value("--out")?)),
                Some("--nynorsk") => nynorsk = true,
                Some("--json") => json = true,
                Some(flag) if flag.starts_with('-') => {
                    return Err(format!("Ukjent valg «{flag}»."));
                }
                _ if folder.is_none() => folder = Some(PathBuf::from(arg)),
                _ => return Err("Ett testsett om gangen.".to_owned()),
            }
        }
        if configurations.is_empty() {
            configurations = Configuration::comparison();
        }
        Ok(Some(Self {
            folder: folder.ok_or("Hvilken mappe er testsettet i?")?,
            configurations,
            models,
            vad,
            prompt,
            nynorsk,
            out,
            json,
        }))
    }
}

/// Runs the bench; false when a configuration failed.
fn bench(args: Args) -> Result<bool, Failure> {
    let failed = Failure::new(Code::Failed);
    let clips = set::load(&args.folder).map_err(Failure::new(Code::Invalid))?;
    let with_microphone = clips.iter().filter(|c| c.microphone.is_some()).count();
    eprintln!(
        "{} opptak ({with_microphone} med mikrofon), {} konfigurasjoner",
        clips.len(),
        args.configurations.len()
    );

    let models_dir = Paths::for_os(Os::CURRENT, &Env::from_process()).map(|p| p.models_dir());
    let mut models = args.models.clone();
    for configuration in &args.configurations {
        let model = configuration.model;
        if models.contains_key(&model) || configuration.unavailable().is_some() {
            continue;
        }
        let default = models_dir
            .as_ref()
            .and_then(|dir| skald_models::find(model.id())?.path(dir));
        if let Some(path) = default {
            models.insert(model, path);
        }
    }
    let vad = args
        .vad
        .clone()
        .or_else(|| models_dir.as_ref().map(|dir| skald_models::vad_path(dir)))
        .ok_or_else(|| {
            failed(
                "Finner ingen hjemmemappe å lete etter Silero VAD i. Oppgi den med --vad FIL."
                    .into(),
            )
        })?;

    let mut vocabulary = Vocabulary::default();
    if let Some(path) = &args.prompt {
        vocabulary.sentence = std::fs::read_to_string(path)
            .map_err(|e| failed(format!("{}: {e}", path.display())))?;
    }
    let language = if args.nynorsk {
        Language::Nynorsk
    } else {
        Language::Bokmal
    };
    let setup = Setup {
        models,
        vad,
        language,
        prompt: vocabulary.prompt(),
        silence: Silence::default(),
    };

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO);
    let started = report::utc_stamp(now.as_secs());
    let out = args
        .out
        .clone()
        .unwrap_or_else(|| args.folder.join("bench-results").join(&started));
    std::fs::create_dir_all(&out).map_err(|e| failed(format!("{}: {e}", out.display())))?;
    eprintln!("Resultatene skrives til {}", out.display());

    skald_engine::route_logs_to_log_crate();
    let mut writer =
        report::Writer::create(&out, &clips, args.configurations.len()).map_err(&failed)?;
    let outcomes =
        runner::run(&clips, &args.configurations, &setup, &mut writer).map_err(&failed)?;
    writer.finish().map_err(&failed)?;

    let audio_seconds: f64 = outcomes
        .iter()
        .find_map(|outcome| match outcome {
            Outcome::Ran { takes, .. } => Some(takes.iter().map(|t| t.audio.as_secs_f64()).sum()),
            _ => None,
        })
        .unwrap_or(0.0);
    let used: Vec<Model> = args
        .configurations
        .iter()
        .filter(|c| c.unavailable().is_none())
        .map(|c| c.model)
        .collect();
    let summary = report::Summary {
        about: report::About {
            set: args.folder.clone(),
            takes: clips.len(),
            audio_minutes: (audio_seconds / 6.0).round() / 10.0,
            models: setup
                .models
                .iter()
                .filter(|(model, _)| used.contains(model))
                .map(|(model, path)| (model.id().to_owned(), path.clone()))
                .collect(),
            language: match language {
                Language::Bokmal => "bokmål",
                Language::Nynorsk => "nynorsk",
            },
            prompt: args.prompt.clone(),
            started,
            results: out.clone(),
        },
        configurations: report::rows(&clips, &args.configurations, &outcomes),
    };
    let markdown = summary.markdown();
    let summary_path = out.join(report::SUMMARY_FILE);
    std::fs::write(&summary_path, &markdown)
        .map_err(|e| failed(format!("{}: {e}", summary_path.display())))?;
    if args.json {
        let json = serde_json::to_string_pretty(&summary).map_err(|e| failed(e.to_string()))?;
        println!("{json}");
    } else {
        print!("{markdown}");
    }
    Ok(!outcomes.iter().any(|o| matches!(o, Outcome::Failed(_))))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Option<Args>, String> {
        Args::parse(args.iter().map(OsString::from))
    }

    #[test]
    fn a_folder_alone_runs_the_comparison() {
        let args = parse(&["clips"]).unwrap().unwrap();
        assert_eq!(args.folder, PathBuf::from("clips"));
        assert_eq!(args.configurations, Configuration::comparison());
        assert!(args.models.is_empty() && !args.json && !args.nynorsk);
    }

    #[test]
    fn options_are_read() {
        let args = parse(&[
            "--config",
            "highpass",
            "clips",
            "--config",
            "no-trim,rnnoise",
            "--config",
            "highpass",
            "--model",
            "nb-whisper-large=/models/ggml-model.bin",
            "--vad",
            "/models/vad.gguf",
            "--out",
            "/tmp/out",
            "--json",
            "--nynorsk",
        ])
        .unwrap()
        .unwrap();
        let names: Vec<String> = args
            .configurations
            .iter()
            .map(ToString::to_string)
            .collect();
        assert_eq!(
            names,
            [
                "nb-whisper-large,greedy,trim,highpass",
                "nb-whisper-large,greedy,no-trim,rnnoise"
            ]
        );
        assert_eq!(
            args.models[&Model::NbWhisperLarge],
            PathBuf::from("/models/ggml-model.bin")
        );
        assert_eq!(args.vad, Some(PathBuf::from("/models/vad.gguf")));
        assert_eq!(args.out, Some(PathBuf::from("/tmp/out")));
        assert!(args.json && args.nynorsk);
    }

    #[test]
    fn mistakes_are_explained() {
        assert_eq!(parse(&[]).unwrap_err(), "Hvilken mappe er testsettet i?");
        assert_eq!(parse(&["a", "b"]).unwrap_err(), "Ett testsett om gangen.");
        assert_eq!(
            parse(&["a", "--frobnicate"]).unwrap_err(),
            "Ukjent valg «--frobnicate»."
        );
        assert_eq!(
            parse(&["a", "--config"]).unwrap_err(),
            "--config trenger en verdi."
        );
        assert!(
            parse(&["a", "--model", "whisper=/x"])
                .unwrap_err()
                .contains("«whisper»")
        );
        assert!(
            parse(&["a", "--config", "hp"])
                .unwrap_err()
                .contains("«hp»")
        );
        assert!(parse(&["--help"]).unwrap().is_none());
    }
}
