//! Runs every take of a test set through each configuration, as Skald runs a take: the level check on the
//! take as recorded, the filters, VAD and its 250 ms check, the silence around the speech cut or kept,
//! the model, and the check of its text. A take the gate stops counts as nothing inserted.
//!
//! Each model is loaded once and runs all its configurations before the next is loaded; Silero VAD is
//! loaded once for the whole run.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use skald_audio::{LEVEL_FLOOR_DBFS, LEVEL_INTERVAL, SAMPLE_RATE};
use skald_core::config::Language;
use skald_core::silence::Silence;
use skald_core::silence_gate::{self, Decision, Reason};
use skald_core::vocabulary::Prompt;
use skald_engine::{TakeOptions, Transcriber, Vad};

use crate::audio;
use crate::configuration::{Configuration, Decoding, Model};
use crate::score::Score;
use crate::set::Clip;

/// What the run needs besides the takes and configurations.
#[derive(Debug)]
pub struct Setup {
    /// The file of each model, for the models the configurations name.
    pub models: BTreeMap<Model, PathBuf>,
    pub vad: PathBuf,
    pub language: Language,
    pub prompt: Prompt,
    pub silence: Silence,
}

/// One take through one configuration.
#[derive(Debug, Clone)]
pub struct TakeResult {
    pub clip: usize,
    /// What would be inserted: the model's text, or nothing when the gate stopped the take.
    pub text: String,
    /// What the model wrote, if it ran.
    pub model_text: Option<String>,
    pub heard_nothing: Option<Reason>,
    /// False when the model stopped early.
    pub complete: bool,
    pub score: Score,
    pub audio: Duration,
    pub speech: Duration,
    pub vad_time: Duration,
    pub transcribe_time: Duration,
}

#[derive(Debug, Clone)]
pub enum Outcome {
    Ran {
        takes: Vec<TakeResult>,
        device: String,
    },
    Unavailable(&'static str),
    Failed(String),
}

/// Told about each step as it happens, for progress and for writing results as they come.
pub trait Progress {
    fn loading(&mut self, model: Model, path: &std::path::Path);
    fn loaded(&mut self, model: Model, device: &str, took: Duration);
    fn configuration(&mut self, index: usize, configuration: &Configuration);
    fn take(&mut self, configuration: &Configuration, clip: &Clip, result: &TakeResult);
    fn finished(&mut self, index: usize, configuration: &Configuration, outcome: &Outcome);
}

/// Runs the takes through the configurations; the outcomes are in the configurations' order. Fails only
/// when the takes or VAD cannot be read; a model that cannot be loaded or a take that fails is the
/// outcome of its configurations.
pub fn run(
    clips: &[Clip],
    configurations: &[Configuration],
    setup: &Setup,
    progress: &mut impl Progress,
) -> Result<Vec<Outcome>, String> {
    let takes: Vec<Vec<f32>> = clips
        .iter()
        .map(|clip| audio::read_wav(&clip.wav))
        .collect::<Result<_, _>>()?;
    let mut vad = Vad::load(&setup.vad).map_err(|e| e.to_string())?;

    let mut outcomes: Vec<Option<Outcome>> = configurations
        .iter()
        .map(|c| c.unavailable().map(Outcome::Unavailable))
        .collect();
    for (index, configuration) in configurations.iter().enumerate() {
        if let Some(outcome) = &outcomes[index] {
            progress.finished(index, configuration, outcome);
        }
    }

    let mut models: Vec<Model> = Vec::new();
    for (configuration, outcome) in configurations.iter().zip(&outcomes) {
        if outcome.is_none() && !models.contains(&configuration.model) {
            models.push(configuration.model);
        }
    }

    for model in models {
        let indices: Vec<usize> = (0..configurations.len())
            .filter(|&i| outcomes[i].is_none() && configurations[i].model == model)
            .collect();
        let mut transcriber = match load(model, setup, progress) {
            Ok(transcriber) => transcriber,
            Err(reason) => {
                for &index in &indices {
                    let outcome = Outcome::Failed(reason.clone());
                    progress.finished(index, &configurations[index], &outcome);
                    outcomes[index] = Some(outcome);
                }
                continue;
            }
        };
        let device = transcriber.device();
        for index in indices {
            let configuration = &configurations[index];
            progress.configuration(index, configuration);
            let outcome = match run_configuration(
                configuration,
                clips,
                &takes,
                &mut transcriber,
                &mut vad,
                setup,
                progress,
            ) {
                Ok(takes) => Outcome::Ran {
                    takes,
                    device: device.clone(),
                },
                Err(reason) => Outcome::Failed(reason),
            };
            progress.finished(index, configuration, &outcome);
            outcomes[index] = Some(outcome);
        }
        // The model is dropped here, before the next is loaded and before the process exits.
    }

    Ok(outcomes
        .into_iter()
        .map(|outcome| outcome.expect("every configuration has run, failed or is unavailable"))
        .collect())
}

fn load(model: Model, setup: &Setup, progress: &mut impl Progress) -> Result<Transcriber, String> {
    let path = setup.models.get(&model).ok_or_else(|| {
        format!(
            "Ingen fil for {}. Oppgi den med --model {}=FIL.",
            model.id(),
            model.id()
        )
    })?;
    progress.loading(model, path);
    let started = Instant::now();
    let transcriber = Transcriber::load(path).map_err(|e| e.to_string())?;
    progress.loaded(model, &transcriber.device(), started.elapsed());
    Ok(transcriber)
}

fn run_configuration(
    configuration: &Configuration,
    clips: &[Clip],
    takes: &[Vec<f32>],
    transcriber: &mut Transcriber,
    vad: &mut Vad,
    setup: &Setup,
    progress: &mut impl Progress,
) -> Result<Vec<TakeResult>, String> {
    let decoding = match configuration.decoding {
        Decoding::Greedy => skald_engine::Decoding::Greedy,
        Decoding::Beam5 => unreachable!("an unavailable configuration does not run"),
    };
    let options = TakeOptions {
        language: setup.language,
        prompt: &setup.prompt,
        decoding,
    };
    let mut results = Vec::with_capacity(clips.len());
    for (index, (clip, take)) in clips.iter().zip(takes).enumerate() {
        let result = run_take(
            index,
            clip,
            take,
            configuration,
            transcriber,
            vad,
            &options,
            setup,
        )
        .map_err(|e| format!("{}: {e}", clip.name))?;
        progress.take(configuration, clip, &result);
        results.push(result);
    }
    Ok(results)
}

#[allow(clippy::too_many_arguments)]
fn run_take(
    index: usize,
    clip: &Clip,
    take: &[f32],
    configuration: &Configuration,
    transcriber: &mut Transcriber,
    vad: &mut Vad,
    options: &TakeOptions,
    setup: &Setup,
) -> Result<TakeResult, String> {
    let mut result = TakeResult {
        clip: index,
        text: String::new(),
        model_text: None,
        heard_nothing: None,
        complete: true,
        score: Score::new(&clip.reference, ""),
        audio: seconds(take.len()),
        speech: Duration::ZERO,
        vad_time: Duration::ZERO,
        transcribe_time: Duration::ZERO,
    };
    // The level check reads the take as recorded, as the meter does.
    let loudest = loudest_dbfs(take);
    if let Err(reason) = silence_gate::check_level(loudest) {
        result.heard_nothing = Some(reason);
        return Ok(result);
    }

    let samples = audio::filtered(take, configuration)?;
    let started = Instant::now();
    let speech = vad.detect(&samples).map_err(|e| e.to_string())?;
    result.vad_time = started.elapsed();
    result.speech = speech.duration();
    if let Err(reason) = silence_gate::check_speech(result.speech) {
        result.heard_nothing = Some(reason);
        return Ok(result);
    }

    let input = if configuration.trim_silence {
        speech.trim(&samples)
    } else {
        &samples[..]
    };
    let transcript = transcriber
        .transcribe(input, options)
        .map_err(|e| e.to_string())?;
    result.transcribe_time = transcript.timings.total;
    result.complete = transcript.complete;
    match silence_gate::decide(loudest, result.speech, &transcript.text, &setup.silence) {
        Decision::Insert => result.text = transcript.text.clone(),
        Decision::HeardNothing(reason) => result.heard_nothing = Some(reason),
    }
    result.model_text = Some(transcript.text);
    result.score = Score::new(&clip.reference, &result.text);
    Ok(result)
}

fn seconds(samples: usize) -> Duration {
    Duration::from_secs_f64(samples as f64 / f64::from(SAMPLE_RATE))
}

/// The take's loudest level reading: the RMS of each stretch the meter reads, in dBFS.
fn loudest_dbfs(samples: &[f32]) -> f32 {
    let per_reading = (u128::from(SAMPLE_RATE) * LEVEL_INTERVAL.as_millis() / 1000) as usize;
    samples
        .chunks(per_reading)
        .map(|chunk| {
            let mean =
                chunk.iter().map(|&s| f64::from(s).powi(2)).sum::<f64>() / chunk.len() as f64;
            ((10.0 * mean.log10()) as f32).max(LEVEL_FLOOR_DBFS)
        })
        .fold(LEVEL_FLOOR_DBFS, f32::max)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loudest_reading_of_a_take() {
        let mut take = vec![0.0f32; 16_000];
        take[960..1920].fill(0.5);
        assert!((loudest_dbfs(&take) - 20.0 * 0.5f32.log10()).abs() < 1e-4);
        assert_eq!(loudest_dbfs(&[0.0; 100]), LEVEL_FLOOR_DBFS);
        assert_eq!(loudest_dbfs(&[]), LEVEL_FLOOR_DBFS);
    }
}
