//! Speech detection with Silero VAD: where in a take someone speaks, how long in all, and the take with
//! the silence before and after the speech cut away.

use std::ops::Range;
use std::path::Path;
use std::time::Duration;

use transcribe_cpp::{Model, VadOptions, VadSession};

use crate::{Error, SAMPLE_RATE};

/// A Silero VAD model, loaded once like the speech model.
pub struct Vad {
    session: VadSession,
}

impl std::fmt::Debug for Vad {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Vad").finish_non_exhaustive()
    }
}

impl Vad {
    /// Loads Silero VAD from a GGUF or a whisper.cpp `.bin` file.
    pub fn load(path: &Path) -> Result<Self, Error> {
        let load_error = |error: transcribe_cpp::Error| Error::Load {
            path: path.to_owned(),
            reason: error.to_string(),
        };
        crate::backends::load().map_err(|reason| Error::Load {
            path: path.to_owned(),
            reason,
        })?;
        let model = Model::load(path).map_err(load_error)?;
        let session = model.vad_session().map_err(load_error)?;
        Ok(Self { session })
    }

    /// Finds the speech in 16 kHz mono samples with Silero's defaults: a frame is speech at a
    /// probability of 0.5, 100 ms of silence ends a segment, each segment keeps 30 ms on either side,
    /// and a segment of 250 ms or less is dropped. Those defaults found no speech in any of the engine
    /// check's takes without speech (docs/research/engine-check.md, section 3).
    pub fn detect(&mut self, samples: &[f32]) -> Result<Speech, Error> {
        let result = self
            .session
            .run(samples, &VadOptions::default())
            .map_err(|error| Error::Vad {
                reason: error.to_string(),
            })?;
        let len = samples.len();
        let segments = result
            .segments
            .iter()
            .map(|segment| clamp(segment.start_sample, len)..clamp(segment.end_sample, len))
            .filter(|segment| !segment.is_empty())
            .collect();
        Ok(Speech { segments })
    }
}

fn clamp(sample: i64, len: usize) -> usize {
    usize::try_from(sample).unwrap_or(0).min(len)
}

/// The speech found in a take, as ranges of its samples in order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Speech {
    segments: Vec<Range<usize>>,
}

impl Speech {
    pub fn new(segments: Vec<Range<usize>>) -> Self {
        Self { segments }
    }

    pub fn segments(&self) -> &[Range<usize>] {
        &self.segments
    }

    /// All the segments added up, which the silence gate compares with its least speech: a take of
    /// several very short bursts counts as little speech, as the rule means it to.
    pub fn duration(&self) -> Duration {
        let samples: usize = self.segments.iter().map(ExactSizeIterator::len).sum();
        Duration::from_secs_f64(samples as f64 / f64::from(SAMPLE_RATE))
    }

    /// The take from [`TRIM_MARGIN`] before the first segment to as long after the last, which cuts the
    /// silence before and after the speech and keeps the pauses within it. Empty when there is no speech.
    pub fn trim<'a>(&self, samples: &'a [f32]) -> &'a [f32] {
        let margin = TRIM_MARGIN.as_millis() as usize * SAMPLE_RATE as usize / 1000;
        match (self.segments.first(), self.segments.last()) {
            (Some(first), Some(last)) => {
                let end = last.end.saturating_add(margin).min(samples.len());
                &samples[first.start.saturating_sub(margin).min(end)..end]
            }
            _ => &[],
        }
    }
}

/// The audio kept on each side of the speech when the silence around it is cut.
///
/// Evidence, the 20 FLEURS clips of docs/research/engine-check.md, clean and with noise, transcribed
/// with the take cut at different margins (word errors over 426 words):
///
/// | margin | clean | noisy | clean, 3 s of noise after | noisy, 3 s of noise after |
/// |---|---:|---:|---:|---:|
/// | not cut | 5.2 % | 20.2 % | 5.4 % | 19.5 % |
/// | 0 ms (Silero's own 30 ms) | 5.9 % | 23.9 % | 5.9 % | 23.9 % |
/// | 200 ms | 4.7 % | 21.1 % | 4.7 % | 21.1 % |
/// | 300 ms | 4.5 % | 20.9 % | 4.5 % | 20.9 % |
/// | 500 ms | 5.6 % | 19.2 % | 5.6 % | 18.5 % |
///
/// Cut close, the model loses words; at 500 ms it writes about as well as with the whole take.
pub const TRIM_MARGIN: Duration = Duration::from_millis(500);

#[cfg(test)]
mod tests {
    use super::*;

    const MS: usize = SAMPLE_RATE as usize / 1000;

    #[test]
    fn duration_adds_up_the_segments() {
        let speech = Speech::new(vec![100 * MS..400 * MS, 1000 * MS..1080 * MS]);
        assert_eq!(speech.duration(), Duration::from_millis(380));
        assert_eq!(Speech::default().duration(), Duration::ZERO);
    }

    #[test]
    fn several_short_bursts_add_up_to_little_speech() {
        let bursts = (0..4)
            .map(|i| (i * 500) * MS..(i * 500 + 50) * MS)
            .collect();
        assert_eq!(Speech::new(bursts).duration(), Duration::from_millis(200));
    }

    #[test]
    fn trim_keeps_the_margin_around_the_first_and_last_segment() {
        let samples: Vec<f32> = (0..4000 * MS).map(|n| n as f32).collect();
        let speech = Speech::new(vec![1000 * MS..1400 * MS, 1800 * MS..2200 * MS]);
        let trimmed = speech.trim(&samples);
        assert_eq!(trimmed.len(), 2200 * MS);
        assert_eq!(trimmed[0], (500 * MS) as f32);
        assert_eq!(*trimmed.last().unwrap(), (2700 * MS - 1) as f32);
    }

    #[test]
    fn trim_without_speech_leaves_nothing() {
        assert!(Speech::default().trim(&[0.5; 160]).is_empty());
    }

    #[test]
    fn trim_stays_within_the_take() {
        let samples = vec![0.0; 1000 * MS];
        let speech = Speech::new(vec![100 * MS..250 * MS, 260 * MS..900 * MS]);
        assert_eq!(speech.trim(&samples).len(), 1000 * MS);
        assert_eq!(speech.trim(&samples[..50 * MS]).len(), 50 * MS);
        let late = Speech::new(std::iter::once(3000 * MS..3500 * MS).collect());
        assert!(late.trim(&samples).is_empty());
    }

    #[test]
    fn segments_outside_the_take_are_clamped() {
        assert_eq!(clamp(-5, 100), 0);
        assert_eq!(clamp(150, 100), 100);
        assert_eq!(clamp(42, 100), 42);
    }
}
