//! Level readings: the RMS of each stretch of audio the overlay draws as one bar, in dBFS.

use std::time::Duration;

use crate::SAMPLE_RATE;

/// The audio one reading covers. The overlay draws the last 2.4 s as 40 bars, one per reading.
pub const LEVEL_INTERVAL: Duration = Duration::from_millis(60);

/// The lowest reading given; digital silence reads as this rather than minus infinity.
pub const LEVEL_FLOOR_DBFS: f32 = -100.0;

pub(crate) const SAMPLES_PER_READING: usize =
    (SAMPLE_RATE as u128 * LEVEL_INTERVAL.as_millis() / 1000) as usize;

/// RMS in dBFS: a full-scale square wave reads 0 and a full-scale sine about −3.
fn rms_to_dbfs(rms: f64) -> f32 {
    // A zero RMS gives minus infinity, which `max` lifts to the floor.
    ((20.0 * rms.log10()) as f32).max(LEVEL_FLOOR_DBFS)
}

/// Turns a stream of 16 kHz samples into one reading per [`LEVEL_INTERVAL`].
#[derive(Debug, Default)]
pub(crate) struct LevelMeter {
    sum_of_squares: f64,
    count: usize,
}

impl LevelMeter {
    pub(crate) fn push(&mut self, samples: &[f32], mut on_reading: impl FnMut(f32)) {
        for &sample in samples {
            self.sum_of_squares += f64::from(sample) * f64::from(sample);
            self.count += 1;
            if self.count == SAMPLES_PER_READING {
                on_reading(self.take_reading());
            }
        }
    }

    /// The reading for a last stretch shorter than [`LEVEL_INTERVAL`], if any audio is left.
    pub(crate) fn finish(&mut self) -> Option<f32> {
        (self.count > 0).then(|| self.take_reading())
    }

    fn take_reading(&mut self) -> f32 {
        let reading = rms_to_dbfs((self.sum_of_squares / self.count as f64).sqrt());
        self.sum_of_squares = 0.0;
        self.count = 0;
        reading
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::TAU;

    fn sine(amplitude: f32, frequency: f32, seconds: f32) -> Vec<f32> {
        let len = (SAMPLE_RATE as f32 * seconds) as usize;
        (0..len)
            .map(|n| amplitude * (TAU * frequency * n as f32 / SAMPLE_RATE as f32).sin())
            .collect()
    }

    fn readings(samples: &[f32]) -> Vec<f32> {
        let mut meter = LevelMeter::default();
        let mut out = Vec::new();
        meter.push(samples, |r| out.push(r));
        out.extend(meter.finish());
        out
    }

    #[test]
    fn a_reading_covers_960_samples() {
        assert_eq!(SAMPLES_PER_READING, 960);
    }

    #[test]
    fn the_overlay_window_of_two_point_four_seconds_gives_forty_readings() {
        assert_eq!(readings(&sine(0.5, 440.0, 2.4)).len(), 40);
    }

    #[test]
    fn a_full_scale_square_wave_reads_zero() {
        let square: Vec<f32> = (0..960)
            .map(|n| if n % 2 == 0 { 1.0 } else { -1.0 })
            .collect();
        assert_eq!(readings(&square), vec![0.0]);
    }

    #[test]
    fn a_sine_reads_its_rms_level() {
        // RMS of a sine with amplitude 0.5 is 0.5 / √2, which is −9.03 dBFS.
        for reading in readings(&sine(0.5, 1000.0, 1.2)) {
            assert!((reading - -9.03).abs() < 0.05, "{reading}");
        }
    }

    #[test]
    fn silence_reads_the_floor() {
        assert_eq!(readings(&[0.0; 1920]), vec![LEVEL_FLOOR_DBFS; 2]);
        assert_eq!(readings(&[]), Vec::<f32>::new());
    }

    #[test]
    fn readings_do_not_depend_on_how_the_audio_is_cut() {
        let signal = sine(0.3, 300.0, 1.0);
        let whole = readings(&signal);

        let mut meter = LevelMeter::default();
        let mut pieces = Vec::new();
        for chunk in signal.chunks(173) {
            meter.push(chunk, |r| pieces.push(r));
        }
        pieces.extend(meter.finish());
        assert_eq!(whole, pieces);
    }

    #[test]
    fn a_short_last_stretch_gives_one_more_reading() {
        let signal = sine(0.5, 440.0, 0.07);
        assert_eq!(signal.len(), 1120);
        assert_eq!(readings(&signal).len(), 2);
    }
}
