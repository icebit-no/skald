//! Streaming conversion of mono audio from the microphone's rate to 16 kHz.

use rubato::audioadapter_buffers::direct::InterleavedSlice;
use rubato::{Fft, FixedSync, Indexing, Resampler, WindowFunction};

use crate::SAMPLE_RATE;

/// Output frames per resampler call, 32 ms. The filter is as sharp as this block is long: at 512
/// the passband is flat to 7.5 kHz from any input rate, and 8.5 kHz is gone.
const OUTPUT_BLOCK: usize = 512;

#[derive(Debug)]
pub(crate) struct ResampleError(String);

impl std::fmt::Display for ResampleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Takes mono audio in pieces of any size and hands on the same audio at 16 kHz, aligned with the
/// input: the filter's delay is cut from the start and flushed at the end.
pub(crate) enum ToSampleRate {
    /// The microphone already gives 16 kHz.
    Unchanged,
    Fft(Box<FftStream>),
}

pub(crate) struct FftStream {
    resampler: Fft<f32>,
    input_rate: u64,
    input: Vec<f32>,
    input_len: usize,
    output: Vec<f32>,
    delay_left: usize,
    frames_in: u64,
    frames_out: u64,
}

impl ToSampleRate {
    pub(crate) fn new(input_rate: u32) -> Result<Self, ResampleError> {
        if input_rate == SAMPLE_RATE {
            return Ok(Self::Unchanged);
        }
        let resampler = Fft::<f32>::new_custom(
            input_rate as usize,
            SAMPLE_RATE as usize,
            OUTPUT_BLOCK,
            1,
            1,
            WindowFunction::BlackmanHarris2,
            FixedSync::Output,
        )
        .map_err(|e| ResampleError(format!("cannot resample from {input_rate} Hz: {e}")))?;
        let input = vec![0.0; resampler.input_frames_max()];
        let output = vec![0.0; resampler.output_frames_max()];
        let delay_left = resampler.output_delay();
        Ok(Self::Fft(Box::new(FftStream {
            resampler,
            input_rate: u64::from(input_rate),
            input,
            input_len: 0,
            output,
            delay_left,
            frames_in: 0,
            frames_out: 0,
        })))
    }

    pub(crate) fn push(&mut self, samples: &[f32], out: &mut impl FnMut(&[f32])) {
        match self {
            Self::Unchanged => out(samples),
            Self::Fft(stream) => stream.push(samples, out),
        }
    }

    pub(crate) fn finish(&mut self, out: &mut impl FnMut(&[f32])) {
        if let Self::Fft(stream) = self {
            stream.finish(out);
        }
    }
}

impl FftStream {
    fn push(&mut self, mut samples: &[f32], out: &mut impl FnMut(&[f32])) {
        self.frames_in += samples.len() as u64;
        while !samples.is_empty() {
            let needed = self.resampler.input_frames_next();
            let taken = (needed - self.input_len).min(samples.len());
            self.input[self.input_len..self.input_len + taken].copy_from_slice(&samples[..taken]);
            self.input_len += taken;
            samples = &samples[taken..];
            if self.input_len == needed {
                self.process(None, out);
            }
        }
    }

    /// Runs what is left through the filter, then silence until the output is as long as the input.
    fn finish(&mut self, out: &mut impl FnMut(&[f32])) {
        let expected = (self.frames_in * u64::from(SAMPLE_RATE)).div_ceil(self.input_rate);
        while self.frames_out < expected {
            let partial = self.input_len;
            self.process(Some(partial), out);
        }
    }

    fn process(&mut self, partial_len: Option<usize>, out: &mut impl FnMut(&[f32])) {
        let input = InterleavedSlice::new(&self.input[..], 1, self.input.len())
            .expect("a mono buffer has one sample per frame");
        let output_len = self.output.len();
        let mut output = InterleavedSlice::new_mut(&mut self.output[..], 1, output_len)
            .expect("a mono buffer has one sample per frame");
        let indexing = partial_len.map(|len| Indexing {
            partial_len: Some(len),
            ..Indexing::default()
        });
        let (_, produced) = self
            .resampler
            .process_into_buffer(&input, &mut output, indexing.as_ref())
            .expect("buffers are sized from the resampler's own maximums");
        self.input_len = 0;

        let skipped = self.delay_left.min(produced);
        self.delay_left -= skipped;
        let expected = (self.frames_in * u64::from(SAMPLE_RATE)).div_ceil(self.input_rate);
        let mut fresh = &self.output[skipped..produced];
        if partial_len.is_some() {
            // Flushing: stop at the length the input calls for, not at the end of the padding.
            let wanted = expected.saturating_sub(self.frames_out) as usize;
            fresh = &fresh[..fresh.len().min(wanted)];
        }
        self.frames_out += fresh.len() as u64;
        if !fresh.is_empty() {
            out(fresh);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::TAU;

    fn sine(rate: u32, frequency: f64, amplitude: f64, seconds: f64) -> Vec<f32> {
        let len = (f64::from(rate) * seconds).round() as usize;
        (0..len)
            .map(|n| (amplitude * (TAU * frequency * n as f64 / f64::from(rate)).sin()) as f32)
            .collect()
    }

    /// Resamples `input` the way capture does: in callback-sized pieces, then a flush.
    fn resample(rate: u32, input: &[f32], piece: usize) -> Vec<f32> {
        let mut to_16k = ToSampleRate::new(rate).unwrap();
        let mut output = Vec::new();
        let mut collect = |s: &[f32]| output.extend_from_slice(s);
        for chunk in input.chunks(piece) {
            to_16k.push(chunk, &mut collect);
        }
        to_16k.finish(&mut collect);
        output
    }

    fn rms(samples: &[f32]) -> f64 {
        (samples.iter().map(|&s| f64::from(s).powi(2)).sum::<f64>() / samples.len() as f64).sqrt()
    }

    const RATES: [u32; 5] = [44_100, 48_000, 96_000, 22_050, 8_000];

    #[test]
    fn the_output_is_as_long_as_the_input_in_time() {
        for rate in RATES {
            for seconds in [1.0, 0.35, 0.01] {
                let input = sine(rate, 440.0, 0.5, seconds);
                let expected = (input.len() as u64 * 16_000).div_ceil(u64::from(rate)) as usize;
                assert_eq!(
                    resample(rate, &input, 512).len(),
                    expected,
                    "{rate} Hz, {seconds} s"
                );
            }
        }
    }

    #[test]
    fn a_tone_comes_out_when_it_went_in() {
        for rate in RATES {
            let output = resample(rate, &sine(rate, 1000.0, 0.5, 1.0), 480);
            let ideal = sine(SAMPLE_RATE, 1000.0, 0.5, 1.0);
            // Away from the edges, where the filter sees the cut, the output is the ideal tone.
            let worst = output[1600..14_400]
                .iter()
                .zip(&ideal[1600..14_400])
                .map(|(a, b)| (a - b).abs())
                .fold(0.0, f32::max);
            assert!(worst < 0.001, "{rate} Hz: off by {worst}");
        }
    }

    #[test]
    fn speech_frequencies_keep_their_level() {
        for rate in RATES {
            for frequency in [100.0, 1000.0, 3500.0, 7000.0, 7500.0] {
                if frequency >= f64::from(rate) / 2.0 {
                    continue;
                }
                let output = resample(rate, &sine(rate, frequency, 0.5, 1.0), 512);
                let level = rms(&output[1600..14_400]);
                assert!(
                    (level - 0.5 / 2f64.sqrt()).abs() < 0.005,
                    "{rate} Hz, {frequency} Hz tone: {level}"
                );
            }
        }
    }

    #[test]
    fn a_tone_above_eight_kilohertz_is_filtered_out() {
        // Without the filter, 12 kHz would fold back to 4 kHz at a 16 kHz rate.
        for rate in [44_100, 48_000, 96_000] {
            for frequency in [8500.0, 12_000.0, 20_000.0] {
                let output = resample(rate, &sine(rate, frequency, 0.5, 1.0), 512);
                let level = rms(&output[1600..14_400]);
                assert!(level < 1e-4, "{rate} Hz, {frequency} Hz tone: {level}");
            }
        }
    }

    #[test]
    fn the_piece_size_does_not_change_the_output() {
        let input = sine(44_100, 440.0, 0.5, 0.5);
        let whole = resample(44_100, &input, input.len());
        for piece in [1, 64, 441, 1000, 4096] {
            assert_eq!(resample(44_100, &input, piece), whole, "pieces of {piece}");
        }
    }

    #[test]
    fn sixteen_kilohertz_passes_through_untouched() {
        let input = sine(SAMPLE_RATE, 440.0, 0.5, 0.3);
        assert_eq!(resample(SAMPLE_RATE, &input, 256), input);
    }

    #[test]
    fn no_input_gives_no_output() {
        assert!(resample(48_000, &[], 512).is_empty());
    }
}
