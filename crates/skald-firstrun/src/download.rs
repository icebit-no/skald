//! Step one's download: its state as the app's download reports it, the speed and time left measured from
//! its progress, and what the step shows for each state.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use skald_models::{Error, Event, Status};

use crate::action::Action;
use crate::button::ButtonSpec;
use crate::format;
use crate::model::ModelCard;

/// The speed is the bytes that arrived over about this long, so a stall shows within seconds and a
/// burst does not swing the time left.
const SPEED_WINDOW: Duration = Duration::from_secs(5);
/// Less than this much progress gives no speed worth showing.
const SPEED_MIN_SPAN: Duration = Duration::from_secs(1);

#[derive(Debug, Clone, PartialEq)]
pub enum Download {
    NotStarted,
    /// A file of the pinned size is there but has not been checked against the pinned SHA-256.
    Unchecked,
    /// Stopped part way, by the user or a lasting break; the next download goes on from here.
    Stopped {
        received: u64,
    },
    /// Asked for, and nothing new has arrived yet: connecting, or checking what an earlier download left.
    Starting {
        received: u64,
    },
    Running {
        received: u64,
        throughput: Throughput,
        retry: Option<Retry>,
        /// Cancel was asked for; a stalled connection takes a few seconds to notice.
        stopping: bool,
    },
    /// In place and checked against the pinned SHA-256.
    Ready,
    Failed {
        /// What is kept for the next try.
        received: u64,
        error: DownloadError,
    },
}

/// The connection broke and is tried again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Retry {
    pub attempt: u32,
    pub of: u32,
}

/// Why a download failed, in the terms the user can act on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DownloadError {
    /// No connection, or it kept breaking.
    NoConnection,
    NotEnoughSpace {
        needed: u64,
        available: u64,
    },
    /// An HTTP status that trying again at once would not change.
    Server {
        status: u16,
    },
    /// The server's file is not the size pinned for it.
    WrongSize,
    /// The whole file arrived and did not match the pinned SHA-256; it was deleted.
    HashMismatch,
    /// The file could not be written.
    Disk {
        path: String,
    },
    /// The pinned address is not HTTPS, so it was never contacted.
    NotHttps,
}

impl Download {
    /// What the models folder holds when the window opens.
    pub fn from_status(status: Status) -> Self {
        match status {
            Status::Missing | Status::Other { .. } => Download::NotStarted,
            Status::Unchecked => Download::Unchecked,
            Status::Partial { received } => Download::Stopped { received },
            Status::Ready => Download::Ready,
        }
    }

    /// The app started the download. A model already in place stays so.
    pub fn start(&mut self, size: u64) {
        let received = match self {
            Download::Ready | Download::Starting { .. } | Download::Running { .. } => return,
            Download::NotStarted => 0,
            Download::Unchecked => size,
            Download::Stopped { received } | Download::Failed { received, .. } => *received,
        };
        *self = Download::Starting { received };
    }

    /// Cancel was asked for and the download has not stopped yet.
    pub fn stopping(&mut self) {
        match self {
            Download::Running { stopping, .. } => *stopping = true,
            Download::Starting { received } => {
                *self = Download::Running {
                    received: *received,
                    throughput: Throughput::default(),
                    retry: None,
                    stopping: true,
                };
            }
            _ => {}
        }
    }

    /// One event of the running download, as it happened at `at`. An event that arrives after the
    /// download has ended is ignored.
    pub fn event(&mut self, event: Event, at: Instant) {
        let (received, mut throughput, stopping) = match std::mem::replace(self, Self::NotStarted) {
            Download::Starting { received } => (received, Throughput::default(), false),
            Download::Running {
                received,
                throughput,
                stopping,
                ..
            } => (received, throughput, stopping),
            other => {
                *self = other;
                return;
            }
        };
        *self = match event {
            Event::Progress { received, .. } => {
                throughput.add(at, received);
                Download::Running {
                    received,
                    throughput,
                    retry: None,
                    stopping,
                }
            }
            Event::Retrying { attempt, of } => Download::Running {
                received,
                // The wait before the next try is no measure of the connection.
                throughput: Throughput::default(),
                retry: Some(Retry { attempt, of }),
                stopping,
            },
        };
    }

    /// The download returned.
    pub fn finish<T>(&mut self, result: &Result<T, Error>) {
        let received = self.received();
        let failed = |received, error| Download::Failed { received, error };
        *self = match result {
            Ok(_) => Download::Ready,
            Err(Error::Cancelled) => Download::Stopped { received },
            Err(Error::Connection(_)) => failed(received, DownloadError::NoConnection),
            Err(Error::NotEnoughSpace { needed, available }) => failed(
                received,
                DownloadError::NotEnoughSpace {
                    needed: *needed,
                    available: *available,
                },
            ),
            Err(Error::Status(status)) => {
                failed(received, DownloadError::Server { status: *status })
            }
            // What had arrived is deleted with these two.
            Err(Error::WrongSize { .. }) => failed(0, DownloadError::WrongSize),
            Err(Error::HashMismatch { .. }) => failed(0, DownloadError::HashMismatch),
            Err(Error::Io { path, .. }) => failed(
                received,
                DownloadError::Disk {
                    path: path.display().to_string(),
                },
            ),
            Err(Error::NotHttps(_)) => failed(received, DownloadError::NotHttps),
        };
    }

    /// The bytes on disk; a model in place counts as none, since none are to come.
    pub fn received(&self) -> u64 {
        match self {
            Download::Stopped { received }
            | Download::Starting { received }
            | Download::Running { received, .. }
            | Download::Failed { received, .. } => *received,
            Download::NotStarted | Download::Unchecked | Download::Ready => 0,
        }
    }

    pub fn is_running(&self) -> bool {
        matches!(self, Download::Starting { .. } | Download::Running { .. })
    }
}

/// The download's speed, from the progress of the last few seconds.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Throughput {
    samples: VecDeque<(Instant, u64)>,
}

impl Throughput {
    pub fn add(&mut self, at: Instant, received: u64) {
        if self
            .samples
            .back()
            .is_some_and(|&(last_at, last)| received < last || at < last_at)
        {
            // The download started over, or the clock went back.
            self.samples.clear();
        }
        self.samples.push_back((at, received));
        while self.samples.len() > 2
            && self
                .samples
                .get(1)
                .is_some_and(|&(second, _)| at.duration_since(second) >= SPEED_WINDOW)
        {
            self.samples.pop_front();
        }
    }

    pub fn bytes_per_second(&self) -> Option<f64> {
        let (&(first_at, first), &(last_at, last)) = (self.samples.front()?, self.samples.back()?);
        let span = last_at.duration_since(first_at);
        (span >= SPEED_MIN_SPAN).then(|| (last - first) as f64 / span.as_secs_f64())
    }

    pub fn time_left(&self, received: u64, total: u64) -> Option<Duration> {
        let rate = self.bytes_per_second().filter(|rate| *rate > 0.0)?;
        Some(Duration::from_secs_f64(
            total.saturating_sub(received) as f64 / rate,
        ))
    }
}

/// What step one shows for the download's state.
#[derive(Debug, Clone, PartialEq)]
pub struct DownloadView {
    /// How much of the file is here, from 0 to 1; `None` shows no bar.
    pub progress: Option<f32>,
    /// Shown as a bar that moves without a measure, while the work has no share to show.
    pub busy: bool,
    pub status: String,
    pub detail: Option<String>,
    /// Shown as a warning.
    pub error: Option<String>,
    pub button: Option<ButtonSpec>,
    pub done: bool,
}

impl DownloadView {
    pub fn of(download: &Download, card: &ModelCard) -> Self {
        let size = card.size;
        let share = |received: u64| Some(format::fraction(received, size));
        let blank = DownloadView {
            progress: None,
            busy: false,
            status: String::new(),
            detail: None,
            error: None,
            button: None,
            done: false,
        };
        match download {
            Download::NotStarted => DownloadView {
                status: format!("{} lastes ned én gang.", format::bytes(size)),
                button: Some(ButtonSpec::primary("Last ned", Action::StartDownload)),
                ..blank
            },
            Download::Unchecked => DownloadView {
                status: "Modellfilen ligger her allerede, men er ikke kontrollert.".to_owned(),
                detail: Some("Den kontrolleres mot SHA-256 før den brukes.".to_owned()),
                button: Some(ButtonSpec::primary(
                    "Kontroller filen",
                    Action::StartDownload,
                )),
                ..blank
            },
            Download::Stopped { received } => DownloadView {
                progress: share(*received),
                status: format!("{} er lastet ned.", format::bytes_of(*received, size)),
                detail: Some("Nedlastingen fortsetter der den stoppet.".to_owned()),
                button: Some(ButtonSpec::primary("Fortsett", Action::StartDownload)),
                ..blank
            },
            Download::Starting { received } => DownloadView {
                progress: share(*received),
                busy: true,
                status: if *received == 0 {
                    "Kobler til …".to_owned()
                } else if *received >= size {
                    "Kontrollerer filen mot SHA-256 …".to_owned()
                } else {
                    "Kontrollerer det som allerede er lastet ned …".to_owned()
                },
                button: Some(ButtonSpec::secondary("Avbryt", Action::CancelDownload)),
                ..blank
            },
            Download::Running {
                received,
                throughput,
                retry,
                stopping,
            } => {
                if *received >= size {
                    return DownloadView {
                        progress: share(*received),
                        busy: true,
                        status: "Kontrollerer filen mot SHA-256 …".to_owned(),
                        ..blank
                    };
                }
                let detail = if *stopping {
                    "Stopper …".to_owned()
                } else if let Some(Retry { attempt, of }) = retry {
                    format!("Forbindelsen brøt. Prøver igjen ({attempt} av {of}) …")
                } else {
                    match (
                        throughput.bytes_per_second(),
                        throughput.time_left(*received, size),
                    ) {
                        (Some(rate), Some(left)) => {
                            format!("{} · {}", format::speed(rate), format::time_left(left))
                        }
                        (Some(rate), None) => format::speed(rate),
                        _ => "Måler farten …".to_owned(),
                    }
                };
                DownloadView {
                    progress: share(*received),
                    status: format!(
                        "{} · {}",
                        format::bytes_of(*received, size),
                        format::percent(*received, size)
                    ),
                    detail: Some(detail),
                    button: (!*stopping)
                        .then(|| ButtonSpec::secondary("Avbryt", Action::CancelDownload)),
                    ..blank
                }
            }
            Download::Ready => DownloadView {
                status: "Lastet ned og kontrollert mot SHA-256.".to_owned(),
                done: true,
                ..blank
            },
            Download::Failed { received, error } => DownloadView {
                progress: (*received > 0).then(|| format::fraction(*received, size)),
                status: if *received > 0 {
                    format!("{} er lastet ned.", format::bytes_of(*received, size))
                } else {
                    "Modellen er ikke lastet ned.".to_owned()
                },
                error: Some(message(error, *received)),
                button: Some(ButtonSpec::primary("Prøv igjen", Action::StartDownload)),
                ..blank
            },
        }
    }
}

fn message(error: &DownloadError, kept: u64) -> String {
    let kept = if kept > 0 {
        " Det som er lastet ned, er tatt vare på."
    } else {
        ""
    };
    match error {
        DownloadError::NoConnection => {
            format!("Fikk ikke kontakt med serveren. Sjekk nettforbindelsen og prøv igjen.{kept}")
        }
        DownloadError::NotEnoughSpace { needed, available } => format!(
            "Det er ikke nok plass på disken: nedlastingen trenger {}, og {} er ledig.",
            format::bytes(*needed),
            format::bytes(*available)
        ),
        DownloadError::Server { status } => {
            format!("Serveren svarte med en feil (HTTP {status}). Prøv igjen senere.{kept}")
        }
        DownloadError::WrongSize => {
            "Filen på serveren har ikke størrelsen Skald venter. Prøv igjen senere.".to_owned()
        }
        DownloadError::HashMismatch => {
            "Filen som kom, stemte ikke med SHA-256-summen og er slettet. Prøv igjen.".to_owned()
        }
        DownloadError::Disk { path } => format!("Kunne ikke lagre modellen i {path}.{kept}"),
        DownloadError::NotHttps => {
            "Adressen til modellen er ikke HTTPS, så Skald laster den ikke ned.".to_owned()
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io;
    use std::path::PathBuf;

    use super::*;

    const SIZE: u64 = 3_095_033_483;

    fn card() -> ModelCard {
        ModelCard::of(skald_models::find("nb-whisper-large").unwrap()).unwrap()
    }

    fn progress(received: u64) -> Event {
        Event::Progress {
            received,
            total: SIZE,
        }
    }

    #[test]
    fn the_models_folder_decides_where_the_step_starts() {
        assert_eq!(Download::from_status(Status::Missing), Download::NotStarted);
        assert_eq!(
            Download::from_status(Status::Other { size: 12 }),
            Download::NotStarted,
            "a file of another pin is replaced"
        );
        assert_eq!(
            Download::from_status(Status::Unchecked),
            Download::Unchecked
        );
        assert_eq!(
            Download::from_status(Status::Partial { received: 7 }),
            Download::Stopped { received: 7 }
        );
        assert_eq!(Download::from_status(Status::Ready), Download::Ready);
    }

    #[test]
    fn a_download_goes_on_from_what_is_kept() {
        let mut download = Download::Stopped { received: 900 };
        download.start(SIZE);
        assert_eq!(download, Download::Starting { received: 900 });

        let mut unchecked = Download::Unchecked;
        unchecked.start(SIZE);
        assert_eq!(unchecked, Download::Starting { received: SIZE });

        let mut ready = Download::Ready;
        ready.start(SIZE);
        assert_eq!(ready, Download::Ready);
    }

    #[test]
    fn progress_moves_it_on_and_late_events_are_ignored() {
        let start = Instant::now();
        let mut download = Download::NotStarted;
        download.event(progress(10), start);
        assert_eq!(download, Download::NotStarted, "not started by the app");

        download.start(SIZE);
        download.event(progress(0), start);
        download.event(progress(50_000_000), start + Duration::from_secs(2));
        assert_eq!(download.received(), 50_000_000);
        let Download::Running { throughput, .. } = &download else {
            panic!("{download:?}");
        };
        assert_eq!(throughput.bytes_per_second(), Some(25_000_000.0));

        download.finish::<()>(&Err(Error::Cancelled));
        assert_eq!(
            download,
            Download::Stopped {
                received: 50_000_000
            }
        );
        download.event(progress(60_000_000), start + Duration::from_secs(3));
        assert_eq!(
            download,
            Download::Stopped {
                received: 50_000_000
            }
        );
    }

    #[test]
    fn a_retry_shows_until_the_next_progress_and_restarts_the_speed() {
        let start = Instant::now();
        let mut download = Download::Starting { received: 0 };
        download.event(progress(0), start);
        download.event(progress(10_000_000), start + Duration::from_secs(1));
        download.event(Event::Retrying { attempt: 2, of: 5 }, start);
        let view = DownloadView::of(&download, &card());
        assert_eq!(
            view.detail.as_deref(),
            Some("Forbindelsen brøt. Prøver igjen (2 av 5) …")
        );
        assert_eq!(download.received(), 10_000_000, "what arrived is kept");

        download.event(progress(11_000_000), start + Duration::from_secs(4));
        let Download::Running {
            retry, throughput, ..
        } = &download
        else {
            panic!("{download:?}");
        };
        assert_eq!(*retry, None);
        assert_eq!(throughput.bytes_per_second(), None, "measured afresh");
    }

    #[test]
    fn each_error_keeps_what_the_download_kept() {
        let finish = |error: Error| {
            let mut download = Download::Running {
                received: 400,
                throughput: Throughput::default(),
                retry: None,
                stopping: false,
            };
            download.finish::<()>(&Err(error));
            download
        };
        assert_eq!(
            finish(Error::Connection("reset".into())),
            Download::Failed {
                received: 400,
                error: DownloadError::NoConnection
            }
        );
        assert_eq!(
            finish(Error::HashMismatch {
                expected: "a".into(),
                actual: "b".into()
            }),
            Download::Failed {
                received: 0,
                error: DownloadError::HashMismatch
            }
        );
        assert_eq!(
            finish(Error::WrongSize {
                expected: 1,
                actual: 2
            }),
            Download::Failed {
                received: 0,
                error: DownloadError::WrongSize
            }
        );
        assert_eq!(
            finish(Error::Status(503)),
            Download::Failed {
                received: 400,
                error: DownloadError::Server { status: 503 }
            }
        );
        assert_eq!(
            finish(Error::Io {
                path: PathBuf::from("/models/nb-whisper-large"),
                error: io::Error::other("full"),
            }),
            Download::Failed {
                received: 400,
                error: DownloadError::Disk {
                    path: "/models/nb-whisper-large".into()
                }
            }
        );
        let mut done = Download::Starting { received: SIZE };
        done.finish(&Ok(PathBuf::from(
            "/models/nb-whisper-large/ggml-model.bin",
        )));
        assert_eq!(done, Download::Ready);
    }

    #[test]
    fn the_speed_is_measured_over_the_last_seconds() {
        let start = Instant::now();
        let at = |seconds: f64| start + Duration::from_secs_f64(seconds);
        let mut throughput = Throughput::default();
        throughput.add(at(0.0), 0);
        throughput.add(at(0.5), 5_000_000);
        assert_eq!(throughput.bytes_per_second(), None, "under a second");

        // 10 MB/s for ten seconds, then 2 MB/s for six.
        for second in 1u32..=10 {
            throughput.add(at(f64::from(second)), u64::from(second) * 10_000_000);
        }
        for second in 1u32..=6 {
            throughput.add(
                at(10.0 + f64::from(second)),
                100_000_000 + u64::from(second) * 2_000_000,
            );
        }
        let rate = throughput.bytes_per_second().unwrap();
        assert!((rate - 2_000_000.0).abs() < 1.0, "{rate}");
        assert_eq!(
            throughput.time_left(112_000_000, 132_000_000),
            Some(Duration::from_secs(10))
        );

        throughput.add(at(17.0), 0);
        assert_eq!(throughput.bytes_per_second(), None, "started over");
    }

    #[test]
    fn a_stalled_download_shows_no_time_left() {
        let start = Instant::now();
        let mut throughput = Throughput::default();
        throughput.add(start, 100);
        throughput.add(start + Duration::from_secs(6), 100);
        assert_eq!(throughput.bytes_per_second(), Some(0.0));
        assert_eq!(throughput.time_left(100, 200), None);
    }

    #[test]
    fn each_state_shows_its_words_and_button() {
        let card = card();
        let view = |download: Download| DownloadView::of(&download, &card);

        let not_started = view(Download::NotStarted);
        assert_eq!(not_started.status, "3,1 GB lastes ned én gang.");
        assert_eq!(
            not_started.button,
            Some(ButtonSpec::primary("Last ned", Action::StartDownload))
        );
        assert_eq!(not_started.progress, None);

        let stopped = view(Download::Stopped {
            received: 1_240_000_000,
        });
        assert_eq!(stopped.status, "1,2 av 3,1 GB er lastet ned.");
        assert_eq!(stopped.button.unwrap().label, "Fortsett");

        let start = Instant::now();
        let mut throughput = Throughput::default();
        throughput.add(start, 1_000_000_000);
        throughput.add(start + Duration::from_secs(4), 1_009_600_000);
        let running = view(Download::Running {
            received: 1_009_600_000,
            throughput,
            retry: None,
            stopping: false,
        });
        assert_eq!(running.status, "1,0 av 3,1 GB · 32 %");
        assert_eq!(
            running.detail.as_deref(),
            Some("2,4 MB/s · ca. 14 min igjen")
        );
        assert_eq!(
            running.button,
            Some(ButtonSpec::secondary("Avbryt", Action::CancelDownload))
        );

        let fresh = view(Download::Running {
            received: 10,
            throughput: Throughput::default(),
            retry: None,
            stopping: false,
        });
        assert_eq!(fresh.detail.as_deref(), Some("Måler farten …"));

        let mut stopping = Download::Running {
            received: 10,
            throughput: Throughput::default(),
            retry: None,
            stopping: false,
        };
        stopping.stopping();
        let stopping = view(stopping);
        assert_eq!(stopping.detail.as_deref(), Some("Stopper …"));
        assert_eq!(stopping.button, None);

        let checking = view(Download::Running {
            received: SIZE,
            throughput: Throughput::default(),
            retry: None,
            stopping: false,
        });
        assert_eq!(checking.status, "Kontrollerer filen mot SHA-256 …");
        assert!(checking.busy);
        assert_eq!(checking.button, None);

        assert_eq!(
            view(Download::Starting { received: 0 }).status,
            "Kobler til …"
        );
        assert_eq!(
            view(Download::Starting { received: 5 }).status,
            "Kontrollerer det som allerede er lastet ned …"
        );

        let ready = view(Download::Ready);
        assert!(ready.done);
        assert_eq!(ready.status, "Lastet ned og kontrollert mot SHA-256.");
        assert_eq!(ready.button, None);
    }

    #[test]
    fn an_error_is_plain_and_offers_a_retry() {
        let card = card();
        let failed = DownloadView::of(
            &Download::Failed {
                received: 500_000_000,
                error: DownloadError::NoConnection,
            },
            &card,
        );
        assert_eq!(
            failed.error.as_deref(),
            Some(
                "Fikk ikke kontakt med serveren. Sjekk nettforbindelsen og prøv igjen. \
                 Det som er lastet ned, er tatt vare på."
            )
        );
        assert_eq!(
            failed.button,
            Some(ButtonSpec::primary("Prøv igjen", Action::StartDownload))
        );

        let mismatch = DownloadView::of(
            &Download::Failed {
                received: 0,
                error: DownloadError::HashMismatch,
            },
            &card,
        );
        assert_eq!(mismatch.progress, None);
        assert_eq!(mismatch.status, "Modellen er ikke lastet ned.");
        assert_eq!(
            mismatch.error.as_deref(),
            Some("Filen som kom, stemte ikke med SHA-256-summen og er slettet. Prøv igjen.")
        );

        let space = DownloadView::of(
            &Download::Failed {
                received: 0,
                error: DownloadError::NotEnoughSpace {
                    needed: 3_095_033_483,
                    available: 1_200_000_000,
                },
            },
            &card,
        );
        assert_eq!(
            space.error.as_deref(),
            Some(
                "Det er ikke nok plass på disken: nedlastingen trenger 3,1 GB, og 1,2 GB er ledig."
            )
        );
    }
}
