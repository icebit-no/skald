//! One sitting at the recorder: the sentences still to record, shown one at a time and each recorded while
//! Enter is held, with the last take redone or a sentence skipped on a key. It reaches the keys, the
//! microphone and the screen through the traits here, so the tests run it without a terminal or a
//! microphone.
//!
//! The sentences come in this order: those after the last one recorded, so a sitting picks up where the
//! one before stopped, then those skipped before it.

use std::collections::BTreeSet;
use std::io::{self, Write};
use std::time::{Duration, Instant};

use skald_audio::SAMPLE_RATE;
use skald_core::silence_gate::SPEECH_LEVEL_DBFS;

use super::folder::Folder;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Key {
    /// Enter went down.
    Enter,
    EnterReleased,
    /// Record the last take again.
    Redo,
    Skip,
    Quit,
}

pub(super) trait Keys {
    /// The next key within `timeout`, or `None` when none came.
    fn next(&mut self, timeout: Duration) -> io::Result<Option<Key>>;
}

/// What ends a take.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TakeEnd {
    /// Letting go of Enter, for a terminal that reports it.
    Release,
    /// Pressing Enter again.
    SecondPress,
}

pub(super) trait Capture {
    type Take: Running;
    /// Opens the microphone and starts a take.
    fn start(&mut self) -> Result<Self::Take, String>;
}

pub(super) trait Running {
    /// What the microphone reported since the last call.
    fn heard(&mut self) -> Vec<Heard>;
    /// Closes the microphone.
    fn stop(self) -> Captured;
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum Heard {
    /// A level reading in dBFS.
    Level(f32),
    /// The take reached the longest a take may be, and keeps what it has.
    Capped,
    /// The microphone stopped delivering audio.
    Lost,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct Captured {
    /// Mono at 16 kHz; full scale is ±1.0.
    pub samples: Vec<f32>,
    pub loudest_dbfs: f32,
}

/// A take shorter than this is a slip of the key, and is not kept.
const SHORTEST_TAKE: Duration = Duration::from_millis(500);

/// How often the line showing a running take is drawn again.
const REDRAW: Duration = Duration::from_millis(50);

/// How long a wait for a key lasts before it is started again; nothing happens in between.
const IDLE: Duration = Duration::from_secs(1);

/// The level an empty meter shows.
const METER_FLOOR_DBFS: f32 = -60.0;
const METER_CELLS: usize = 20;

/// Clears the line from the cursor to its end.
const CLEAR_LINE: &str = "\x1b[K";

pub(super) struct Session<'a, K, C, W> {
    folder: &'a Folder,
    sentences: &'a [String],
    keys: K,
    capture: C,
    take_end: TakeEnd,
    screen: W,
    manifest_ready: bool,
}

enum Taken {
    Kept,
    Discarded,
    Quit,
}

impl<'a, K: Keys, C: Capture, W: Write> Session<'a, K, C, W> {
    pub(super) fn new(
        folder: &'a Folder,
        sentences: &'a [String],
        keys: K,
        capture: C,
        take_end: TakeEnd,
        screen: W,
    ) -> Self {
        Self {
            folder,
            sentences,
            keys,
            capture,
            take_end,
            screen,
            manifest_ready: false,
        }
    }

    /// Runs until every sentence has been offered or the user quits. Fails when the microphone, the
    /// folder, the keys or the screen do; what was recorded until then stays in the folder.
    pub(super) fn run(&mut self) -> Result<(), String> {
        let count = self.sentences.len();
        let recorded = self.folder.recorded(count);
        let queue = queue(count, &recorded);
        self.intro(recorded.len(), queue.is_empty())?;
        let mut last = recorded.last().copied();
        let mut next = 0;
        let mut again = None;
        let mut shown = None;
        while let Some(number) = again.or_else(|| queue.get(next).copied()) {
            if shown != Some(number) {
                self.show(number, again.is_some())?;
                shown = Some(number);
            }
            match self.key(IDLE)? {
                Some(Key::Enter) => match self.take(number)? {
                    Taken::Kept => {
                        last = Some(number);
                        if again.take().is_none() {
                            next += 1;
                        }
                    }
                    Taken::Discarded => {}
                    Taken::Quit => return Ok(()),
                },
                Some(Key::Redo) => match (again, last) {
                    (Some(_), _) => {}
                    (None, Some(previous)) => again = Some(previous),
                    (None, None) => self.line("Det er ikke noe opptak å ta på nytt ennå.")?,
                },
                Some(Key::Skip) => match again.take() {
                    Some(previous) => {
                        self.line(&format!("Beholder opptaket av setning {previous}."))?;
                    }
                    None => {
                        self.line(&format!("Hopper over setning {number}."))?;
                        next += 1;
                    }
                },
                Some(Key::Quit) => return Ok(()),
                Some(Key::EnterReleased) | None => {}
            }
        }
        Ok(())
    }

    fn intro(&mut self, recorded: usize, all_recorded: bool) -> Result<(), String> {
        let count = self.sentences.len();
        if all_recorded {
            return self.line(&format!("Alle {count} setningene er tatt opp."));
        }
        if recorded > 0 {
            self.line(&format!("{recorded} av {count} setninger er tatt opp."))?;
        }
        self.line(match self.take_end {
            TakeEnd::Release => "Hold Enter mens du leser setningen, og slipp når du er ferdig.",
            TakeEnd::SecondPress => {
                "Trykk Enter, les setningen, og trykk Enter igjen når du er ferdig."
            }
        })?;
        // The microphone takes a moment to open, and a word said before it has is lost.
        self.line("Begynn å lese når «● Tar opp» står på skjermen.")?;
        self.line("n: ta opp forrige på nytt   h: hopp over   q: avslutt")
    }

    fn show(&mut self, number: usize, again: bool) -> Result<(), String> {
        let count = self.sentences.len();
        let again = if again { ", på nytt" } else { "" };
        self.line("")?;
        self.line(&format!("Setning {number} av {count}{again}:"))?;
        self.line(&format!("    {}", self.sentences[number - 1]))
    }

    /// Records one take of the sentence and keeps it, unless it is too short or the user quits during it.
    fn take(&mut self, number: usize) -> Result<Taken, String> {
        let mut take = self
            .capture
            .start()
            .map_err(|reason| format!("Kunne ikke starte opptaket: {reason}."))?;
        let started = Instant::now();
        let mut level = METER_FLOOR_DBFS;
        'take: loop {
            for heard in take.heard() {
                match heard {
                    Heard::Level(reading) => level = reading,
                    Heard::Capped => break 'take,
                    Heard::Lost => {
                        drop(take.stop());
                        self.status("")?;
                        return Err(
                            "Mikrofonen forsvant under opptaket, som ikke ble lagret.".into()
                        );
                    }
                }
            }
            self.status(&meter(started.elapsed(), level))?;
            match (self.key(REDRAW)?, self.take_end) {
                (Some(Key::EnterReleased), TakeEnd::Release)
                | (Some(Key::Enter), TakeEnd::SecondPress) => break,
                (Some(Key::Quit), _) => {
                    drop(take.stop());
                    self.line("Opptaket ble ikke lagret.")?;
                    return Ok(Taken::Quit);
                }
                _ => {}
            }
        }

        let captured = take.stop();
        let length =
            Duration::from_secs_f64(captured.samples.len() as f64 / f64::from(SAMPLE_RATE));
        if length < SHORTEST_TAKE {
            self.line(&format!(
                "For kort til å lagres ({}). Prøv igjen.",
                seconds(length)
            ))?;
            return Ok(Taken::Discarded);
        }
        self.folder
            .write(number, &self.sentences[number - 1], &captured.samples)?;
        if !self.manifest_ready {
            self.folder.ensure_manifest()?;
            self.manifest_ready = true;
        }
        let name = self.folder.name(number);
        self.line(&format!("Lagret som {name}.wav ({}).", seconds(length)))?;
        if captured.loudest_dbfs < SPEECH_LEVEL_DBFS {
            self.line(&format!(
                "Nesten stille (sterkeste {} dBFS), så Skald ville tatt det som stillhet. Er det riktig \
                 mikrofon? n tar det opp på nytt.",
                decibels(captured.loudest_dbfs)
            ))?;
        }
        Ok(Taken::Kept)
    }

    fn key(&mut self, timeout: Duration) -> Result<Option<Key>, String> {
        self.keys
            .next(timeout)
            .map_err(|e| format!("Kunne ikke lese tastene: {e}"))
    }

    /// Writes a line of its own over whatever the current line shows. A terminal in raw mode neither
    /// returns to the line's start nor moves down by itself.
    fn line(&mut self, text: &str) -> Result<(), String> {
        self.write(&format!("\r{CLEAR_LINE}{text}\r\n"))
    }

    /// Shows the text on the current line, for the next line or status to replace.
    fn status(&mut self, text: &str) -> Result<(), String> {
        self.write(&format!("\r{CLEAR_LINE}{text}"))
    }

    fn write(&mut self, text: &str) -> Result<(), String> {
        self.screen
            .write_all(text.as_bytes())
            .and_then(|()| self.screen.flush())
            .map_err(|e| format!("Kunne ikke skrive til terminalen: {e}"))
    }
}

/// The sentences still to record, in the order they are offered.
fn queue(count: usize, recorded: &BTreeSet<usize>) -> Vec<usize> {
    let last = recorded.last().copied().unwrap_or(0);
    let (before, after): (Vec<usize>, Vec<usize>) = (1..=count)
        .filter(|number| !recorded.contains(number))
        .partition(|&number| number < last);
    after.into_iter().chain(before).collect()
}

fn meter(elapsed: Duration, level_dbfs: f32) -> String {
    let share = ((level_dbfs - METER_FLOOR_DBFS) / -METER_FLOOR_DBFS).clamp(0.0, 1.0);
    let lit = (share * METER_CELLS as f32).round() as usize;
    format!(
        "● Tar opp  {}  {}{}",
        seconds(elapsed),
        "█".repeat(lit),
        "░".repeat(METER_CELLS - lit)
    )
}

/// Seconds with one decimal and a decimal comma, as in «4,2 s».
fn seconds(length: Duration) -> String {
    format!("{:.1} s", length.as_secs_f64()).replace('.', ",")
}

/// Whole decibels with a minus sign, as in «−52».
fn decibels(dbfs: f32) -> String {
    format!("{dbfs:.0}").replace('-', "\u{2212}")
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::f32::consts::TAU;

    use super::super::Label;
    use super::super::tests::Scratch;
    use super::*;
    use crate::set;

    /// Keys pressed in order; `None` is a wait in which no key came. A session that waits for more keys
    /// than the script has fails.
    struct Script(VecDeque<Option<Key>>);

    impl Keys for Script {
        fn next(&mut self, _timeout: Duration) -> io::Result<Option<Key>> {
            self.0
                .pop_front()
                .ok_or_else(|| io::Error::other("the script has no more keys"))
        }
    }

    /// One take the fake microphone gives.
    #[derive(Clone)]
    struct Planned {
        seconds: f32,
        amplitude: f32,
        heard: Vec<Heard>,
    }

    fn spoken(seconds: f32, amplitude: f32) -> Planned {
        Planned {
            seconds,
            amplitude,
            heard: vec![Heard::Level(-20.0)],
        }
    }

    /// A microphone that gives the planned takes in order, and fails to start once they run out.
    struct Microphone {
        takes: VecDeque<Planned>,
        started: usize,
    }

    impl Capture for &mut Microphone {
        type Take = Planned;

        fn start(&mut self) -> Result<Planned, String> {
            self.started += 1;
            self.takes
                .pop_front()
                .ok_or_else(|| "no more takes are planned".to_owned())
        }
    }

    impl Running for Planned {
        fn heard(&mut self) -> Vec<Heard> {
            std::mem::take(&mut self.heard)
        }

        fn stop(self) -> Captured {
            let len = (self.seconds * SAMPLE_RATE as f32) as usize;
            let samples = (0..len)
                .map(|n| self.amplitude * (TAU * 440.0 * n as f32 / SAMPLE_RATE as f32).sin())
                .collect();
            let loudest_dbfs = 20.0 * (self.amplitude / 2f32.sqrt()).log10();
            Captured {
                samples,
                loudest_dbfs,
            }
        }
    }

    struct Sitting {
        result: Result<(), String>,
        screen: String,
        started: usize,
    }

    fn sentences(count: usize) -> Vec<String> {
        (1..=count)
            .map(|n| format!("Setning nummer {n}."))
            .collect()
    }

    fn sit(
        folder: &Folder,
        count: usize,
        take_end: TakeEnd,
        keys: &[Option<Key>],
        takes: &[Planned],
    ) -> Sitting {
        let sentences = sentences(count);
        let mut microphone = Microphone {
            takes: takes.iter().cloned().collect(),
            started: 0,
        };
        let mut screen = Vec::new();
        let result = Session::new(
            folder,
            &sentences,
            Script(keys.iter().copied().collect()),
            &mut microphone,
            take_end,
            &mut screen,
        )
        .run();
        Sitting {
            result,
            screen: String::from_utf8(screen).unwrap(),
            started: microphone.started,
        }
    }

    /// One take held for a moment and let go.
    const HOLD: [Option<Key>; 3] = [Some(Key::Enter), None, Some(Key::EnterReleased)];

    fn keys(parts: &[&[Option<Key>]]) -> Vec<Option<Key>> {
        parts.concat()
    }

    /// The peak of a written take, to tell the takes apart.
    fn peak(folder: &Folder, number: usize) -> f32 {
        let wav = folder.path().join(format!("{}.wav", folder.name(number)));
        crate::audio::read_wav(&wav)
            .unwrap()
            .iter()
            .fold(0.0, |peak, s| f32::max(peak, s.abs()))
    }

    #[test]
    fn each_sentence_is_recorded_in_turn_into_a_test_set() {
        let scratch = Scratch::new("in-turn");
        let folder = Folder::new(scratch.path().to_owned(), Label::Usb, 3);
        let sitting = sit(
            &folder,
            3,
            TakeEnd::Release,
            &keys(&[&HOLD, &HOLD, &HOLD]),
            &[spoken(1.0, 0.3), spoken(1.0, 0.3), spoken(1.0, 0.3)],
        );
        sitting.result.unwrap();
        assert_eq!(folder.recorded(3), BTreeSet::from([1, 2, 3]));
        let clips = set::load(scratch.path()).unwrap();
        assert_eq!(clips.len(), 3);
        assert_eq!(clips[1].name, "usb-02");
        assert_eq!(clips[1].reference, "Setning nummer 2.");
        assert!(clips.iter().all(|c| c.microphone.as_deref() == Some("usb")));
        assert!(sitting.screen.contains("Hold Enter mens du leser"));
        assert!(
            sitting
                .screen
                .contains("Begynn å lese når «● Tar opp» står på skjermen.")
        );
        assert!(sitting.screen.contains("Setning 3 av 3:\r\n"));
        assert!(sitting.screen.contains("Lagret som usb-03.wav (1,0 s)."));
    }

    #[test]
    fn a_second_press_ends_the_take_where_the_release_is_not_reported() {
        let scratch = Scratch::new("second-press");
        let folder = Folder::new(scratch.path().to_owned(), Label::BuiltIn, 2);
        let press = [
            Some(Key::Enter),
            None,
            Some(Key::EnterReleased),
            Some(Key::Enter),
        ];
        let sitting = sit(
            &folder,
            2,
            TakeEnd::SecondPress,
            &keys(&[&press, &[Some(Key::Quit)]]),
            &[spoken(1.0, 0.3)],
        );
        sitting.result.unwrap();
        assert_eq!(folder.recorded(2), BTreeSet::from([1]));
        assert!(
            sitting
                .screen
                .contains("trykk Enter igjen når du er ferdig")
        );
    }

    #[test]
    fn a_new_sitting_picks_up_after_the_last_take_and_ends_with_the_skipped() {
        let scratch = Scratch::new("resume");
        let folder = Folder::new(scratch.path().to_owned(), Label::Usb, 5);
        for number in [1, 3] {
            folder
                .write(number, "Tatt opp før.", &[0.1; 16_000])
                .unwrap();
        }
        let sitting = sit(
            &folder,
            5,
            TakeEnd::Release,
            &keys(&[&HOLD, &HOLD, &[Some(Key::Quit)]]),
            &[spoken(1.0, 0.3), spoken(1.0, 0.3)],
        );
        sitting.result.unwrap();
        assert_eq!(folder.recorded(5), BTreeSet::from([1, 3, 4, 5]));
        assert!(sitting.screen.contains("2 av 5 setninger er tatt opp."));
        let order: Vec<usize> = [4, 5, 2]
            .iter()
            .map(|n| sitting.screen.find(&format!("Setning {n} av 5:")).unwrap())
            .collect();
        assert!(order.is_sorted(), "{}", sitting.screen);
        assert!(!sitting.screen.contains("Setning 1 av 5:"));
    }

    #[test]
    fn redo_records_the_last_take_again_and_goes_on_where_it_was() {
        let scratch = Scratch::new("redo");
        let folder = Folder::new(scratch.path().to_owned(), Label::Usb, 3);
        let sitting = sit(
            &folder,
            3,
            TakeEnd::Release,
            &keys(&[&HOLD, &[Some(Key::Redo)], &HOLD, &HOLD, &[Some(Key::Quit)]]),
            &[spoken(1.0, 0.2), spoken(1.0, 0.6), spoken(1.0, 0.4)],
        );
        sitting.result.unwrap();
        assert_eq!(folder.recorded(3), BTreeSet::from([1, 2]));
        assert!((peak(&folder, 1) - 0.6).abs() < 0.01);
        assert!((peak(&folder, 2) - 0.4).abs() < 0.01);
        assert!(sitting.screen.contains("Setning 1 av 3, på nytt:"));
    }

    #[test]
    fn redo_reaches_the_last_take_of_the_sitting_before() {
        let scratch = Scratch::new("redo-resumed");
        let folder = Folder::new(scratch.path().to_owned(), Label::Distance, 3);
        for number in [1, 2] {
            folder
                .write(number, "Tatt opp før.", &[0.1; 16_000])
                .unwrap();
        }
        let sitting = sit(
            &folder,
            3,
            TakeEnd::Release,
            &keys(&[&[Some(Key::Redo)], &HOLD, &HOLD]),
            &[spoken(1.0, 0.5), spoken(1.0, 0.3)],
        );
        sitting.result.unwrap();
        assert!((peak(&folder, 1) - 0.1).abs() < 0.01);
        assert!((peak(&folder, 2) - 0.5).abs() < 0.01);
        assert!((peak(&folder, 3) - 0.3).abs() < 0.01);
    }

    #[test]
    fn redo_before_any_take_says_so() {
        let scratch = Scratch::new("redo-nothing");
        let folder = Folder::new(scratch.path().to_owned(), Label::Usb, 2);
        let sitting = sit(
            &folder,
            2,
            TakeEnd::Release,
            &[Some(Key::Redo), Some(Key::Quit)],
            &[],
        );
        sitting.result.unwrap();
        assert!(
            sitting
                .screen
                .contains("Det er ikke noe opptak å ta på nytt ennå.")
        );
        assert_eq!(sitting.started, 0);
    }

    #[test]
    fn skip_moves_on_and_skip_during_a_redo_keeps_the_take() {
        let scratch = Scratch::new("skip");
        let folder = Folder::new(scratch.path().to_owned(), Label::Usb, 3);
        let sitting = sit(
            &folder,
            3,
            TakeEnd::Release,
            &keys(&[
                &[Some(Key::Skip)],
                &HOLD,
                &[Some(Key::Redo), Some(Key::Skip)],
                &HOLD,
            ]),
            &[spoken(1.0, 0.2), spoken(1.0, 0.3)],
        );
        sitting.result.unwrap();
        assert_eq!(folder.recorded(3), BTreeSet::from([2, 3]));
        assert!((peak(&folder, 2) - 0.2).abs() < 0.01);
        assert!(sitting.screen.contains("Hopper over setning 1."));
        assert!(sitting.screen.contains("Beholder opptaket av setning 2."));
    }

    #[test]
    fn a_take_too_short_is_not_kept_and_the_sentence_stays() {
        let scratch = Scratch::new("short");
        let folder = Folder::new(scratch.path().to_owned(), Label::Usb, 1);
        let sitting = sit(
            &folder,
            1,
            TakeEnd::Release,
            &keys(&[&HOLD, &HOLD]),
            &[spoken(0.2, 0.3), spoken(1.0, 0.3)],
        );
        sitting.result.unwrap();
        assert!(
            sitting
                .screen
                .contains("For kort til å lagres (0,2 s). Prøv igjen.")
        );
        assert_eq!(folder.recorded(1), BTreeSet::from([1]));
        assert_eq!(sitting.started, 2);
    }

    #[test]
    fn a_quiet_take_is_kept_with_a_warning() {
        let scratch = Scratch::new("quiet");
        let folder = Folder::new(scratch.path().to_owned(), Label::Distance, 1);
        let sitting = sit(&folder, 1, TakeEnd::Release, &HOLD, &[spoken(1.0, 0.001)]);
        sitting.result.unwrap();
        assert_eq!(folder.recorded(1), BTreeSet::from([1]));
        assert!(
            sitting
                .screen
                .contains("Nesten stille (sterkeste \u{2212}63 dBFS)"),
            "{}",
            sitting.screen
        );
    }

    #[test]
    fn quitting_during_a_take_keeps_nothing() {
        let scratch = Scratch::new("quit");
        let folder = Folder::new(scratch.path().to_owned(), Label::Usb, 2);
        let sitting = sit(
            &folder,
            2,
            TakeEnd::Release,
            &[Some(Key::Enter), None, Some(Key::Quit)],
            &[spoken(1.0, 0.3)],
        );
        sitting.result.unwrap();
        assert!(folder.recorded(2).is_empty());
        assert!(!scratch.path().join(set::MANIFEST).exists());
    }

    #[test]
    fn a_lost_microphone_ends_the_sitting_without_the_take() {
        let scratch = Scratch::new("lost");
        let folder = Folder::new(scratch.path().to_owned(), Label::Usb, 2);
        let lost = Planned {
            heard: vec![Heard::Level(-20.0), Heard::Lost],
            ..spoken(1.0, 0.3)
        };
        let sitting = sit(&folder, 2, TakeEnd::Release, &HOLD[..1], &[lost]);
        assert_eq!(
            sitting.result.unwrap_err(),
            "Mikrofonen forsvant under opptaket, som ikke ble lagret."
        );
        assert!(folder.recorded(2).is_empty());
    }

    #[test]
    fn a_take_at_the_cap_is_kept() {
        let scratch = Scratch::new("cap");
        let folder = Folder::new(scratch.path().to_owned(), Label::Usb, 1);
        let capped = Planned {
            heard: vec![Heard::Capped],
            ..spoken(2.0, 0.3)
        };
        let sitting = sit(&folder, 1, TakeEnd::Release, &HOLD[..1], &[capped]);
        sitting.result.unwrap();
        assert_eq!(folder.recorded(1), BTreeSet::from([1]));
    }

    #[test]
    fn a_microphone_that_does_not_start_ends_the_sitting() {
        let scratch = Scratch::new("no-start");
        let folder = Folder::new(scratch.path().to_owned(), Label::Usb, 1);
        let sitting = sit(&folder, 1, TakeEnd::Release, &HOLD[..1], &[]);
        assert_eq!(
            sitting.result.unwrap_err(),
            "Kunne ikke starte opptaket: no more takes are planned."
        );
    }

    #[test]
    fn a_finished_set_says_so_and_records_nothing() {
        let scratch = Scratch::new("finished");
        let folder = Folder::new(scratch.path().to_owned(), Label::Usb, 2);
        for number in [1, 2] {
            folder
                .write(number, "Tatt opp før.", &[0.1; 16_000])
                .unwrap();
        }
        let sitting = sit(&folder, 2, TakeEnd::Release, &[], &[]);
        sitting.result.unwrap();
        assert!(sitting.screen.contains("Alle 2 setningene er tatt opp."));
    }

    #[test]
    fn the_queue_starts_after_the_last_take() {
        assert_eq!(queue(4, &BTreeSet::new()), [1, 2, 3, 4]);
        assert_eq!(queue(6, &BTreeSet::from([1, 2, 4])), [5, 6, 3]);
        assert_eq!(queue(3, &BTreeSet::from([3])), [1, 2]);
        assert!(queue(2, &BTreeSet::from([1, 2])).is_empty());
    }

    #[test]
    fn the_meter_fills_with_the_level() {
        assert!(meter(Duration::from_millis(2340), -60.0).ends_with(&"░".repeat(20)));
        assert!(meter(Duration::ZERO, 0.0).ends_with(&"█".repeat(20)));
        assert!(meter(Duration::from_millis(2340), -30.0).starts_with("● Tar opp  2,3 s  "));
    }
}
