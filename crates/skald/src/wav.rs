//! A WAV file as a take, for `skald transcribe`: 16 kHz samples, the channels mixed to mono, and the loudest
//! level reading the silence gate starts from.

use std::path::Path;

use skald_audio::{LEVEL_FLOOR_DBFS, LEVEL_INTERVAL, SAMPLE_RATE};

/// The file's samples at full scale ±1.0. The error is the line to show the user.
pub(crate) fn read(path: &Path) -> Result<Vec<f32>, String> {
    let failed = |error: hound::Error| format!("Kan ikke lese {}: {error}", path.display());
    let mut reader = hound::WavReader::open(path).map_err(failed)?;
    let spec = reader.spec();
    if spec.sample_rate != SAMPLE_RATE {
        return Err(format!(
            "{} er tatt opp med {} Hz; Skald tar lydfiler med {} kHz.",
            path.display(),
            spec.sample_rate,
            SAMPLE_RATE / 1000
        ));
    }
    let interleaved: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader.samples::<f32>().collect::<Result<_, _>>(),
        hound::SampleFormat::Int => {
            let scale = (1_i64 << (spec.bits_per_sample.max(1) - 1)) as f32;
            reader
                .samples::<i32>()
                .map(|sample| sample.map(|s| s as f32 / scale))
                .collect::<Result<_, _>>()
        }
    }
    .map_err(failed)?;
    let channels = usize::from(spec.channels.max(1));
    Ok(interleaved
        .chunks_exact(channels)
        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
        .collect())
}

/// The loudest level reading as Skald's meter takes them while recording: the RMS of each stretch of
/// [`LEVEL_INTERVAL`], in dBFS.
pub(crate) fn loudest_dbfs(samples: &[f32]) -> f32 {
    let per_reading = (SAMPLE_RATE as u128 * LEVEL_INTERVAL.as_millis() / 1000) as usize;
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
pub(crate) mod tests {
    use std::f32::consts::TAU;
    use std::path::PathBuf;

    use super::*;
    use crate::files::tests::Scratch;

    /// A second of a 440 Hz tone at half scale in the given layout, written to `path`.
    pub(crate) fn write_tone(path: &Path, rate: u32, channels: u16, float: bool) {
        let spec = hound::WavSpec {
            channels,
            sample_rate: rate,
            bits_per_sample: if float { 32 } else { 16 },
            sample_format: if float {
                hound::SampleFormat::Float
            } else {
                hound::SampleFormat::Int
            },
        };
        let mut writer = hound::WavWriter::create(path, spec).unwrap();
        for n in 0..rate {
            let s = 0.5 * (TAU * 440.0 * n as f32 / rate as f32).sin();
            for channel in 0..channels {
                let s = if channel == 0 { s } else { 0.0 };
                if float {
                    writer.write_sample(s).unwrap();
                } else {
                    writer
                        .write_sample((s * f32::from(i16::MAX)) as i16)
                        .unwrap();
                }
            }
        }
        writer.finalize().unwrap();
    }

    fn tone(scratch: &Scratch, name: &str, rate: u32, channels: u16, float: bool) -> PathBuf {
        let path = scratch.0.join(name);
        write_tone(&path, rate, channels, float);
        path
    }

    #[test]
    fn reads_16_khz_whole_numbers_and_floats_at_full_scale() {
        let scratch = Scratch::new("wav");
        for float in [false, true] {
            let samples = read(&tone(&scratch, "mono.wav", 16_000, 1, float)).unwrap();
            assert_eq!(samples.len(), 16_000);
            let peak = samples.iter().copied().fold(0.0, f32::max);
            assert!((peak - 0.5).abs() < 0.01, "{peak}");
            // A sine at half scale reads −9 dBFS.
            assert!((loudest_dbfs(&samples) + 9.03).abs() < 0.1);
        }
    }

    #[test]
    fn mixes_the_channels_to_mono() {
        let scratch = Scratch::new("wav-stereo");
        let samples = read(&tone(&scratch, "stereo.wav", 16_000, 2, false)).unwrap();
        assert_eq!(samples.len(), 16_000);
        let peak = samples.iter().copied().fold(0.0, f32::max);
        assert!((peak - 0.25).abs() < 0.01, "{peak}");
    }

    #[test]
    fn another_rate_or_no_file_says_why_in_bokmål() {
        let scratch = Scratch::new("wav-rate");
        let error = read(&tone(&scratch, "cd.wav", 44_100, 1, false)).unwrap_err();
        assert!(
            error.ends_with("er tatt opp med 44100 Hz; Skald tar lydfiler med 16 kHz."),
            "{error}"
        );
        let error = read(&scratch.0.join("borte.wav")).unwrap_err();
        assert!(error.starts_with("Kan ikke lese "), "{error}");
    }

    #[test]
    fn silence_reads_the_floor() {
        assert_eq!(loudest_dbfs(&[0.0; 4000]), LEVEL_FLOOR_DBFS);
        assert_eq!(loudest_dbfs(&[]), LEVEL_FLOOR_DBFS);
    }
}
