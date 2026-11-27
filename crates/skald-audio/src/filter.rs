//! Filters a take can go through before the model hears it: an 80 Hz high-pass and level normalisation.
//! docs/design.md, Audio in, keeps each only where `skald bench` shows it helps (M3).
//!
//! Both work on 16 kHz mono samples, full scale ±1.0, in place.

use crate::SAMPLE_RATE;
use crate::level::{LEVEL_FLOOR_DBFS, LevelMeter};

/// Below this, a microphone picks up rumble, handling noise and mains hum rather than speech: a man's
/// voice rarely goes under 85 Hz.
pub const HIGH_PASS_HZ: f32 = 80.0;

/// A second-order Butterworth high-pass, the biquad of Robert Bristow-Johnson's «Audio EQ Cookbook»:
/// −3 dB at the cutoff and 12 dB less per octave below it, flat above. It keeps its state between calls,
/// so a take filtered in pieces comes out as one filtered whole.
#[derive(Debug, Clone)]
pub struct HighPass {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
    // Transposed direct form II, in f64 so the poles near 1 at a low cutoff stay put.
    z1: f64,
    z2: f64,
}

impl HighPass {
    /// A high-pass at `cutoff_hz` for 16 kHz audio.
    pub fn new(cutoff_hz: f32) -> Self {
        let w0 = std::f64::consts::TAU * f64::from(cutoff_hz) / f64::from(SAMPLE_RATE);
        let (sin, cos) = w0.sin_cos();
        let alpha = sin / std::f64::consts::SQRT_2; // sin(w0) / (2Q) with Q = 1/√2, Butterworth
        let a0 = 1.0 + alpha;
        Self {
            b0: (1.0 + cos) / 2.0 / a0,
            b1: -(1.0 + cos) / a0,
            b2: (1.0 + cos) / 2.0 / a0,
            a1: -2.0 * cos / a0,
            a2: (1.0 - alpha) / a0,
            z1: 0.0,
            z2: 0.0,
        }
    }

    pub fn process(&mut self, samples: &mut [f32]) {
        for sample in samples {
            let x = f64::from(*sample);
            let y = self.b0 * x + self.z1;
            self.z1 = self.b1 * x - self.a1 * y + self.z2;
            self.z2 = self.b2 * x - self.a2 * y;
            *sample = y as f32;
        }
    }
}

/// Where level normalisation puts a take's loudest level reading: where read speech close to a good
/// microphone reads, so a quiet or distant take is lifted to it and a hot one is brought down.
///
/// Evidence: the 20 FLEURS clips of docs/research/engine-check.md read −17.9 to −4.2 dBFS at their
/// loudest, −13.4 in the middle; diktafon on a MacBook microphone read −33 at its loudest in a dictation.
pub const NORMALISED_LEVEL_DBFS: f32 = -14.0;

/// The highest sample normalisation leaves, so lifting a take never clips it.
pub const PEAK_CEILING_DBFS: f32 = -1.0;

/// Scales a take by one gain so its loudest level reading, as the meter measures it, reaches
/// `target_dbfs`, without lifting any sample above [`PEAK_CEILING_DBFS`]. A take of digital silence is
/// left as it is. Returns the gain in dB.
///
/// One gain for the whole take, not a gain that follows the voice: the pauses keep their level relative
/// to the speech, as they were recorded.
pub fn normalise_level(samples: &mut [f32], target_dbfs: f32) -> f32 {
    let mut meter = LevelMeter::default();
    let mut loudest = LEVEL_FLOOR_DBFS;
    meter.push(samples, |reading| loudest = loudest.max(reading));
    if let Some(reading) = meter.finish() {
        loudest = loudest.max(reading);
    }
    let peak = samples.iter().fold(0.0f32, |peak, s| peak.max(s.abs()));
    if loudest <= LEVEL_FLOOR_DBFS || peak == 0.0 {
        return 0.0;
    }
    let peak_dbfs = 20.0 * peak.log10();
    let gain_db = (target_dbfs - loudest).min(PEAK_CEILING_DBFS - peak_dbfs);
    let gain = 10f32.powf(gain_db / 20.0);
    for sample in samples.iter_mut() {
        *sample *= gain;
    }
    gain_db
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::TAU;

    /// A sine; at 500 Hz every 60 ms level reading holds whole periods, so it reads the sine's RMS exactly.
    fn sine(amplitude: f32, frequency: f32, seconds: f32) -> Vec<f32> {
        let len = (SAMPLE_RATE as f32 * seconds) as usize;
        (0..len)
            .map(|n| amplitude * (TAU * frequency * n as f32 / SAMPLE_RATE as f32).sin())
            .collect()
    }

    /// The level of a filtered sine after the filter has settled, relative to the sine's own, in dB.
    fn high_pass_gain_db(frequency: f32) -> f32 {
        let mut signal = sine(0.5, frequency, 2.0);
        HighPass::new(HIGH_PASS_HZ).process(&mut signal);
        let settled = &signal[signal.len() / 2..];
        let peak = settled.iter().fold(0.0f32, |p, s| p.max(s.abs()));
        20.0 * (peak / 0.5).log10()
    }

    fn rms_dbfs(samples: &[f32]) -> f32 {
        let mean =
            samples.iter().map(|&s| f64::from(s).powi(2)).sum::<f64>() / samples.len() as f64;
        (10.0 * mean.log10()) as f32
    }

    fn peak(samples: &[f32]) -> f32 {
        samples.iter().fold(0.0f32, |p, s| p.max(s.abs()))
    }

    #[test]
    fn high_pass_is_three_db_down_at_the_cutoff() {
        let gain = high_pass_gain_db(HIGH_PASS_HZ);
        assert!((gain - -3.01).abs() < 0.1, "{gain}");
    }

    #[test]
    fn high_pass_leaves_speech_frequencies_alone() {
        for frequency in [300.0, 1000.0, 4000.0] {
            let gain = high_pass_gain_db(frequency);
            assert!(gain.abs() < 0.1, "{frequency} Hz: {gain}");
        }
    }

    #[test]
    fn high_pass_falls_twelve_db_per_octave_below_the_cutoff() {
        // Two octaves below the cutoff a second-order Butterworth is 24 dB down, and a little more.
        let gain = high_pass_gain_db(HIGH_PASS_HZ / 4.0);
        assert!((-24.5..=-23.5).contains(&gain), "{gain}");
        assert!(high_pass_gain_db(50.0) < -7.0, "mains hum is turned down");
    }

    #[test]
    fn high_pass_removes_a_dc_offset() {
        let mut signal = vec![0.25; SAMPLE_RATE as usize];
        HighPass::new(HIGH_PASS_HZ).process(&mut signal);
        assert!(
            signal[signal.len() - 1].abs() < 1e-4,
            "{}",
            signal[signal.len() - 1]
        );
    }

    #[test]
    fn high_pass_in_pieces_equals_high_pass_in_one_go() {
        let signal: Vec<f32> = sine(0.3, 60.0, 0.5)
            .iter()
            .zip(sine(0.2, 900.0, 0.5))
            .map(|(a, b)| a + b)
            .collect();
        let mut whole = signal.clone();
        HighPass::new(HIGH_PASS_HZ).process(&mut whole);

        let mut pieces = signal;
        let mut filter = HighPass::new(HIGH_PASS_HZ);
        for chunk in pieces.chunks_mut(173) {
            filter.process(chunk);
        }
        assert_eq!(whole, pieces);
    }

    #[test]
    fn normalisation_lifts_a_quiet_take_to_the_target() {
        let mut take = sine(0.01, 500.0, 1.0);
        let gain = normalise_level(&mut take, NORMALISED_LEVEL_DBFS);
        assert!(gain > 20.0, "{gain}");
        assert!(
            (rms_dbfs(&take) - NORMALISED_LEVEL_DBFS).abs() < 0.01,
            "{}",
            rms_dbfs(&take)
        );
    }

    #[test]
    fn normalisation_brings_a_hot_take_down() {
        let mut take = sine(0.9, 500.0, 1.0);
        let gain = normalise_level(&mut take, NORMALISED_LEVEL_DBFS);
        assert!(gain < 0.0, "{gain}");
        assert!((rms_dbfs(&take) - NORMALISED_LEVEL_DBFS).abs() < 0.01);
    }

    #[test]
    fn normalisation_follows_the_loudest_reading_not_the_average() {
        // Half a second of speech-like tone after a second and a half of near silence: the tone reaches
        // the target, as it would without the pause.
        let mut take = vec![0.0f32; SAMPLE_RATE as usize * 3 / 2];
        take.extend(sine(0.02, 500.0, 0.5));
        normalise_level(&mut take, NORMALISED_LEVEL_DBFS);
        let tone = &take[take.len() - SAMPLE_RATE as usize / 4..];
        assert!(
            (rms_dbfs(tone) - NORMALISED_LEVEL_DBFS).abs() < 0.01,
            "{}",
            rms_dbfs(tone)
        );
    }

    #[test]
    fn normalisation_never_lifts_a_sample_above_the_ceiling() {
        // Quiet tone with one loud click: lifting the tone to the target would clip the click.
        let mut take = sine(0.01, 500.0, 1.0);
        take[8000] = 0.5;
        let gain = normalise_level(&mut take, NORMALISED_LEVEL_DBFS);
        let ceiling = 10f32.powf(PEAK_CEILING_DBFS / 20.0);
        assert!((peak(&take) - ceiling).abs() < 1e-4, "{}", peak(&take));
        assert!(
            (gain - (PEAK_CEILING_DBFS - 20.0 * 0.5f32.log10())).abs() < 1e-3,
            "{gain}"
        );
    }

    #[test]
    fn normalisation_leaves_digital_silence_alone() {
        let mut take = vec![0.0f32; 16_000];
        assert_eq!(normalise_level(&mut take, NORMALISED_LEVEL_DBFS), 0.0);
        assert!(take.iter().all(|&s| s == 0.0));
        assert_eq!(normalise_level(&mut [], NORMALISED_LEVEL_DBFS), 0.0);
    }
}
