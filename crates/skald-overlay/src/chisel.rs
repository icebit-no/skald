//! How fast the chisel crosses the take in Rister: paced by this machine's measured seconds of work per
//! second of audio, so it reaches the end about when the text arrives (docs/design.md, Overlay).

use std::collections::VecDeque;
use std::time::Duration;

/// Seconds of work per second of audio, from the latest takes on this machine.
#[derive(Clone, Debug)]
pub struct Pace {
    ratios: VecDeque<f64>,
}

/// How many takes the pace is kept from.
const KEPT_TAKES: usize = 8;

/// The pace before any take has been measured: NB-Whisper Large on an M4 Pro took 1.1 s for 9 s of
/// speech (docs/research/engine-check.md). A slower machine's first take lets the chisel wait at 96 %.
const FIRST_GUESS: f64 = 0.12;

impl Default for Pace {
    fn default() -> Self {
        Self {
            ratios: VecDeque::with_capacity(KEPT_TAKES),
        }
    }
}

impl Pace {
    /// Keeps one finished take: `work` from the release of the key to the text.
    pub fn record(&mut self, audio: Duration, work: Duration) {
        if audio.is_zero() {
            return;
        }
        if self.ratios.len() == KEPT_TAKES {
            self.ratios.pop_front();
        }
        self.ratios
            .push_back(work.as_secs_f64() / audio.as_secs_f64());
    }

    /// The median of the kept takes, so one take slowed by something else does not move the chisel.
    pub fn seconds_per_second(&self) -> f64 {
        if self.ratios.is_empty() {
            return FIRST_GUESS;
        }
        let mut sorted: Vec<f64> = self.ratios.iter().copied().collect();
        sorted.sort_by(f64::total_cmp);
        let middle = sorted.len() / 2;
        if sorted.len().is_multiple_of(2) {
            (sorted[middle - 1] + sorted[middle]) / 2.0
        } else {
            sorted[middle]
        }
    }

    pub fn expected_work(&self, audio: Duration) -> Duration {
        audio.mul_f64(self.seconds_per_second())
    }
}

/// Where the chisel's even run ends and it starts slowing down.
pub const SLOWS_FROM: f32 = 0.96;

/// How long the chisel takes from wherever it is to the end once the text has arrived.
pub const RUN_OUT: Duration = Duration::from_millis(150);

/// The highest the chisel creeps to while it waits for the text: it slows down instead of stopping, and
/// never reaches the end without the text.
const CREEPS_TO: f32 = 0.995;

/// How quickly the creep past 96 % fades, as a share of the expected time.
const CREEP_FALLOFF: f32 = 0.8;

/// One carving: the chisel's position over time, from the moment it starts moving.
#[derive(Clone, Debug, PartialEq)]
pub struct Chisel {
    expected: Duration,
    text_at: Option<Duration>,
}

impl Chisel {
    /// A chisel that should reach the end after `expected`.
    pub fn new(expected: Duration) -> Self {
        Self {
            expected: expected.max(Duration::from_millis(1)),
            text_at: None,
        }
    }

    /// The text arrived `at` after the chisel started; an arrival before it started counts as at its start.
    pub fn text_arrived(&mut self, at: Duration) {
        self.text_at.get_or_insert(at);
    }

    /// When the chisel reaches the end, once the text has arrived.
    pub fn done_at(&self) -> Option<Duration> {
        self.text_at.map(|at| at + RUN_OUT)
    }

    /// How far across the take the chisel is after `elapsed`, from 0 to 1.
    pub fn progress(&self, elapsed: Duration) -> f32 {
        match self.text_at {
            Some(at) if elapsed >= at => {
                let from = self.unhurried(at);
                let run = (elapsed - at).as_secs_f32() / RUN_OUT.as_secs_f32();
                from + (1.0 - from) * ease_out(run.min(1.0))
            }
            _ => self.unhurried(elapsed),
        }
    }

    /// Even to 96 % of the expected time, then ever slower towards [`CREEPS_TO`].
    fn unhurried(&self, elapsed: Duration) -> f32 {
        let expected = self.expected.as_secs_f32();
        let share = elapsed.as_secs_f32() / expected;
        if share <= SLOWS_FROM {
            return share;
        }
        let past = (share - SLOWS_FROM) / CREEP_FALLOFF;
        SLOWS_FROM + (CREEPS_TO - SLOWS_FROM) * (1.0 - (-past).exp())
    }
}

fn ease_out(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(3)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(millis: u64) -> Duration {
        Duration::from_millis(millis)
    }

    #[test]
    fn before_any_take_the_pace_is_the_first_guess() {
        let pace = Pace::default();
        assert_eq!(pace.seconds_per_second(), FIRST_GUESS);
        let work = pace.expected_work(Duration::from_secs(9));
        assert!(
            work.abs_diff(ms(1080)) < Duration::from_micros(1),
            "{work:?}"
        );
    }

    #[test]
    fn the_pace_is_the_median_of_the_latest_takes() {
        let mut pace = Pace::default();
        pace.record(Duration::from_secs(10), Duration::from_secs(2));
        pace.record(Duration::from_secs(4), Duration::from_secs(1));
        pace.record(Duration::from_secs(5), Duration::from_secs(5));
        assert_eq!(pace.seconds_per_second(), 0.25);
        pace.record(Duration::from_secs(10), Duration::from_secs(3));
        assert!((pace.seconds_per_second() - 0.275).abs() < 1e-12);
    }

    #[test]
    fn only_the_latest_eight_takes_count() {
        let mut pace = Pace::default();
        for _ in 0..8 {
            pace.record(Duration::from_secs(1), Duration::from_secs(1));
        }
        for _ in 0..8 {
            pace.record(Duration::from_secs(10), Duration::from_secs(1));
        }
        assert!((pace.seconds_per_second() - 0.1).abs() < 1e-12);
    }

    #[test]
    fn a_take_without_audio_is_not_kept() {
        let mut pace = Pace::default();
        pace.record(Duration::ZERO, Duration::from_secs(1));
        assert_eq!(pace.seconds_per_second(), FIRST_GUESS);
    }

    #[test]
    fn the_chisel_moves_evenly_to_96_percent() {
        let chisel = Chisel::new(ms(1000));
        assert_eq!(chisel.progress(Duration::ZERO), 0.0);
        assert!((chisel.progress(ms(500)) - 0.5).abs() < 1e-6);
        assert!((chisel.progress(ms(960)) - 0.96).abs() < 1e-6);
    }

    #[test]
    fn past_96_percent_it_slows_down_but_never_stops_or_ends() {
        let chisel = Chisel::new(ms(1000));
        let mut last = chisel.progress(ms(960));
        // Three times the expected wait; past that the steps fall below what an f32 tells apart.
        for step in 1..=60 {
            let now = chisel.progress(ms(960 + step * 50));
            assert!(now > last, "it stopped at {now}");
            assert!(now < CREEPS_TO, "it reached the end at {now}");
            assert!(
                now - last < 0.05 * 0.05 / CREEP_FALLOFF,
                "it hurried at {now}"
            );
            last = now;
        }
        assert!(chisel.progress(Duration::from_secs(3600)) < 1.0);
    }

    #[test]
    fn the_slowing_starts_without_a_jump() {
        let chisel = Chisel::new(ms(1000));
        let before = chisel.progress(Duration::from_micros(959_999));
        let after = chisel.progress(Duration::from_micros(960_001));
        assert!((after - before).abs() < 1e-5);
    }

    #[test]
    fn late_text_lets_the_chisel_run_out_in_150_ms() {
        let mut chisel = Chisel::new(ms(1000));
        chisel.text_arrived(ms(1500));
        let waiting = chisel.progress(ms(1500));
        assert!(waiting > SLOWS_FROM && waiting < 1.0);
        assert!(chisel.progress(ms(1575)) > waiting);
        assert_eq!(chisel.progress(ms(1650)), 1.0);
        assert_eq!(chisel.progress(ms(4000)), 1.0);
        assert_eq!(chisel.done_at(), Some(ms(1650)));
    }

    #[test]
    fn early_text_lets_the_chisel_run_to_the_end_in_150_ms() {
        let mut chisel = Chisel::new(ms(2000));
        chisel.text_arrived(ms(600));
        assert!((chisel.progress(ms(600)) - 0.3).abs() < 1e-6);
        let halfway = chisel.progress(ms(675));
        assert!(halfway > 0.3 && halfway < 1.0);
        assert_eq!(chisel.progress(ms(750)), 1.0);
    }

    #[test]
    fn text_before_the_chisel_moves_runs_it_out_from_the_start() {
        let mut chisel = Chisel::new(ms(800));
        chisel.text_arrived(Duration::ZERO);
        assert_eq!(chisel.progress(Duration::ZERO), 0.0);
        assert_eq!(chisel.progress(ms(150)), 1.0);
    }

    #[test]
    fn the_first_arrival_counts() {
        let mut chisel = Chisel::new(ms(1000));
        chisel.text_arrived(ms(400));
        chisel.text_arrived(ms(900));
        assert_eq!(chisel.done_at(), Some(ms(550)));
    }

    #[test]
    fn the_chisel_never_goes_back() {
        for text_at in [None, Some(ms(300)), Some(ms(1200)), Some(ms(5000))] {
            let mut chisel = Chisel::new(ms(1000));
            if let Some(at) = text_at {
                chisel.text_arrived(at);
            }
            let mut last = 0.0;
            for step in 0..400 {
                let now = chisel.progress(ms(step * 16));
                assert!(now >= last, "{text_at:?}: back from {last} to {now}");
                assert!((0.0..=1.0).contains(&now));
                last = now;
            }
        }
    }
}
