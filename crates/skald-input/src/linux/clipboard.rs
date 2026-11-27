//! The clipboard on Wayland, through the wlroots data-control protocol, which reads and sets the selection
//! without a window of Skald's own in front. Hyprland, sway and the other wlroots compositors offer it, and
//! KDE Plasma; GNOME does not.
//!
//! Wayland keeps no copy of the clipboard: whoever set the selection is asked for its data each time
//! something pastes. So what Skald puts there lives in Skald, and a thread of its own keeps the connection
//! and answers those requests for as long as the selection is Skald's: until something else is copied, or
//! Skald quits.

use std::collections::HashMap;
use std::fs::File;
use std::io::{self, PipeReader, PipeWriter, Read, Write};
use std::os::fd::{AsFd, OwnedFd};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};

use rustix::event::{PollFd, PollFlags, Timespec, poll};
use wayland_client::backend::ObjectId;
use wayland_client::protocol::wl_registry::{self, WlRegistry};
use wayland_client::protocol::wl_seat::{self, WlSeat};
use wayland_client::{Connection, Dispatch, EventQueue, Proxy, QueueHandle, event_created_child};
use wayland_protocols_wlr::data_control::v1::client::zwlr_data_control_device_v1::{
    self, ZwlrDataControlDeviceV1,
};
use wayland_protocols_wlr::data_control::v1::client::zwlr_data_control_manager_v1::{
    self, ZwlrDataControlManagerV1,
};
use wayland_protocols_wlr::data_control::v1::client::zwlr_data_control_offer_v1::{
    self, ZwlrDataControlOfferV1,
};
use wayland_protocols_wlr::data_control::v1::client::zwlr_data_control_source_v1::{
    self, ZwlrDataControlSourceV1,
};

use crate::insert::{ChangeStamp, Previous};

/// The type password managers on Linux offer beside a secret, as KeePassXC does; clipboard managers keep
/// no history of contents that offer it.
pub(crate) const SENSITIVE: &str = "x-kde-passwordManagerHint";

/// What Skald offers under [`SENSITIVE`]: the value KDE gives it.
const SENSITIVE_VALUE: &[u8] = b"secret";

/// The types text is offered as, so Wayland and X11 programs alike can paste it.
const TEXT_TYPES: [&str; 5] = [
    "text/plain;charset=utf-8",
    "text/plain",
    "UTF8_STRING",
    "STRING",
    "TEXT",
];

/// The longest a program may take to hand over one type of the clipboard's contents.
const READ_WITHIN: Duration = Duration::from_secs(2);

/// The most the clipboard's contents may take, every type together. A program can offer an image in many
/// formats, each made when asked, so a limit keeps one copy from filling memory.
const MOST_BYTES: usize = 256 << 20;

/// The clipboard's contents: each type with its data, in the order they were offered. Empty when nothing
/// was copied.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Snapshot {
    items: Vec<(String, Vec<u8>)>,
}

impl Snapshot {
    /// Text as every program reads it.
    pub(crate) fn text(text: &str) -> Self {
        Self {
            items: TEXT_TYPES
                .iter()
                .map(|mime| ((*mime).to_owned(), text.as_bytes().to_vec()))
                .collect(),
        }
    }

    pub(crate) fn types(&self) -> impl Iterator<Item = &str> {
        self.items.iter().map(|(mime, _)| mime.as_str())
    }

    pub(crate) fn data(&self, mime: &str) -> Option<&[u8]> {
        self.items
            .iter()
            .find(|(offered, _)| offered == mime)
            .map(|(_, data)| data.as_slice())
    }
}

#[derive(Debug)]
pub(crate) enum ConnectError {
    /// No Wayland session to connect to, or it would not answer.
    NoWayland(String),
    /// The compositor has no data-control protocol, as GNOME's.
    NoDataControl,
    /// The compositor offers no seat, so there is no clipboard.
    NoSeat,
    Thread(io::Error),
}

impl std::fmt::Display for ConnectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConnectError::NoWayland(reason) => write!(f, "no Wayland session: {reason}"),
            ConnectError::NoDataControl => {
                f.write_str("the compositor has no wlr data-control protocol")
            }
            ConnectError::NoSeat => f.write_str("the compositor offers no seat"),
            ConnectError::Thread(error) => write!(f, "the clipboard's thread: {error}"),
        }
    }
}

/// The clipboard of the Wayland session, until dropped.
pub(crate) struct Clipboard {
    requests: Sender<Request>,
    /// A byte written here wakes the thread for the requests sent.
    wake: PipeWriter,
}

impl Clipboard {
    /// Connects to the session's compositor and learns what is on the clipboard now.
    pub(crate) fn connect() -> Result<Self, ConnectError> {
        let connection = Connection::connect_to_env()
            .map_err(|error| ConnectError::NoWayland(error.to_string()))?;
        let mut queue = connection.new_event_queue();
        let handle = queue.handle();
        connection.display().get_registry(&handle, ());
        let mut state = State::default();
        queue
            .roundtrip(&mut state)
            .map_err(|error| ConnectError::NoWayland(error.to_string()))?;
        let manager = state.manager.clone().ok_or(ConnectError::NoDataControl)?;
        let seat = state.seat.clone().ok_or(ConnectError::NoSeat)?;
        state.device = Some(manager.get_data_device(&seat, &handle, ()));
        // The device tells what the selection is as soon as it exists.
        queue
            .roundtrip(&mut state)
            .map_err(|error| ConnectError::NoWayland(error.to_string()))?;

        let (requests, inbox) = mpsc::channel();
        let (woken, wake) = io::pipe().map_err(ConnectError::Thread)?;
        thread::Builder::new()
            .name("skald-clipboard".into())
            .spawn(move || serve(queue, state, &inbox, woken))
            .map_err(ConnectError::Thread)?;
        Ok(Self { requests, wake })
    }

    /// What the clipboard holds, every type of it, unless it is marked as a secret.
    pub(crate) fn read(&self) -> Previous<Snapshot> {
        let (stamp, types) = match self.ask(|reply| Request::Types { reply }) {
            Some(Some(selection)) => selection,
            Some(None) => return Previous::Saved(Snapshot::default()),
            None => return Previous::Unreadable,
        };
        if types.iter().any(|mime| mime == SENSITIVE) {
            return Previous::MarkedPrivate;
        }
        let deadline = Instant::now() + READ_WITHIN * u32::try_from(types.len()).unwrap_or(1);
        let mut snapshot = Snapshot::default();
        let mut bytes = 0;
        for mime in types {
            let pipe = self.ask(|reply| Request::Receive {
                stamp,
                mime: mime.clone(),
                reply,
            });
            // The selection changed while it was read, or the thread has gone.
            let Some(Some(pipe)) = pipe else {
                return Previous::Unreadable;
            };
            match read_to_end(pipe, deadline, MOST_BYTES - bytes) {
                Some(data) => {
                    bytes += data.len();
                    snapshot.items.push((mime, data));
                }
                None => return Previous::Unreadable,
            }
        }
        Previous::Saved(snapshot)
    }

    /// Makes `snapshot` the clipboard's contents, offered as a secret when `sensitive`, so clipboard
    /// managers keep no history of it. An empty snapshot clears the clipboard. Returns the clipboard's
    /// stamp with the change made.
    pub(crate) fn write(&self, snapshot: Snapshot, sensitive: bool) -> Option<ChangeStamp> {
        self.ask(|reply| Request::Write {
            snapshot,
            sensitive,
            reply,
        })
        .flatten()
    }

    /// Changes with every new selection, Skald's own included.
    pub(crate) fn stamp(&self) -> Option<ChangeStamp> {
        self.ask(|reply| Request::Stamp { reply })
    }

    /// Sends a request to the thread and waits for its answer; `None` once the thread has gone.
    fn ask<T>(&self, request: impl FnOnce(Sender<T>) -> Request) -> Option<T> {
        let (reply, answer) = mpsc::channel();
        self.requests.send(request(reply)).ok()?;
        (&self.wake).write_all(&[1]).ok()?;
        answer.recv().ok()
    }
}

/// Reads a pipe until the program writing it closes it, or gives up at `deadline` or past `most` bytes.
fn read_to_end(mut pipe: PipeReader, deadline: Instant, most: usize) -> Option<Vec<u8>> {
    let mut data = Vec::new();
    let mut chunk = [0; 64 * 1024];
    loop {
        let left = deadline.checked_duration_since(Instant::now())?;
        let timeout = Timespec::try_from(left).ok()?;
        let mut fds = [PollFd::new(&pipe, PollFlags::IN)];
        if poll(&mut fds, Some(&timeout)).ok()? == 0 {
            return None;
        }
        match pipe.read(&mut chunk) {
            Ok(0) => return Some(data),
            Ok(read) => {
                data.extend_from_slice(&chunk[..read]);
                if data.len() > most {
                    return None;
                }
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(_) => return None,
        }
    }
}

enum Request {
    /// The selection's stamp and types; `None` when nothing is copied.
    Types {
        reply: Sender<Option<(ChangeStamp, Vec<String>)>>,
    },
    /// A pipe the selection's owner writes `mime` into; `None` when the selection is no longer the one
    /// `stamp` names.
    Receive {
        stamp: ChangeStamp,
        mime: String,
        reply: Sender<Option<PipeReader>>,
    },
    Write {
        snapshot: Snapshot,
        sensitive: bool,
        reply: Sender<Option<ChangeStamp>>,
    },
    Stamp {
        reply: Sender<ChangeStamp>,
    },
}

/// The connection's side: what the compositor has said, and what Skald offers.
#[derive(Default)]
struct State {
    seat: Option<WlSeat>,
    manager: Option<ZwlrDataControlManagerV1>,
    device: Option<ZwlrDataControlDeviceV1>,
    /// The types of each offer the compositor has made and not yet withdrawn.
    offers: HashMap<ObjectId, Vec<String>>,
    selection: Option<ZwlrDataControlOfferV1>,
    /// Counts the selections the compositor has announced.
    stamp: ChangeStamp,
    /// What each of Skald's sources offers, kept until the compositor cancels it.
    sources: HashMap<ObjectId, Arc<Offered>>,
}

/// What one of Skald's sources hands out.
struct Offered {
    snapshot: Snapshot,
    sensitive: bool,
}

impl Offered {
    fn data(&self, mime: &str) -> Option<&[u8]> {
        if self.sensitive && mime == SENSITIVE {
            return Some(SENSITIVE_VALUE);
        }
        self.snapshot.data(mime)
    }
}

/// Answers requests and the compositor until the clipboard is dropped or the connection fails.
fn serve(
    mut queue: EventQueue<State>,
    mut state: State,
    requests: &Receiver<Request>,
    mut woken: PipeReader,
) {
    loop {
        loop {
            match requests.try_recv() {
                Ok(request) => {
                    if handle(request, &mut queue, &mut state).is_err() {
                        return;
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return,
            }
        }
        if queue.dispatch_pending(&mut state).is_err() || queue.flush().is_err() {
            return;
        }
        let Some(guard) = queue.prepare_read() else {
            continue;
        };
        let (from_compositor, from_skald) = {
            let connection = guard.connection_fd();
            let mut fds = [
                PollFd::new(&connection, PollFlags::IN),
                PollFd::new(&woken, PollFlags::IN),
            ];
            match poll(&mut fds, None) {
                Ok(_) => (!fds[0].revents().is_empty(), !fds[1].revents().is_empty()),
                Err(error) if error == rustix::io::Errno::INTR => (false, false),
                Err(_) => return,
            }
        };
        if from_compositor {
            if guard.read().is_err() {
                return;
            }
        } else {
            drop(guard);
        }
        if from_skald {
            let mut bytes = [0; 64];
            // End of file: the clipboard was dropped, which the requests' channel tells next.
            if woken.read(&mut bytes).is_err() {
                return;
            }
        }
    }
}

/// Carries out one request; an error ends the connection.
fn handle(
    request: Request,
    queue: &mut EventQueue<State>,
    state: &mut State,
) -> Result<(), wayland_client::DispatchError> {
    match request {
        Request::Types { reply } => {
            let types = state
                .selection
                .as_ref()
                .map(|offer| state.offers.get(&offer.id()).cloned().unwrap_or_default());
            let _ = reply.send(types.map(|types| (state.stamp, types)));
        }
        Request::Receive { stamp, mime, reply } => {
            let pipe = match &state.selection {
                Some(offer) if stamp == state.stamp => io::pipe().ok().map(|(reader, writer)| {
                    offer.receive(mime, writer.as_fd());
                    reader
                }),
                _ => None,
            };
            // The compositor must have the request before the caller waits on the pipe.
            queue.flush()?;
            let _ = reply.send(pipe);
        }
        Request::Write {
            snapshot,
            sensitive,
            reply,
        } => {
            let handle = queue.handle();
            let (Some(manager), Some(device)) = (&state.manager, &state.device) else {
                let _ = reply.send(None);
                return Ok(());
            };
            if snapshot.items.is_empty() {
                device.set_selection(None);
            } else {
                let source = manager.create_data_source(&handle, ());
                for mime in snapshot.types() {
                    source.offer(mime.to_owned());
                }
                if sensitive {
                    source.offer(SENSITIVE.to_owned());
                }
                device.set_selection(Some(&source));
                state.sources.insert(
                    source.id(),
                    Arc::new(Offered {
                        snapshot,
                        sensitive,
                    }),
                );
            }
            // The compositor announces the new selection before it answers the roundtrip.
            queue.roundtrip(state)?;
            let _ = reply.send(Some(state.stamp));
        }
        Request::Stamp { reply } => {
            let _ = reply.send(state.stamp);
        }
    }
    Ok(())
}

impl Dispatch<WlRegistry, ()> for State {
    fn event(
        state: &mut Self,
        registry: &WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        handle: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name, interface, ..
        } = event
        {
            if interface == WlSeat::interface().name && state.seat.is_none() {
                state.seat = Some(registry.bind(name, 1, handle, ()));
            } else if interface == ZwlrDataControlManagerV1::interface().name {
                state.manager = Some(registry.bind(name, 1, handle, ()));
            }
        }
    }
}

impl Dispatch<WlSeat, ()> for State {
    fn event(
        _: &mut Self,
        _: &WlSeat,
        _: wl_seat::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwlrDataControlManagerV1, ()> for State {
    fn event(
        _: &mut Self,
        _: &ZwlrDataControlManagerV1,
        _: zwlr_data_control_manager_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwlrDataControlDeviceV1, ()> for State {
    fn event(
        state: &mut Self,
        _: &ZwlrDataControlDeviceV1,
        event: zwlr_data_control_device_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            zwlr_data_control_device_v1::Event::DataOffer { id } => {
                state.offers.insert(id.id(), Vec::new());
            }
            zwlr_data_control_device_v1::Event::Selection { id } => {
                if let Some(previous) = state.selection.take()
                    && id.as_ref() != Some(&previous)
                {
                    state.offers.remove(&previous.id());
                    previous.destroy();
                }
                state.selection = id;
                state.stamp = state.stamp.wrapping_add(1);
            }
            zwlr_data_control_device_v1::Event::Finished => {
                state.device = None;
                state.selection = None;
            }
            // Only offered from version 2, which Skald does not bind.
            zwlr_data_control_device_v1::Event::PrimarySelection { id: Some(offer) } => {
                state.offers.remove(&offer.id());
                offer.destroy();
            }
            _ => {}
        }
    }

    event_created_child!(State, ZwlrDataControlDeviceV1, [
        zwlr_data_control_device_v1::EVT_DATA_OFFER_OPCODE => (ZwlrDataControlOfferV1, ()),
    ]);
}

impl Dispatch<ZwlrDataControlOfferV1, ()> for State {
    fn event(
        state: &mut Self,
        offer: &ZwlrDataControlOfferV1,
        event: zwlr_data_control_offer_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let zwlr_data_control_offer_v1::Event::Offer { mime_type } = event
            && let Some(types) = state.offers.get_mut(&offer.id())
        {
            types.push(mime_type);
        }
    }
}

impl Dispatch<ZwlrDataControlSourceV1, ()> for State {
    fn event(
        state: &mut Self,
        source: &ZwlrDataControlSourceV1,
        event: zwlr_data_control_source_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            zwlr_data_control_source_v1::Event::Send { mime_type, fd } => {
                if let Some(offered) = state.sources.get(&source.id()) {
                    hand_over(Arc::clone(offered), mime_type, fd);
                }
            }
            zwlr_data_control_source_v1::Event::Cancelled => {
                state.sources.remove(&source.id());
                source.destroy();
            }
            _ => {}
        }
    }
}

/// Writes the data a paste asked for on a thread of its own, so a program that reads slowly holds up
/// neither the other pastes nor the clipboard's requests.
fn hand_over(offered: Arc<Offered>, mime: String, fd: OwnedFd) {
    let _ = thread::Builder::new()
        .name("skald-clipboard-paste".into())
        .spawn(move || {
            if let Some(data) = offered.data(&mime) {
                // A reader that went away is no reason to tell anyone; the write just stops.
                let _ = File::from(fd).write_all(data);
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_is_offered_under_every_name_programs_read_it_by() {
        let snapshot = Snapshot::text("Hei ");
        assert_eq!(snapshot.types().collect::<Vec<_>>(), TEXT_TYPES);
        for mime in TEXT_TYPES {
            assert_eq!(snapshot.data(mime), Some("Hei ".as_bytes()));
        }
        assert_eq!(snapshot.data("text/html"), None);
    }

    #[test]
    fn a_sensitive_source_hands_out_the_secret_marker_and_its_text() {
        let offered = Offered {
            snapshot: Snapshot::text("Hei "),
            sensitive: true,
        };
        assert_eq!(offered.data(SENSITIVE), Some(SENSITIVE_VALUE));
        assert_eq!(offered.data("text/plain"), Some("Hei ".as_bytes()));
        let restored = Offered {
            snapshot: Snapshot::text("før"),
            sensitive: false,
        };
        assert_eq!(restored.data(SENSITIVE), None);
    }

    #[test]
    fn a_pipe_is_read_to_its_end_within_the_limits() {
        let (reader, mut writer) = io::pipe().unwrap();
        writer.write_all(b"<b>Hei</b>").unwrap();
        drop(writer);
        let later = Instant::now() + Duration::from_secs(1);
        assert_eq!(
            read_to_end(reader, later, 1024).as_deref(),
            Some(&b"<b>Hei</b>"[..])
        );

        let (reader, mut writer) = io::pipe().unwrap();
        writer.write_all(b"<b>Hei</b>").unwrap();
        drop(writer);
        assert_eq!(read_to_end(reader, later, 4), None, "past the limit");

        // A writer that never closes its end is given up on at the deadline.
        let (reader, _writer) = io::pipe().unwrap();
        let soon = Instant::now() + Duration::from_millis(50);
        assert_eq!(read_to_end(reader, soon, 1024), None);
    }
}
