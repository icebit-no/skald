//! Takes as the model hears them: a WAV file read to 16 kHz mono, then the filters a configuration puts
//! it through.

use std::path::Path;

use rubato::audioadapter_buffers::direct::InterleavedSlice;
use rubato::{Fft, FixedSync, Resampler};
use skald_audio::SAMPLE_RATE;
use skald_audio::filter::{self, HighPass};

use crate::configuration::Configuration;

/// Reads a WAV file of any rate and channel count, 8 to 32 bit integer or 32 bit float, as 16 kHz mono
/// full scale ±1.0: the channels averaged, the rate converted.
pub fn read_wav(path: &Path) -> Result<Vec<f32>, String> {
    let error = |e: hound::Error| format!("{}: {e}", path.display());
    let mut reader = hound::WavReader::open(path).map_err(error)?;
    let spec = reader.spec();
    let interleaved: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader.samples::<f32>().collect::<Result<_, _>>(),
        hound::SampleFormat::Int => {
            let scale = (1i64 << (spec.bits_per_sample - 1)) as f32;
            reader
                .samples::<i32>()
                .map(|sample| sample.map(|s| s as f32 / scale))
                .collect()
        }
    }
    .map_err(error)?;
    let channels = usize::from(spec.channels.max(1));
    let mono: Vec<f32> = interleaved
        .chunks_exact(channels)
        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
        .collect();
    resample(&mono, spec.sample_rate, SAMPLE_RATE).map_err(|e| format!("{}: {e}", path.display()))
}

/// The same audio at another rate, as long as the original and aligned with it.
fn resample(samples: &[f32], from: u32, to: u32) -> Result<Vec<f32>, String> {
    if from == to || samples.is_empty() {
        return Ok(samples.to_vec());
    }
    let mut resampler = Fft::<f32>::new(from as usize, to as usize, 1024, 1, FixedSync::Input)
        .map_err(|e| format!("Kan ikke gjøre om {from} Hz til {to} Hz: {e}"))?;
    let input = InterleavedSlice::new(samples, 1, samples.len()).map_err(|e| e.to_string())?;
    let output = resampler
        .process_all(&input, samples.len(), None)
        .map_err(|e| format!("Kunne ikke gjøre om {from} Hz til {to} Hz: {e}"))?;
    Ok(output.take_data())
}

/// The take after the configuration's filters, in the order the configuration module gives.
pub fn filtered(take: &[f32], configuration: &Configuration) -> Result<Vec<f32>, String> {
    let mut samples = take.to_vec();
    if configuration.high_pass {
        HighPass::new(filter::HIGH_PASS_HZ).process(&mut samples);
    }
    if configuration.rnnoise {
        samples = denoise(&samples)?;
    }
    if configuration.normalise_level {
        filter::normalise_level(&mut samples, filter::NORMALISED_LEVEL_DBFS);
    }
    Ok(samples)
}

/// RNNoise's rate and frame.
const RNNOISE_RATE: u32 = 48_000;
const RNNOISE_FRAME: usize = nnnoiseless::DenoiseState::FRAME_SIZE;

/// RNNoise through nnnoiseless, a Rust port of Xiph's RNNoise with its weights built in. RNNoise works on
/// 10 ms frames at 48 kHz with samples in the range of 16-bit integers, so the take is converted to that
/// and back. Each frame's output is the frame before it, so the first output is dropped and one frame of
/// silence after the take brings out its end.
fn denoise(take: &[f32]) -> Result<Vec<f32>, String> {
    let mut upsampled = resample(take, SAMPLE_RATE, RNNOISE_RATE)?;
    let len = upsampled.len();
    let frames = len.div_ceil(RNNOISE_FRAME) + 1;
    upsampled.resize(frames * RNNOISE_FRAME, 0.0);
    for sample in &mut upsampled {
        *sample *= 32768.0;
    }

    let mut state = nnnoiseless::DenoiseState::new();
    let mut denoised = Vec::with_capacity(upsampled.len());
    let mut frame = [0.0f32; RNNOISE_FRAME];
    let (inputs, _) = upsampled.as_chunks::<RNNOISE_FRAME>();
    for (i, input) in inputs.iter().enumerate() {
        state.process_frame(&mut frame, input);
        if i > 0 {
            denoised.extend(frame.iter().map(|s| s / 32768.0));
        }
    }
    denoised.truncate(len);

    let mut out = resample(&denoised, RNNOISE_RATE, SAMPLE_RATE)?;
    out.resize(take.len(), 0.0);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::TAU;

    fn sine(amplitude: f32, frequency: f32, rate: u32, seconds: f32) -> Vec<f32> {
        let len = (rate as f32 * seconds) as usize;
        (0..len)
            .map(|n| amplitude * (TAU * frequency * n as f32 / rate as f32).sin())
            .collect()
    }

    fn rms(samples: &[f32]) -> f32 {
        (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
    }

    /// Pseudo-random noise in ±1.0, the same on every run.
    fn noise(len: usize, amplitude: f32) -> Vec<f32> {
        let mut state = 0x2545_f491_u32;
        (0..len)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                amplitude * (state as f32 / u32::MAX as f32 * 2.0 - 1.0)
            })
            .collect()
    }

    struct Wav(std::path::PathBuf);

    impl Wav {
        fn write(name: &str, spec: hound::WavSpec, samples: &[f32]) -> Self {
            let path =
                std::env::temp_dir().join(format!("skald-bench-{name}-{}.wav", std::process::id()));
            let mut writer = hound::WavWriter::create(&path, spec).unwrap();
            for &s in samples {
                match spec.sample_format {
                    hound::SampleFormat::Float => writer.write_sample(s).unwrap(),
                    hound::SampleFormat::Int => writer.write_sample((s * 32767.0) as i16).unwrap(),
                }
            }
            writer.finalize().unwrap();
            Self(path)
        }
    }

    impl Drop for Wav {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    #[test]
    fn a_16_khz_mono_file_is_read_as_it_is() {
        let samples = sine(0.5, 440.0, SAMPLE_RATE, 0.5);
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: SAMPLE_RATE,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let wav = Wav::write("mono16", spec, &samples);
        let read = read_wav(&wav.0).unwrap();
        assert_eq!(read.len(), samples.len());
        assert!(read.iter().zip(&samples).all(|(a, b)| (a - b).abs() < 1e-4));
    }

    #[test]
    fn a_48_khz_stereo_file_becomes_16_khz_mono() {
        let left = sine(0.4, 440.0, 48_000, 1.0);
        let interleaved: Vec<f32> = left.iter().flat_map(|&s| [s, s]).collect();
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: 48_000,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let wav = Wav::write("stereo48", spec, &interleaved);
        let read = read_wav(&wav.0).unwrap();
        assert_eq!(read.len(), SAMPLE_RATE as usize);
        let expected = sine(0.4, 440.0, SAMPLE_RATE, 1.0);
        let middle = 2000..14_000;
        let worst = read[middle.clone()]
            .iter()
            .zip(&expected[middle])
            .map(|(a, b)| (a - b).abs())
            .fold(0.0, f32::max);
        assert!(worst < 0.01, "{worst}");
    }

    #[test]
    fn no_filter_leaves_the_take_alone() {
        let take = sine(0.3, 440.0, SAMPLE_RATE, 0.2);
        assert_eq!(filtered(&take, &Configuration::default()).unwrap(), take);
    }

    /// The amplitude of one frequency in a stretch of 16 kHz audio.
    fn amplitude(samples: &[f32], frequency: f32) -> f32 {
        let (mut sin, mut cos) = (0.0f64, 0.0f64);
        for (n, &s) in samples.iter().enumerate() {
            let phase = f64::from(TAU * frequency) * n as f64 / f64::from(SAMPLE_RATE);
            sin += f64::from(s) * phase.sin();
            cos += f64::from(s) * phase.cos();
        }
        (2.0 * sin.hypot(cos) / samples.len() as f64) as f32
    }

    #[test]
    fn the_filters_run_when_named() {
        // A quiet 1 kHz tone under a louder 20 Hz rumble, as from a desk being knocked.
        let take: Vec<f32> = sine(0.01, 1000.0, SAMPLE_RATE, 1.0)
            .iter()
            .zip(sine(0.05, 20.0, SAMPLE_RATE, 1.0))
            .map(|(tone, rumble)| tone + rumble)
            .collect();
        let configuration = Configuration::parse("highpass,normalise").unwrap();
        let out = filtered(&take, &configuration).unwrap();
        assert_eq!(out.len(), take.len());
        let tail = &out[out.len() / 2..];
        let (tone, rumble) = (amplitude(tail, 1000.0), amplitude(tail, 20.0));
        assert!(rumble < tone / 2.0, "rumble {rumble} against tone {tone}");
        let level = 20.0 * rms(tail).log10();
        assert!(
            (level - filter::NORMALISED_LEVEL_DBFS).abs() < 1.0,
            "{level}"
        );
    }

    #[test]
    fn rnnoise_keeps_the_length_and_turns_noise_down() {
        let take = noise(SAMPLE_RATE as usize * 2, 0.05);
        let out = denoise(&take).unwrap();
        assert_eq!(out.len(), take.len());
        let after = &out[SAMPLE_RATE as usize..];
        let before = &take[SAMPLE_RATE as usize..];
        assert!(
            rms(after) < rms(before) / 4.0,
            "{} against {}",
            rms(after),
            rms(before)
        );
    }

    #[test]
    fn rnnoise_of_silence_is_silence() {
        let out = denoise(&[0.0; 8000]).unwrap();
        assert_eq!(out.len(), 8000);
        assert!(out.iter().all(|s| s.abs() < 1e-6));
    }
}
