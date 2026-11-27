//! What a run writes: one JSON line per take and configuration as each take finishes, progress on standard
//! error, and a summary of word errors and spacing errors per configuration and per microphone, as
//! Markdown or JSON.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs::File;
use std::io::{BufWriter, Write as _};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Serialize;
use skald_core::silence_gate::Reason;

use crate::configuration::{Configuration, Model};
use crate::runner::{Outcome, Progress, TakeResult};
use crate::score::{Score, Step, Tally};
use crate::set::Clip;

/// The per-take results in a run's folder.
pub const TAKES_FILE: &str = "takes.jsonl";
pub const SUMMARY_FILE: &str = "summary.md";

/// Why the gate stopped a take, as the progress lines say it.
fn reason_text(reason: Reason) -> &'static str {
    match reason {
        Reason::BelowSpeechLevel => "under talenivå",
        Reason::TooLittleSpeech => "for lite tale",
        Reason::NoWords => "ingen ord",
        Reason::KnownPhrase => "kjent frase",
    }
}

fn reason_id(reason: Reason) -> &'static str {
    match reason {
        Reason::BelowSpeechLevel => "below_speech_level",
        Reason::TooLittleSpeech => "too_little_speech",
        Reason::NoWords => "no_words",
        Reason::KnownPhrase => "known_phrase",
    }
}

#[derive(Debug, Serialize, PartialEq, Eq)]
struct Pair {
    reference: String,
    text: String,
}

/// One line of the takes file.
#[derive(Debug, Serialize)]
struct TakeLine<'a> {
    configuration: String,
    take: &'a str,
    microphone: Option<&'a str>,
    reference: &'a str,
    /// What would be inserted.
    text: &'a str,
    model_text: Option<&'a str>,
    heard_nothing: Option<&'static str>,
    complete: bool,
    reference_words: usize,
    word_errors: usize,
    spacing_word_errors: usize,
    splits: Vec<Pair>,
    joins: Vec<Pair>,
    substitutions: Vec<Pair>,
    deletions: Vec<&'a str>,
    insertions: Vec<&'a str>,
    audio_s: f64,
    speech_s: f64,
    vad_ms: f64,
    transcribe_ms: f64,
}

/// The differences of a score, by kind, as words.
struct Differences<'a> {
    splits: Vec<Pair>,
    joins: Vec<Pair>,
    substitutions: Vec<Pair>,
    deletions: Vec<&'a str>,
    insertions: Vec<&'a str>,
}

fn differences(score: &Score) -> Differences<'_> {
    let (reference, text) = (&score.reference, &score.text);
    let mut out = Differences {
        splits: Vec::new(),
        joins: Vec::new(),
        substitutions: Vec::new(),
        deletions: Vec::new(),
        insertions: Vec::new(),
    };
    for step in &score.steps {
        match step {
            Step::Match { .. } => {}
            Step::Substitute {
                reference: r,
                text: t,
            } => out.substitutions.push(Pair {
                reference: reference[*r].clone(),
                text: text[*t].clone(),
            }),
            Step::Delete { reference: r } => out.deletions.push(&reference[*r]),
            Step::Insert { text: t } => out.insertions.push(&text[*t]),
            Step::Split {
                reference: r,
                text: t,
            } => out.splits.push(Pair {
                reference: reference[*r].clone(),
                text: text[t.clone()].join(" "),
            }),
            Step::Join {
                reference: r,
                text: t,
            } => out.joins.push(Pair {
                reference: reference[r.clone()].join(" "),
                text: text[*t].clone(),
            }),
        }
    }
    out
}

fn take_line<'a>(
    configuration: &Configuration,
    clip: &'a Clip,
    result: &'a TakeResult,
) -> TakeLine<'a> {
    let d = differences(&result.score);
    TakeLine {
        configuration: configuration.to_string(),
        take: &clip.name,
        microphone: clip.microphone.as_deref(),
        reference: &clip.reference,
        text: &result.text,
        model_text: result.model_text.as_deref(),
        heard_nothing: result.heard_nothing.map(reason_id),
        complete: result.complete,
        reference_words: result.score.reference.len(),
        word_errors: result.score.word_errors(),
        spacing_word_errors: result.score.spacing_word_errors(),
        splits: d.splits,
        joins: d.joins,
        substitutions: d.substitutions,
        deletions: d.deletions,
        insertions: d.insertions,
        audio_s: result.audio.as_secs_f64(),
        speech_s: result.speech.as_secs_f64(),
        vad_ms: millis(result.vad_time),
        transcribe_ms: millis(result.transcribe_time),
    }
}

fn millis(duration: Duration) -> f64 {
    (duration.as_secs_f64() * 1000.0).round()
}

/// Writes each take's line as it finishes and tells on standard error how the run goes.
pub struct Writer<'a> {
    clips: &'a [Clip],
    configurations: usize,
    takes: BufWriter<File>,
    error: Option<String>,
    path: PathBuf,
}

impl<'a> Writer<'a> {
    pub fn create(folder: &Path, clips: &'a [Clip], configurations: usize) -> Result<Self, String> {
        let path = folder.join(TAKES_FILE);
        let file = File::create(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        Ok(Self {
            clips,
            configurations,
            takes: BufWriter::new(file),
            error: None,
            path,
        })
    }

    /// The first error writing the takes file, if there was one.
    pub fn finish(mut self) -> Result<(), String> {
        if let Err(e) = self.takes.flush() {
            self.error
                .get_or_insert(format!("{}: {e}", self.path.display()));
        }
        self.error.map_or(Ok(()), Err)
    }
}

impl Progress for Writer<'_> {
    fn loading(&mut self, model: Model, path: &Path) {
        eprintln!("Laster {} fra {}", model.id(), path.display());
    }

    fn loaded(&mut self, model: Model, device: &str, took: Duration) {
        eprintln!(
            "Lastet {} på {device} på {} s",
            model.id(),
            decimal(took.as_secs_f64())
        );
    }

    fn configuration(&mut self, index: usize, configuration: &Configuration) {
        eprintln!("[{}/{}] {configuration}", index + 1, self.configurations);
    }

    fn take(&mut self, configuration: &Configuration, clip: &Clip, result: &TakeResult) {
        let line = take_line(configuration, clip, result);
        let written = serde_json::to_writer(&mut self.takes, &line)
            .map_err(|e| e.to_string())
            .and_then(|()| self.takes.write_all(b"\n").map_err(|e| e.to_string()))
            .and_then(|()| self.takes.flush().map_err(|e| e.to_string()));
        if let Err(e) = written {
            self.error
                .get_or_insert(format!("{}: {e}", self.path.display()));
        }
        let note = match result.heard_nothing {
            Some(reason) => format!(", hørte ingenting ({})", reason_text(reason)),
            None => String::new(),
        };
        eprintln!(
            "  {}/{} {}: {} av {} ord feil{note}",
            result.clip + 1,
            self.clips.len(),
            clip.name,
            result.score.word_errors(),
            result.score.reference.len()
        );
    }

    fn finished(&mut self, index: usize, configuration: &Configuration, outcome: &Outcome) {
        let position = format!("[{}/{}] {configuration}", index + 1, self.configurations);
        match outcome {
            Outcome::Ran { takes, .. } => {
                let tally = tally(takes.iter());
                eprintln!(
                    "  {} ordfeil, {} særskrivinger, {} sammenskrivinger",
                    percent(tally.word_error_rate()),
                    tally.splits,
                    tally.joins
                );
            }
            Outcome::Unavailable(reason) => eprintln!("{position}: ikke tilgjengelig: {reason}"),
            Outcome::Failed(reason) => eprintln!("{position}: feilet: {reason}"),
        }
    }
}

fn tally<'a>(takes: impl Iterator<Item = &'a TakeResult>) -> Tally {
    let mut tally = Tally::default();
    for take in takes {
        tally.add(&take.score);
    }
    tally
}

/// A number with one decimal and a decimal comma, as bokmål writes it.
fn decimal(value: f64) -> String {
    format!("{value:.1}").replace('.', ",")
}

fn percent(rate: Option<f64>) -> String {
    rate.map_or_else(|| "–".to_owned(), |rate| format!("{} %", decimal(rate)))
}

/// The microphone a take without one is listed under, when other takes have one.
const NO_MICROPHONE: &str = "(ingen)";

/// What the summary says about one configuration.
#[derive(Debug, Serialize)]
pub struct Row {
    pub configuration: String,
    #[serde(flatten)]
    pub status: Status,
}

#[derive(Debug, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Status {
    Ran {
        device: String,
        #[serde(flatten)]
        counts: Counts,
        transcribe_s: f64,
        /// By microphone, when the manifest names any.
        #[serde(skip_serializing_if = "BTreeMap::is_empty")]
        microphones: BTreeMap<String, Counts>,
    },
    NotAvailable {
        reason: &'static str,
    },
    Failed {
        reason: String,
    },
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq)]
pub struct Counts {
    pub takes: usize,
    pub reference_words: usize,
    pub word_errors: usize,
    pub spacing_word_errors: usize,
    pub word_error_rate: Option<f64>,
    /// The word error rate without the word errors of splits and joins.
    pub other_error_rate: Option<f64>,
    pub splits: usize,
    pub joins: usize,
    /// Takes the silence gate stopped, so nothing was inserted.
    pub heard_nothing: usize,
}

fn counts<'a>(takes: impl Iterator<Item = &'a TakeResult> + Clone) -> Counts {
    let tally = tally(takes.clone());
    Counts {
        takes: tally.clips,
        reference_words: tally.reference_words,
        word_errors: tally.word_errors,
        spacing_word_errors: tally.spacing_word_errors,
        word_error_rate: tally.word_error_rate().map(round1),
        other_error_rate: tally.other_error_rate().map(round1),
        splits: tally.splits,
        joins: tally.joins,
        heard_nothing: takes.filter(|t| t.heard_nothing.is_some()).count(),
    }
}

fn round1(value: f64) -> f64 {
    (value * 10.0).round() / 10.0
}

/// The summary's rows, in the configurations' order.
pub fn rows(clips: &[Clip], configurations: &[Configuration], outcomes: &[Outcome]) -> Vec<Row> {
    let any_microphone = clips.iter().any(|clip| clip.microphone.is_some());
    configurations
        .iter()
        .zip(outcomes)
        .map(|(configuration, outcome)| Row {
            configuration: configuration.to_string(),
            status: match outcome {
                Outcome::Ran { takes, device } => {
                    let mut by_microphone: BTreeMap<String, Vec<&TakeResult>> = BTreeMap::new();
                    if any_microphone {
                        for take in takes {
                            let microphone = clips[take.clip]
                                .microphone
                                .as_deref()
                                .unwrap_or(NO_MICROPHONE);
                            by_microphone
                                .entry(microphone.to_owned())
                                .or_default()
                                .push(take);
                        }
                    }
                    Status::Ran {
                        device: device.clone(),
                        counts: counts(takes.iter()),
                        transcribe_s: round1(
                            takes.iter().map(|t| t.transcribe_time.as_secs_f64()).sum(),
                        ),
                        microphones: by_microphone
                            .into_iter()
                            .map(|(microphone, takes)| (microphone, counts(takes.into_iter())))
                            .collect(),
                    }
                }
                Outcome::Unavailable(reason) => Status::NotAvailable { reason },
                Outcome::Failed(reason) => Status::Failed {
                    reason: reason.clone(),
                },
            },
        })
        .collect()
}

/// What the summary's header says about the run.
#[derive(Debug, Serialize)]
pub struct About {
    pub set: PathBuf,
    pub takes: usize,
    pub audio_minutes: f64,
    pub models: BTreeMap<String, PathBuf>,
    pub language: &'static str,
    pub prompt: Option<PathBuf>,
    pub started: String,
    pub results: PathBuf,
}

#[derive(Debug, Serialize)]
pub struct Summary {
    #[serde(flatten)]
    pub about: About,
    pub configurations: Vec<Row>,
}

impl Summary {
    pub fn markdown(&self) -> String {
        let about = &self.about;
        let mut out = String::new();
        let _ = writeln!(out, "# skald bench, {}\n", about.started);
        let _ = writeln!(
            out,
            "- Testsett: `{}`, {} opptak, {} min lyd",
            about.set.display(),
            about.takes,
            decimal(about.audio_minutes)
        );
        for (model, path) in &about.models {
            let _ = writeln!(out, "- {model}: `{}`", path.display());
        }
        let prompt = about
            .prompt
            .as_ref()
            .map_or_else(|| "ingen".to_owned(), |p| format!("`{}`", p.display()));
        let _ = writeln!(
            out,
            "- Språk: {}; ordskattens setning: {prompt}",
            about.language
        );
        let _ = writeln!(
            out,
            "- Ordene sammenlignes etter normaliseringen i `crates/skald-bench/src/normalise.rs`, og \
             særskrivinger og sammenskrivinger telles slik `crates/skald-bench/src/score.rs` beskriver. \
             «Uten mellomromsfeil» er ordfeilraten uten ordfeilene i særskrivinger og sammenskrivinger.\n"
        );

        out.push_str(
            "| Konfigurasjon | Opptak | Ord | Ordfeil | Ordfeilrate | Uten mellomromsfeil | Særskrivinger | \
             Sammenskrivinger | Hørte ingenting | Transkribering |\n\
             |---|---:|---:|---:|---:|---:|---:|---:|---:|---:|\n",
        );
        for row in &self.configurations {
            let name = &row.configuration;
            match &row.status {
                Status::Ran {
                    counts: c,
                    transcribe_s,
                    ..
                } => {
                    let _ = writeln!(
                        out,
                        "| `{name}` | {} | {} | {} | {} | {} | {} | {} | {} | {} s |",
                        c.takes,
                        c.reference_words,
                        c.word_errors,
                        percent(c.word_error_rate),
                        percent(c.other_error_rate),
                        c.splits,
                        c.joins,
                        c.heard_nothing,
                        decimal(*transcribe_s)
                    );
                }
                Status::NotAvailable { reason } => {
                    let _ = writeln!(out, "| `{name}` | ikke tilgjengelig: {reason} |||||||||");
                }
                Status::Failed { reason } => {
                    let _ = writeln!(out, "| `{name}` | feilet: {reason} |||||||||");
                }
            }
        }

        let with_microphones = self.configurations.iter().any(
            |row| matches!(&row.status, Status::Ran { microphones, .. } if !microphones.is_empty()),
        );
        if with_microphones {
            out.push_str(
                "\n| Konfigurasjon | Mikrofon | Opptak | Ord | Ordfeilrate | Uten mellomromsfeil | \
                 Særskrivinger | Sammenskrivinger | Hørte ingenting |\n\
                 |---|---|---:|---:|---:|---:|---:|---:|---:|\n",
            );
            for row in &self.configurations {
                let Status::Ran { microphones, .. } = &row.status else {
                    continue;
                };
                for (microphone, c) in microphones {
                    let _ = writeln!(
                        out,
                        "| `{}` | {microphone} | {} | {} | {} | {} | {} | {} | {} |",
                        row.configuration,
                        c.takes,
                        c.reference_words,
                        percent(c.word_error_rate),
                        percent(c.other_error_rate),
                        c.splits,
                        c.joins,
                        c.heard_nothing
                    );
                }
            }
        }
        out
    }
}

/// UTC time as `2026-11-27T183012Z`, for the name of a run's folder.
pub fn utc_stamp(unix_seconds: u64) -> String {
    let days = (unix_seconds / 86_400) as i64;
    let rest = unix_seconds % 86_400;
    // Howard Hinnant's days-to-civil algorithm, for the proleptic Gregorian calendar.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let mp = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}{:02}{:02}Z",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn result(clip: usize, reference: &str, text: &str) -> TakeResult {
        TakeResult {
            clip,
            text: text.to_owned(),
            model_text: Some(text.to_owned()),
            heard_nothing: None,
            complete: true,
            score: Score::new(reference, text),
            audio: Duration::from_secs(3),
            speech: Duration::from_secs(2),
            vad_time: Duration::from_millis(10),
            transcribe_time: Duration::from_millis(500),
        }
    }

    fn clip(name: &str, reference: &str, microphone: Option<&str>) -> Clip {
        Clip {
            name: name.to_owned(),
            wav: PathBuf::from(format!("{name}.wav")),
            reference: reference.to_owned(),
            microphone: microphone.map(str::to_owned),
        }
    }

    #[test]
    fn a_take_line_lists_each_kind_of_difference() {
        let clip = clip("s01", "I dag var en lang arbeidsdag hjemme.", None);
        let result = result(
            0,
            &clip.reference,
            "Idag var ei lang arbeids dag hjemme i dag.",
        );
        let line = take_line(&Configuration::default(), &clip, &result);
        assert_eq!(
            line.splits,
            vec![Pair {
                reference: "arbeidsdag".into(),
                text: "arbeids dag".into()
            }]
        );
        assert_eq!(
            line.joins,
            vec![Pair {
                reference: "i dag".into(),
                text: "idag".into()
            }]
        );
        assert_eq!(
            line.substitutions,
            vec![Pair {
                reference: "en".into(),
                text: "ei".into()
            }]
        );
        assert_eq!(line.insertions, vec!["i", "dag"]);
        assert!(line.deletions.is_empty());
        let json = serde_json::to_string(&line).unwrap();
        assert!(
            json.starts_with(r#"{"configuration":"nb-whisper-large,greedy,trim","take":"s01""#),
            "{json}"
        );
    }

    #[test]
    fn rows_break_down_by_microphone_when_the_manifest_names_any() {
        let clips = [
            clip("a-usb", "en lang arbeidsdag", Some("usb")),
            clip("a-far", "en lang arbeidsdag", Some("distance")),
            clip("b", "hei", None),
        ];
        let takes = vec![
            result(0, &clips[0].reference, "en lang arbeidsdag"),
            result(1, &clips[1].reference, "en lang arbeids dag"),
            result(2, &clips[2].reference, "hei"),
        ];
        let configurations = [
            Configuration::default(),
            Configuration::parse("beam5").unwrap(),
        ];
        let outcomes = [
            Outcome::Ran {
                takes,
                device: "MTL0".into(),
            },
            Outcome::Unavailable("motoren dekoder bare grådig"),
        ];
        let rows = rows(&clips, &configurations, &outcomes);
        let Status::Ran {
            counts,
            microphones,
            ..
        } = &rows[0].status
        else {
            panic!("{:?}", rows[0]);
        };
        assert_eq!(
            (counts.takes, counts.reference_words, counts.word_errors),
            (3, 7, 2)
        );
        assert_eq!(counts.splits, 1);
        assert_eq!(counts.word_error_rate, Some(28.6));
        assert_eq!(counts.other_error_rate, Some(0.0));
        let names: Vec<&str> = microphones.keys().map(String::as_str).collect();
        assert_eq!(names, [NO_MICROPHONE, "distance", "usb"]);
        assert_eq!(microphones["distance"].splits, 1);
        assert_eq!(microphones["usb"].word_errors, 0);

        let summary = Summary {
            about: About {
                set: PathBuf::from("/set"),
                takes: 3,
                audio_minutes: 0.15,
                models: BTreeMap::new(),
                language: "bokmål",
                prompt: None,
                started: "2026-11-27T183012Z".into(),
                results: PathBuf::from("/set/bench-results/x"),
            },
            configurations: rows,
        };
        let markdown = summary.markdown();
        assert!(
            markdown.contains("| `nb-whisper-large,greedy,trim` | 3 | 7 | 2 | 28,6 % | 0,0 % | 1 | 0 | 0 | 1,5 s |"),
            "{markdown}"
        );
        assert!(markdown.contains(
            "| `nb-whisper-large,beam5,trim` | ikke tilgjengelig: motoren dekoder bare grådig |"
        ));
        assert!(markdown.contains(
            "| `nb-whisper-large,greedy,trim` | distance | 1 | 3 | 66,7 % | 0,0 % | 1 | 0 | 0 |"
        ));

        let json = serde_json::to_value(&summary).unwrap();
        assert_eq!(json["configurations"][0]["status"], "ran");
        assert_eq!(json["configurations"][0]["splits"], 1);
        assert_eq!(json["configurations"][1]["status"], "not_available");
    }

    #[test]
    fn without_a_manifest_there_is_no_microphone_table() {
        let clips = [clip("a", "hei", None)];
        let outcomes = [Outcome::Ran {
            takes: vec![result(0, "hei", "hei")],
            device: "MTL0".into(),
        }];
        let rows = rows(&clips, &[Configuration::default()], &outcomes);
        assert!(
            matches!(&rows[0].status, Status::Ran { microphones, .. } if microphones.is_empty())
        );
    }

    #[test]
    fn utc_stamps() {
        assert_eq!(utc_stamp(0), "1970-01-01T000000Z");
        assert_eq!(utc_stamp(951_782_400), "2000-02-29T000000Z");
        assert_eq!(utc_stamp(1_795_804_212), "2026-11-27T183012Z");
    }
}
