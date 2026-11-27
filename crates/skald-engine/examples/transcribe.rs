//! Transcribes a WAV file as Skald transcribes a take, and prints the text, the timings and the silence
//! gate's decision.
//!
//! ```sh
//! cargo run --release -p skald-engine --example transcribe -- \
//!     --model ggml-model.bin --vad silero-vad-v6.2-F32.gguf \
//!     [--config config.toml] [--prompt prompt.txt] [--nynorsk] take.wav
//! ```
//!
//! The WAV file is 16 kHz mono. `--config` reads Skald's configuration for the ordskatt, the language and
//! the known phrases; `--prompt` sends a file's text as the ordskatt's sentence instead.

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use skald_core::Config;
use skald_core::config::Language;
use skald_core::silence_gate::{self, Decision};
use skald_engine::{Decoding, Engine, SAMPLE_RATE, TakeOptions};

struct Args {
    model: PathBuf,
    vad: PathBuf,
    config: Option<PathBuf>,
    prompt: Option<PathBuf>,
    nynorsk: bool,
    wav: PathBuf,
}

fn parse_args() -> Result<Args, String> {
    let mut model = None;
    let mut vad = None;
    let mut config = None;
    let mut prompt = None;
    let mut nynorsk = false;
    let mut wav = None;
    let mut args = std::env::args_os().skip(1);
    while let Some(arg) = args.next() {
        let mut value = |name: &str| {
            args.next()
                .map(PathBuf::from)
                .ok_or_else(|| format!("{name} needs a value"))
        };
        match arg.to_str() {
            Some("--model") => model = Some(value("--model")?),
            Some("--vad") => vad = Some(value("--vad")?),
            Some("--config") => config = Some(value("--config")?),
            Some("--prompt") => prompt = Some(value("--prompt")?),
            Some("--nynorsk") => nynorsk = true,
            Some(flag) if flag.starts_with("--") => return Err(format!("unknown option {flag}")),
            _ => wav = Some(PathBuf::from(arg)),
        }
    }
    let usage = "usage: transcribe --model FILE --vad FILE [--config FILE] [--prompt FILE] [--nynorsk] TAKE.wav";
    Ok(Args {
        model: model.ok_or(usage)?,
        vad: vad.ok_or(usage)?,
        config,
        prompt,
        nynorsk,
        wav: wav.ok_or(usage)?,
    })
}

fn read_wav(path: &PathBuf) -> Result<Vec<f32>, String> {
    let mut reader =
        hound::WavReader::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let spec = reader.spec();
    if spec.channels != 1 || spec.sample_rate != SAMPLE_RATE {
        return Err(format!(
            "{}: {} channels at {} Hz; the engine takes mono at {SAMPLE_RATE} Hz",
            path.display(),
            spec.channels,
            spec.sample_rate
        ));
    }
    let samples: Result<Vec<f32>, hound::Error> = match spec.sample_format {
        hound::SampleFormat::Float => reader.samples::<f32>().collect(),
        hound::SampleFormat::Int => {
            let scale = (1i64 << (spec.bits_per_sample - 1)) as f32;
            reader
                .samples::<i32>()
                .map(|sample| sample.map(|s| s as f32 / scale))
                .collect()
        }
    };
    samples.map_err(|e| format!("{}: {e}", path.display()))
}

/// The loudest level reading as Skald's level meter takes it: the RMS of each 60 ms, in dBFS.
fn loudest_dbfs(samples: &[f32]) -> f32 {
    samples
        .chunks(SAMPLE_RATE as usize * 60 / 1000)
        .map(|chunk| {
            let mean =
                chunk.iter().map(|&s| f64::from(s).powi(2)).sum::<f64>() / chunk.len() as f64;
            ((10.0 * mean.log10()) as f32).max(-100.0)
        })
        .fold(-100.0, f32::max)
}

fn ms(duration: Duration) -> String {
    format!("{:.0} ms", duration.as_secs_f64() * 1000.0)
}

fn run() -> Result<(), String> {
    let args = parse_args()?;
    let mut config = match &args.config {
        Some(path) => {
            let text =
                std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
            Config::from_toml(&text).map_err(|e| format!("{}: {e}", path.display()))?
        }
        None => Config::default(),
    };
    if let Some(path) = &args.prompt {
        config.vocabulary.sentence =
            std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    if args.nynorsk {
        config.language = Language::Nynorsk;
    }
    let samples = read_wav(&args.wav)?;

    skald_engine::route_logs_to_log_crate();
    let started = Instant::now();
    let mut engine = Engine::load(&args.model, &args.vad).map_err(|e| e.to_string())?;
    println!(
        "loaded in {} on {}",
        ms(started.elapsed()),
        engine.transcriber().device()
    );

    let prompt = config.vocabulary.prompt();
    let options = TakeOptions {
        language: config.language,
        prompt: &prompt,
        decoding: Decoding::Greedy,
    };
    let loudest = loudest_dbfs(&samples);
    let take = engine
        .take(&samples, loudest, &options, &config.silence)
        .map_err(|e| e.to_string())?;

    let audio = Duration::from_secs_f64(samples.len() as f64 / f64::from(SAMPLE_RATE));
    println!(
        "take {:.2} s, loudest {loudest:.1} dBFS, speech {} in {} segments (VAD {})",
        audio.as_secs_f64(),
        ms(take.speech.duration()),
        take.speech.segments().len(),
        ms(take.vad_time)
    );
    if let Some(transcript) = &take.transcript {
        let timings = &transcript.timings;
        println!(
            "prompt {} tokens, {} parts dropped from the front",
            transcript.prompt.tokens, transcript.prompt.dropped
        );
        println!(
            "transcribed in {}: mel {}, encoder {}, decoder {}{}",
            ms(timings.total),
            ms(timings.mel),
            ms(timings.encode),
            ms(timings.decode),
            if transcript.complete {
                ""
            } else {
                " (stopped early)"
            }
        );
        println!("text: {}", transcript.text);
    }
    match take.decision {
        Decision::Insert => println!("decision: insert"),
        Decision::HeardNothing(reason) => {
            println!("decision: «{}» ({reason:?})", silence_gate::HEARD_NOTHING);
        }
    }
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("transcribe: {message}");
            ExitCode::FAILURE
        }
    }
}
