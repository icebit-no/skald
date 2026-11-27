//! The system's microphones, and a take from one of them.

use std::fmt;
use std::str::FromStr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{DeviceId, ErrorKind, FromSample, SampleFormat, SizedSample};
use rtrb::{Producer, RingBuffer};

use crate::microphone::{Connected, MicId, Microphone};
use crate::take::{
    Ending, Processed, Processor, Recording, TAKE_CAP_SAMPLES, TakeEvent, push_mono,
};

/// How long the worker sleeps between drains of the ring; a fraction of a level reading.
const DRAIN_INTERVAL: Duration = Duration::from_millis(10);

/// How much audio the ring holds while the worker is away.
const RING: Duration = Duration::from_secs(2);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CaptureError {
    NotConnected,
    PermissionDenied,
    /// Another program holds the microphone.
    Busy,
    /// The microphone offers no sample format or rate this crate reads.
    UnsupportedFormat(String),
    /// Any other failure the audio system reports.
    System(String),
}

impl fmt::Display for CaptureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotConnected => f.write_str("the microphone is not connected"),
            Self::PermissionDenied => f.write_str("access to the microphone was denied"),
            Self::Busy => f.write_str("the microphone is in use by another program"),
            Self::UnsupportedFormat(detail) => {
                write!(
                    f,
                    "the microphone's audio format is not supported: {detail}"
                )
            }
            Self::System(detail) => write!(f, "the audio system failed: {detail}"),
        }
    }
}

impl std::error::Error for CaptureError {}

impl From<cpal::Error> for CaptureError {
    fn from(error: cpal::Error) -> Self {
        match error.kind() {
            ErrorKind::DeviceNotAvailable => Self::NotConnected,
            ErrorKind::PermissionDenied => Self::PermissionDenied,
            ErrorKind::DeviceBusy => Self::Busy,
            ErrorKind::UnsupportedConfig => Self::UnsupportedFormat(error.to_string()),
            _ => Self::System(error.to_string()),
        }
    }
}

/// The microphones connected now. Listing them opens none.
pub fn connected_microphones() -> Result<Connected, CaptureError> {
    let host = cpal::default_host();
    let microphones = host
        .input_devices()?
        // A device that disappears while it is being listed is left out.
        .filter_map(|device| microphone(&device).ok())
        .collect();
    let default = host
        .default_input_device()
        .and_then(|device| device.id().ok())
        .map(|id| MicId::new(id.to_string()));
    Ok(Connected {
        microphones,
        default,
    })
}

fn microphone(device: &cpal::Device) -> Result<Microphone, cpal::Error> {
    let id = MicId::new(device.id()?.to_string());
    let name = device.description()?.name().to_owned();
    Ok(Microphone { id, name })
}

/// A take in progress. Audio flows from the moment it starts; level readings and the cap arrive
/// as [`TakeEvent`]s.
///
/// Dropping a take without stopping it discards its audio.
pub struct Take {
    // Declared before the worker, so that a dropped take closes the stream before the worker's
    // last drain.
    stream: cpal::Stream,
    worker: Worker,
    events: Receiver<TakeEvent>,
    lost: Arc<AtomicBool>,
    microphone: Microphone,
}

impl Take {
    /// Opens the microphone and starts recording. On macOS the first take may make the system ask
    /// the user for access to the microphone.
    pub fn start(microphone: &Microphone) -> Result<Self, CaptureError> {
        let host = cpal::default_host();
        let device = DeviceId::from_str(microphone.id.as_str())
            .ok()
            .and_then(|id| host.device_by_id(&id))
            .ok_or(CaptureError::NotConnected)?;
        let supported = device.default_input_config()?;
        let config = supported.config();

        let processor = Processor::new(config.sample_rate, TAKE_CAP_SAMPLES)
            .map_err(|e| CaptureError::UnsupportedFormat(e.to_string()))?;
        let ring_len = (config.sample_rate as f64 * RING.as_secs_f64()) as usize;
        let (producer, consumer) = RingBuffer::new(ring_len);
        let shared = Shared {
            capped: Arc::new(AtomicBool::new(false)),
            lost: Arc::new(AtomicBool::new(false)),
        };

        let channels = usize::from(config.channels);
        let stream = match supported.sample_format() {
            SampleFormat::F32 => build::<f32>(&device, config, channels, producer, &shared),
            SampleFormat::F64 => build::<f64>(&device, config, channels, producer, &shared),
            SampleFormat::I8 => build::<i8>(&device, config, channels, producer, &shared),
            SampleFormat::I16 => build::<i16>(&device, config, channels, producer, &shared),
            SampleFormat::I24 => build::<cpal::I24>(&device, config, channels, producer, &shared),
            SampleFormat::I32 => build::<i32>(&device, config, channels, producer, &shared),
            SampleFormat::I64 => build::<i64>(&device, config, channels, producer, &shared),
            SampleFormat::U8 => build::<u8>(&device, config, channels, producer, &shared),
            SampleFormat::U16 => build::<u16>(&device, config, channels, producer, &shared),
            SampleFormat::U24 => build::<cpal::U24>(&device, config, channels, producer, &shared),
            SampleFormat::U32 => build::<u32>(&device, config, channels, producer, &shared),
            SampleFormat::U64 => build::<u64>(&device, config, channels, producer, &shared),
            other => return Err(CaptureError::UnsupportedFormat(other.to_string())),
        }?;

        let (sender, events) = mpsc::channel();
        let worker = Worker::spawn(consumer, processor, sender, &shared)?;
        let take = Self {
            stream,
            worker,
            events,
            lost: shared.lost,
            microphone: microphone.clone(),
        };
        take.stream.play()?;
        Ok(take)
    }

    pub fn microphone(&self) -> &Microphone {
        &self.microphone
    }

    /// Level readings, the cap and a lost microphone, as they happen. Never blocks the audio.
    pub fn events(&self) -> &Receiver<TakeEvent> {
        &self.events
    }

    /// Closes the microphone and returns the take, with all audio up to now or up to the cap.
    pub fn stop(self) -> Recording {
        let Self {
            stream,
            worker,
            events: _,
            lost,
            microphone,
        } = self;
        // Closing the stream first means the callback has written its last audio to the ring
        // before the worker's final drain.
        drop(stream);
        let processed = worker.finish();
        let ending = if processed.capped {
            Ending::Capped
        } else if lost.load(Ordering::Acquire) {
            Ending::MicrophoneLost
        } else {
            Ending::Stopped
        };
        Recording {
            samples: processed.samples,
            levels: processed.levels,
            ending,
            microphone,
        }
    }
}

/// Flags the audio callbacks and the worker share.
struct Shared {
    /// Set by the worker at the cap, so the callback stops pushing audio no one keeps.
    capped: Arc<AtomicBool>,
    /// Set by the error callback when the microphone stops delivering audio.
    lost: Arc<AtomicBool>,
}

fn build<T>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    channels: usize,
    mut ring: Producer<f32>,
    shared: &Shared,
) -> Result<cpal::Stream, CaptureError>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    let capped = shared.capped.clone();
    let lost = shared.lost.clone();
    let stream = device.build_input_stream::<T, _, _>(
        config,
        move |data, _| {
            if !capped.load(Ordering::Relaxed) {
                push_mono(data, channels, &mut ring);
            }
        },
        move |error| {
            if matches!(
                error.kind(),
                ErrorKind::DeviceNotAvailable | ErrorKind::StreamInvalidated
            ) {
                lost.store(true, Ordering::Release);
            }
        },
        None,
    )?;
    Ok(stream)
}

/// The thread that drains the ring for one take.
struct Worker {
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<Processed>>,
}

impl Worker {
    fn spawn(
        mut ring: rtrb::Consumer<f32>,
        mut processor: Processor,
        events: Sender<TakeEvent>,
        shared: &Shared,
    ) -> Result<Self, CaptureError> {
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let capped = shared.capped.clone();
        let lost = shared.lost.clone();
        let handle = thread::Builder::new()
            .name("skald-audio-take".into())
            .spawn(move || {
                // A receiver that has gone away only means no one is watching the meter.
                let mut send = |event| {
                    let _ = events.send(event);
                };
                let mut lost_reported = false;
                loop {
                    // Read before draining, so audio written before the stop is still drained.
                    let last_round = stopping.load(Ordering::Acquire);
                    processor.drain(&mut ring, &mut send);
                    if processor.is_capped() {
                        capped.store(true, Ordering::Relaxed);
                    }
                    if !lost_reported && lost.load(Ordering::Acquire) {
                        lost_reported = true;
                        send(TakeEvent::MicrophoneLost);
                    }
                    if last_round {
                        break;
                    }
                    thread::park_timeout(DRAIN_INTERVAL);
                }
                processor.finish(&mut send)
            })
            .map_err(|e| CaptureError::System(format!("cannot start the take's worker: {e}")))?;
        Ok(Self {
            stop,
            handle: Some(handle),
        })
    }

    fn finish(mut self) -> Processed {
        self.join()
            .expect("the handle is taken only here or on drop")
    }

    fn join(&mut self) -> Option<Processed> {
        let handle = self.handle.take()?;
        self.stop.store(true, Ordering::Release);
        handle.thread().unpark();
        Some(
            handle
                .join()
                .unwrap_or_else(|panic| std::panic::resume_unwind(panic)),
        )
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        if !thread::panicking() {
            self.join();
        }
    }
}
