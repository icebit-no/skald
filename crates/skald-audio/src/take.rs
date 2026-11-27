//! A take's audio path, apart from the device: the audio callback mixes each frame to mono into a
//! lock-free ring, and a worker drains the ring, resamples to 16 kHz, takes level readings and keeps
//! the samples up to the cap.

use std::time::Duration;

use cpal::{FromSample, Sample};
use rtrb::{Consumer, Producer};

use crate::level::LevelMeter;
use crate::microphone::Microphone;
use crate::resample::{ResampleError, ToSampleRate};
use crate::{LEVEL_FLOOR_DBFS, SAMPLE_RATE};

/// The longest take; audio after it is dropped.
pub const TAKE_CAP: Duration = Duration::from_secs(120);

pub(crate) const TAKE_CAP_SAMPLES: usize = TAKE_CAP.as_secs() as usize * SAMPLE_RATE as usize;

/// What a running take reports, in the order it happens.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TakeEvent {
    /// One reading for the meter, in dBFS, every [`LEVEL_INTERVAL`](crate::LEVEL_INTERVAL) of audio.
    Level(f32),
    /// The take reached [`TAKE_CAP`]; it keeps what it has and drops the rest until it is stopped.
    Capped,
    /// The microphone stopped delivering audio, for example because it was unplugged. The take
    /// keeps what came before.
    MicrophoneLost,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ending {
    Stopped,
    Capped,
    MicrophoneLost,
}

/// A finished take.
#[derive(Clone, Debug, PartialEq)]
pub struct Recording {
    /// Mono at [`SAMPLE_RATE`]; full scale is ±1.0.
    pub samples: Vec<f32>,
    /// Every level reading of the take in order, the same ones the take reported while it ran,
    /// plus one for a last stretch shorter than a reading.
    pub levels: Vec<f32>,
    pub ending: Ending,
    pub microphone: Microphone,
}

impl Recording {
    pub fn duration(&self) -> Duration {
        Duration::from_secs_f64(self.samples.len() as f64 / f64::from(SAMPLE_RATE))
    }

    /// The loudest level reading in dBFS, which the silence gate compares with speech level.
    pub fn loudest_dbfs(&self) -> f32 {
        self.levels.iter().copied().fold(LEVEL_FLOOR_DBFS, f32::max)
    }
}

/// The audio callback's work: mixes interleaved frames to mono f32 and pushes them into the ring,
/// without blocking or allocating. Frames that do not fit are dropped, and their number returned.
pub(crate) fn push_mono<T>(interleaved: &[T], channels: usize, ring: &mut Producer<f32>) -> usize
where
    T: Sample,
    f32: FromSample<T>,
{
    let frames = interleaved.len() / channels;
    let fitting = frames.min(ring.slots());
    if let Ok(chunk) = ring.write_chunk_uninit(fitting) {
        let scale = 1.0 / channels as f32;
        let mono = interleaved
            .chunks_exact(channels)
            .map(|frame| frame.iter().map(|&s| s.to_sample::<f32>()).sum::<f32>() * scale);
        chunk.fill_from_iter(mono);
    }
    frames - fitting
}

/// The worker's state for one take.
pub(crate) struct Processor {
    to_16k: ToSampleRate,
    meter: LevelMeter,
    samples: Vec<f32>,
    levels: Vec<f32>,
    cap: usize,
}

/// What the worker hands back when the take stops.
pub(crate) struct Processed {
    pub(crate) samples: Vec<f32>,
    pub(crate) levels: Vec<f32>,
    pub(crate) capped: bool,
}

impl Processor {
    pub(crate) fn new(input_rate: u32, cap: usize) -> Result<Self, ResampleError> {
        Ok(Self {
            to_16k: ToSampleRate::new(input_rate)?,
            meter: LevelMeter::default(),
            samples: Vec::with_capacity(cap),
            levels: Vec::with_capacity(cap.div_ceil(crate::level::SAMPLES_PER_READING)),
            cap,
        })
    }

    pub(crate) fn is_capped(&self) -> bool {
        self.samples.len() == self.cap
    }

    /// Drains whatever the ring holds.
    pub(crate) fn drain(&mut self, ring: &mut Consumer<f32>, events: &mut impl FnMut(TakeEvent)) {
        let Ok(chunk) = ring.read_chunk(ring.slots()) else {
            return;
        };
        let (first, second) = chunk.as_slices();
        self.push(first, events);
        self.push(second, events);
        chunk.commit_all();
    }

    pub(crate) fn push(&mut self, mono: &[f32], events: &mut impl FnMut(TakeEvent)) {
        let Self {
            to_16k,
            meter,
            samples,
            levels,
            cap,
        } = self;
        to_16k.push(mono, &mut |out| {
            keep(out, samples, *cap, meter, levels, events);
        });
    }

    pub(crate) fn finish(mut self, events: &mut impl FnMut(TakeEvent)) -> Processed {
        let Self {
            to_16k,
            meter,
            samples,
            levels,
            cap,
        } = &mut self;
        to_16k.finish(&mut |out| keep(out, samples, *cap, meter, levels, events));
        levels.extend(meter.finish());
        let capped = self.is_capped();
        Processed {
            samples: self.samples,
            levels: self.levels,
            capped,
        }
    }
}

fn keep(
    out: &[f32],
    samples: &mut Vec<f32>,
    cap: usize,
    meter: &mut LevelMeter,
    levels: &mut Vec<f32>,
    events: &mut impl FnMut(TakeEvent),
) {
    let room = cap - samples.len();
    if room == 0 {
        return;
    }
    let kept = &out[..out.len().min(room)];
    samples.extend_from_slice(kept);
    meter.push(kept, |reading| {
        levels.push(reading);
        events(TakeEvent::Level(reading));
    });
    if samples.len() == cap {
        events(TakeEvent::Capped);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::level::SAMPLES_PER_READING;
    use rtrb::RingBuffer;
    use std::f64::consts::TAU;

    /// A tone in the given number of channels, interleaved; only the first channel carries it when
    /// `left_only` is set.
    fn interleaved_sine(rate: u32, channels: usize, seconds: f64, left_only: bool) -> Vec<f32> {
        let frames = (f64::from(rate) * seconds).round() as usize;
        let mut out = Vec::with_capacity(frames * channels);
        for n in 0..frames {
            let s = (0.5 * (TAU * 440.0 * n as f64 / f64::from(rate)).sin()) as f32;
            for channel in 0..channels {
                out.push(if left_only && channel > 0 { 0.0 } else { s });
            }
        }
        out
    }

    struct Run {
        processed: Processed,
        events: Vec<TakeEvent>,
    }

    /// Feeds `input` through the ring in callback-sized buffers, draining after each as the worker
    /// would, then finishes the take.
    fn run(rate: u32, channels: usize, input: &[f32], cap: usize) -> Run {
        let (mut producer, mut consumer) = RingBuffer::new(rate as usize);
        let mut processor = Processor::new(rate, cap).unwrap();
        let mut events = Vec::new();
        let mut record = |e| events.push(e);
        for buffer in input.chunks(512 * channels) {
            assert_eq!(push_mono(buffer, channels, &mut producer), 0);
            processor.drain(&mut consumer, &mut record);
        }
        let processed = processor.finish(&mut record);
        Run { processed, events }
    }

    fn rms(samples: &[f32]) -> f64 {
        (samples.iter().map(|&s| f64::from(s).powi(2)).sum::<f64>() / samples.len() as f64).sqrt()
    }

    #[test]
    fn every_common_rate_and_layout_becomes_16_khz_mono() {
        for rate in [44_100, 48_000, 96_000] {
            for channels in [1, 2] {
                let input = interleaved_sine(rate, channels, 2.0, false);
                let run = run(rate, channels, &input, TAKE_CAP_SAMPLES);
                let samples = &run.processed.samples;
                assert_eq!(samples.len(), 32_000, "{rate} Hz × {channels}");
                let level = rms(&samples[1600..30_400]);
                assert!(
                    (level - 0.5 / 2f64.sqrt()).abs() < 0.005,
                    "{rate} Hz × {channels}: {level}"
                );
            }
        }
    }

    #[test]
    fn readings_flow_while_the_take_runs() {
        let input = interleaved_sine(48_000, 2, 2.4, false);
        let run = run(48_000, 2, &input, TAKE_CAP_SAMPLES);
        let readings: Vec<f32> = run
            .events
            .iter()
            .filter_map(|e| match e {
                TakeEvent::Level(r) => Some(*r),
                _ => None,
            })
            .collect();
        assert_eq!(readings.len(), 40);
        assert_eq!(run.processed.levels, readings);
        // Leave out the first reading, which holds the filter's run-in.
        for reading in &readings[1..] {
            assert!((reading - -9.03).abs() < 0.1, "{reading}");
        }
    }

    #[test]
    fn a_channel_carrying_nothing_halves_the_level() {
        let input = interleaved_sine(48_000, 2, 1.0, true);
        let run = run(48_000, 2, &input, TAKE_CAP_SAMPLES);
        let level = rms(&run.processed.samples[1600..14_400]);
        assert!((level - 0.25 / 2f64.sqrt()).abs() < 0.005, "{level}");
    }

    #[test]
    fn integer_samples_are_scaled_to_full_range() {
        let (mut producer, mut consumer) = RingBuffer::new(8);
        push_mono(&[i16::MAX, i16::MAX, i16::MIN, 0], 2, &mut producer);
        let mono: Vec<f32> = std::iter::from_fn(|| consumer.pop().ok()).collect();
        assert!((mono[0] - 1.0).abs() < 1e-4);
        assert!((mono[1] - -0.5).abs() < 1e-4);
    }

    #[test]
    fn a_full_ring_drops_frames_instead_of_blocking() {
        let (mut producer, _consumer) = RingBuffer::new(3);
        assert_eq!(push_mono(&[0.1f32; 10], 2, &mut producer), 2);
        assert_eq!(push_mono(&[0.1f32; 4], 2, &mut producer), 2);
    }

    #[test]
    fn the_cap_ends_the_take_once() {
        let cap = SAMPLES_PER_READING * 10;
        let input = interleaved_sine(44_100, 1, 2.0, false);
        let run = run(44_100, 1, &input, cap);
        assert_eq!(run.processed.samples.len(), cap);
        assert!(run.processed.capped);
        assert_eq!(run.processed.levels.len(), 10);
        let capped = run
            .events
            .iter()
            .filter(|e| **e == TakeEvent::Capped)
            .count();
        assert_eq!(capped, 1);
        assert_eq!(run.events.last(), Some(&TakeEvent::Capped));
    }

    #[test]
    fn the_real_cap_is_two_minutes_of_16_khz() {
        let input = vec![0.01f32; 121 * SAMPLE_RATE as usize];
        let run = run(SAMPLE_RATE, 1, &input, TAKE_CAP_SAMPLES);
        assert_eq!(run.processed.samples.len(), 1_920_000);
        assert!(run.processed.capped);
    }

    #[test]
    fn a_take_shorter_than_the_cap_is_not_capped() {
        let input = interleaved_sine(48_000, 1, 0.5, false);
        let run = run(48_000, 1, &input, TAKE_CAP_SAMPLES);
        assert!(!run.processed.capped);
        assert!(!run.events.contains(&TakeEvent::Capped));
    }

    #[test]
    fn the_loudest_reading_is_the_highest_level() {
        let recording = Recording {
            samples: vec![],
            levels: vec![-60.0, -12.5, -40.0],
            ending: Ending::Stopped,
            microphone: Microphone {
                id: crate::MicId::new("m"),
                name: "m".into(),
            },
        };
        assert_eq!(recording.loudest_dbfs(), -12.5);
        let silent = Recording {
            levels: vec![],
            ..recording
        };
        assert_eq!(silent.loudest_dbfs(), LEVEL_FLOOR_DBFS);
    }
}
