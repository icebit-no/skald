//! The local socket between the command line and the running Skald, a Unix socket on macOS and Linux.
//!
//! Only the user can open it: its folder is made 0700 before the socket is created and the socket itself
//! 0600. The folder is what keeps others out while the socket is being created, before its own mode is
//! set. A socket left by a Skald that crashed accepts no connection; binding finds that and replaces it.

use std::fs;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::os::unix::fs::{FileTypeExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::commands;
use crate::protocol::{self, Error, ErrorCode, MAX_LINE, Output, Request};
use crate::skald::Skald;

/// How long a connection may sit idle before the server closes it.
const IDLE: Duration = Duration::from_secs(60);

/// How long the command line waits for an answer. Transcribing a long file takes longer than the rest.
const ANSWER: Duration = Duration::from_secs(30);
const TRANSCRIBE_ANSWER: Duration = Duration::from_secs(600);

/// A socket bound at its path, not yet answering.
#[derive(Debug)]
pub struct Server {
    listener: UnixListener,
    path: PathBuf,
}

#[derive(Debug)]
pub enum BindError {
    /// Another Skald answers on the socket.
    AlreadyRunning,
    /// Something other than a socket is at the socket's path; it is left alone.
    NotASocket(PathBuf),
    Io(PathBuf, io::Error),
}

impl std::fmt::Display for BindError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AlreadyRunning => f.write_str("another Skald is answering on the socket"),
            Self::NotASocket(path) => write!(f, "{} is not a socket", path.display()),
            Self::Io(path, error) => write!(f, "{}: {error}", path.display()),
        }
    }
}

impl std::error::Error for BindError {}

impl Server {
    pub fn bind(path: &Path) -> Result<Self, BindError> {
        let io = |at: &Path| {
            let at = at.to_path_buf();
            move |error| BindError::Io(at, error)
        };
        let folder = path.parent().expect("the socket is in a folder");
        fs::create_dir_all(folder).map_err(io(folder))?;
        fs::set_permissions(folder, fs::Permissions::from_mode(0o700)).map_err(io(folder))?;

        match fs::symlink_metadata(path) {
            Ok(metadata) if metadata.file_type().is_socket() => match UnixStream::connect(path) {
                Ok(_) => return Err(BindError::AlreadyRunning),
                Err(error) if error.kind() == io::ErrorKind::ConnectionRefused => {
                    fs::remove_file(path).map_err(io(path))?;
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(BindError::Io(path.to_path_buf(), error)),
            },
            Ok(_) => return Err(BindError::NotASocket(path.to_path_buf())),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(BindError::Io(path.to_path_buf(), error)),
        }

        let listener = UnixListener::bind(path).map_err(io(path))?;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).map_err(io(path))?;
        Ok(Self {
            listener,
            path: path.to_path_buf(),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Answers on a thread of its own until the returned [`Serving`] is stopped or dropped. Each
    /// connection gets a thread, so a long transcription does not hold up a quick command.
    pub fn serve(self, skald: Arc<dyn Skald + Send + Sync>) -> Serving {
        let stopping = Arc::new(AtomicBool::new(false));
        let thread = {
            let stopping = Arc::clone(&stopping);
            let listener = self.listener;
            thread::Builder::new()
                .name("skald-control".to_owned())
                .spawn(move || accept(&listener, &skald, &stopping))
                .expect("a thread can be started")
        };
        Serving {
            path: self.path,
            stopping,
            thread: Some(thread),
        }
    }
}

/// The server answering. Stopping it removes the socket.
#[derive(Debug)]
pub struct Serving {
    path: PathBuf,
    stopping: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Serving {
    pub fn stop(mut self) {
        self.shut_down();
    }

    fn shut_down(&mut self) {
        let Some(thread) = self.thread.take() else {
            return;
        };
        self.stopping.store(true, Ordering::SeqCst);
        // Wakes the thread waiting for the next connection, so it sees it is stopping.
        let _ = UnixStream::connect(&self.path);
        let _ = thread.join();
        let _ = fs::remove_file(&self.path);
    }
}

impl Drop for Serving {
    fn drop(&mut self) {
        self.shut_down();
    }
}

fn accept(listener: &UnixListener, skald: &Arc<dyn Skald + Send + Sync>, stopping: &AtomicBool) {
    for stream in listener.incoming() {
        if stopping.load(Ordering::SeqCst) {
            break;
        }
        match stream {
            Ok(stream) => {
                let skald = Arc::clone(skald);
                let _ = thread::Builder::new()
                    .name("skald-control-connection".to_owned())
                    .spawn(move || answer(stream, &*skald));
            }
            // Out of file descriptors, for one: wait rather than spin.
            Err(_) => thread::sleep(Duration::from_millis(50)),
        }
    }
}

fn answer(stream: UnixStream, skald: &dyn Skald) {
    if stream.set_read_timeout(Some(IDLE)).is_err() {
        return;
    }
    let Ok(mut writer) = stream.try_clone() else {
        return;
    };
    let mut reader = BufReader::new(stream);
    loop {
        let line = match read_line(&mut reader) {
            Ok(Some(line)) => line,
            Ok(None) | Err(_) => return,
        };
        let result =
            protocol::decode_request(&line).and_then(|request| commands::execute(&request, skald));
        let mut response = protocol::encode_response(&result);
        response.push('\n');
        if writer.write_all(response.as_bytes()).is_err() {
            return;
        }
    }
}

/// One line without its line break; `None` at the end of the stream. A line over [`MAX_LINE`] is an
/// error.
fn read_line(reader: &mut impl BufRead) -> io::Result<Option<String>> {
    let mut line = String::new();
    let read = reader
        .by_ref()
        .take(MAX_LINE as u64 + 1)
        .read_line(&mut line)?;
    if read == 0 {
        return Ok(None);
    }
    if line.len() > MAX_LINE {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "line too long"));
    }
    if line.ends_with('\n') {
        line.pop();
    }
    Ok(Some(line))
}

/// A connection to the running Skald.
#[derive(Debug)]
pub struct Client {
    reader: BufReader<UnixStream>,
    writer: UnixStream,
}

impl Client {
    /// Connects to the Skald answering on the socket; `None` when no Skald is running.
    pub fn connect(path: &Path) -> Result<Option<Self>, Error> {
        let stream = match UnixStream::connect(path) {
            Ok(stream) => stream,
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused
                ) =>
            {
                return Ok(None);
            }
            Err(error) => {
                return Err(Error::new(
                    ErrorCode::Io,
                    format!("Får ikke kontakt med Skald på {}: {error}", path.display()),
                ));
            }
        };
        let writer = stream.try_clone().map_err(lost)?;
        Ok(Some(Self {
            reader: BufReader::new(stream),
            writer,
        }))
    }

    pub fn send(&mut self, request: &Request) -> Result<Output, Error> {
        let wait = match request {
            Request::Transcribe { .. } => TRANSCRIBE_ANSWER,
            _ => ANSWER,
        };
        self.reader
            .get_ref()
            .set_read_timeout(Some(wait))
            .map_err(lost)?;
        let mut line = protocol::encode_request(request);
        line.push('\n');
        self.writer.write_all(line.as_bytes()).map_err(lost)?;
        match read_line(&mut self.reader) {
            Ok(Some(line)) => protocol::decode_response(&line),
            Ok(None) => Err(Error::new(
                ErrorCode::Io,
                "Skald lukket forbindelsen uten å svare.",
            )),
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) =>
            {
                Err(Error::new(ErrorCode::Io, "Skald svarer ikke."))
            }
            Err(error) => Err(lost(error)),
        }
    }
}

fn lost(error: io::Error) -> Error {
    Error::new(
        ErrorCode::Io,
        format!("Mistet forbindelsen med Skald: {error}"),
    )
}
