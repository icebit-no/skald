//! The level bars: one per level reading, the RMS of 60 ms of audio in dBFS. While recording they show the
//! last 2.4 s scrolling in from the right; on release they fold into the whole take (docs/design.md, Overlay).

use std::time::Duration;

/// The audio one reading covers, and so one bar.
pub const READING: Duration = Duration::from_millis(60);

/// The bars in the recording row: the last 2.4 s.
pub const LIVE_BARS: usize = 40;

pub const LOWEST_BAR: f32 = 3.0;
pub const HIGHEST_BAR: f32 = 24.0;

/// The level drawn as the lowest bar: below the room tone of the recordings in skald-core's silence gate
/// (−54 to −62 dBFS), so a quiet room stays flat.
const QUIET_DBFS: f32 = -60.0;

/// The level drawn as the highest bar: the loudest takes measured read −18 dBFS and up.
const LOUD_DBFS: f32 = -15.0;

/// How long a reading may take to arrive and still be drawn on time. The row runs this far behind the
/// first reading's clock, so a bar enters at the right edge once its reading is usually there.
const LAG: f32 = 1.0;

/// A bar's height in points for a reading in dBFS, linear in decibels.
pub fn bar_height(dbfs: f32) -> f32 {
    let share = ((dbfs - QUIET_DBFS) / (LOUD_DBFS - QUIET_DBFS)).clamp(0.0, 1.0);
    if share.is_nan() {
        return LOWEST_BAR;
    }
    LOWEST_BAR + (HIGHEST_BAR - LOWEST_BAR) * share
}

/// A take's readings as they arrive.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Levels {
    heights: Vec<f32>,
    first_at: Option<Duration>,
}

/// One bar of the recording row.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LiveBar {
    /// How many bar pitches the bar stands left of the row's right end; it enters at 0.
    pub from_right: f32,
    pub height: f32,
}

impl Levels {
    /// A reading in dBFS, arriving `at`.
    pub fn push(&mut self, dbfs: f32, at: Duration) {
        self.first_at.get_or_insert(at);
        self.heights.push(bar_height(dbfs));
    }

    /// The audio the readings cover.
    pub fn audio(&self) -> Duration {
        READING * self.heights.len() as u32
    }

    /// Which reading stands at the right end of the row at `now`, with the fraction of the way the row
    /// has scrolled towards the next. Before the first reading the row stands still.
    pub fn position(&self, now: Duration) -> f32 {
        match self.first_at {
            Some(first) => now.saturating_sub(first).as_secs_f32() / READING.as_secs_f32() - LAG,
            None => -LAG,
        }
    }

    /// The row at a position: the bars on screen, the leftmost partly out of view. A reading that has not
    /// arrived yet, and the time before the take, show as the lowest bar.
    pub fn row(&self, position: f32) -> impl Iterator<Item = LiveBar> + '_ {
        let newest = position.floor() as i64;
        (newest - LIVE_BARS as i64..=newest).map(move |index| LiveBar {
            from_right: position - index as f32,
            height: self.height(index),
        })
    }

    fn height(&self, index: i64) -> f32 {
        usize::try_from(index)
            .ok()
            .and_then(|index| self.heights.get(index))
            .copied()
            .unwrap_or(LOWEST_BAR)
    }

    /// The whole take as `slots` bars. A take with fewer readings than slots keeps its readings as they
    /// were, with the lowest bars before them, as the recording row showed it; a longer one gives each
    /// slot the loudest reading of its share of the take.
    pub fn fold(&self, slots: usize) -> Vec<f32> {
        let count = self.heights.len();
        if count <= slots {
            let mut folded = vec![LOWEST_BAR; slots - count];
            folded.extend_from_slice(&self.heights);
            return folded;
        }
        (0..slots)
            .map(|slot| {
                let share = &self.heights[slot * count / slots..(slot + 1) * count / slots];
                share.iter().copied().fold(LOWEST_BAR, f32::max)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(millis: u64) -> Duration {
        Duration::from_millis(millis)
    }

    fn levels(readings: &[f32]) -> Levels {
        let mut levels = Levels::default();
        for (i, &dbfs) in readings.iter().enumerate() {
            levels.push(dbfs, READING * i as u32);
        }
        levels
    }

    #[test]
    fn heights_run_from_quiet_to_loud() {
        assert_eq!(bar_height(-100.0), LOWEST_BAR);
        assert_eq!(bar_height(QUIET_DBFS), LOWEST_BAR);
        assert_eq!(bar_height(LOUD_DBFS), HIGHEST_BAR);
        assert_eq!(bar_height(0.0), HIGHEST_BAR);
        assert!((bar_height(-37.5) - 13.5).abs() < 1e-4);
        assert_eq!(bar_height(f32::NAN), LOWEST_BAR);
        assert_eq!(bar_height(f32::NEG_INFINITY), LOWEST_BAR);
    }

    #[test]
    fn the_readings_cover_60_ms_each() {
        assert_eq!(levels(&[-20.0; 40]).audio(), ms(2400));
        assert_eq!(Levels::default().audio(), Duration::ZERO);
    }

    #[test]
    fn the_row_holds_forty_bars_and_one_entering() {
        let row: Vec<LiveBar> = levels(&[-20.0; 100]).row(70.25).collect();
        assert_eq!(row.len(), LIVE_BARS + 1);
        assert_eq!(row.last().unwrap().from_right, 0.25);
        assert_eq!(row[0].from_right, 40.25);
    }

    #[test]
    fn the_row_scrolls_one_bar_per_reading_after_the_first_arrives() {
        let mut levels = Levels::default();
        assert_eq!(levels.position(ms(500)), -LAG);
        levels.push(-20.0, ms(1000));
        assert_eq!(levels.position(ms(1000)), -LAG);
        assert!((levels.position(ms(1090)) - (1.5 - LAG)).abs() < 1e-5);
        assert!((levels.position(ms(1000) + READING * 10) - (10.0 - LAG)).abs() < 1e-4);
    }

    #[test]
    fn a_reading_enters_at_the_right_end_once_it_has_arrived() {
        let mut levels = Levels::default();
        levels.push(LOUD_DBFS, ms(0));
        levels.push(LOUD_DBFS, ms(60));
        let at = |position: f32, index: usize| {
            levels
                .row(position)
                .find(|bar| (position - bar.from_right - index as f32).abs() < 1e-5)
                .map(|bar| bar.height)
        };
        assert_eq!(at(1.0, 1), Some(HIGHEST_BAR));
        assert_eq!(at(1.0, 2), None, "the third reading is not in the row yet");
        assert_eq!(
            at(2.0, 2),
            Some(LOWEST_BAR),
            "it is late, so its bar is low"
        );
    }

    #[test]
    fn the_time_before_the_take_is_quiet() {
        let row: Vec<f32> = levels(&[LOUD_DBFS; 3])
            .row(2.0)
            .map(|bar| bar.height)
            .collect();
        assert_eq!(row[..LIVE_BARS - 2], [LOWEST_BAR; LIVE_BARS - 2]);
        assert_eq!(row[LIVE_BARS - 2..], [HIGHEST_BAR; 3]);
    }

    #[test]
    fn a_short_take_folds_into_the_bars_it_had() {
        let take = levels(&[LOUD_DBFS, -100.0, LOUD_DBFS]);
        let folded = take.fold(LIVE_BARS);
        assert_eq!(folded.len(), LIVE_BARS);
        assert_eq!(folded[..LIVE_BARS - 3], [LOWEST_BAR; LIVE_BARS - 3]);
        assert_eq!(
            folded[LIVE_BARS - 3..],
            [HIGHEST_BAR, LOWEST_BAR, HIGHEST_BAR]
        );
    }

    #[test]
    fn a_long_take_keeps_the_loudest_reading_of_each_share() {
        let mut readings = vec![-100.0; 120];
        readings[0] = LOUD_DBFS;
        readings[119] = -37.5;
        readings[61] = LOUD_DBFS;
        let folded = levels(&readings).fold(LIVE_BARS);
        assert_eq!(folded.len(), LIVE_BARS);
        assert_eq!(folded[0], HIGHEST_BAR);
        assert_eq!(folded[20], HIGHEST_BAR, "readings 60 to 62");
        assert!((folded[39] - 13.5).abs() < 1e-4);
        assert_eq!(folded[1], LOWEST_BAR);
    }

    #[test]
    fn every_reading_of_an_uneven_take_falls_in_one_slot() {
        let readings: Vec<f32> = (0..97).map(|i| -60.0 + i as f32 * 0.4).collect();
        let folded = levels(&readings).fold(LIVE_BARS);
        assert_eq!(folded.len(), LIVE_BARS);
        assert!(folded.windows(2).all(|pair| pair[0] <= pair[1]));
        assert_eq!(*folded.last().unwrap(), bar_height(-60.0 + 96.0 * 0.4));
    }
}
