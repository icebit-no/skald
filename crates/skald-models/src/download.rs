//! Fetching a pinned model file: HTTPS only, the rest of a broken download asked for with an HTTP range, and
//! nothing given the file's name until its SHA-256 matches.

use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use ring::digest;
use ureq::Agent;
use ureq::tls::{RootCerts, TlsConfig, TlsProvider};

use crate::free_space;
use crate::registry::ModelFile;
use crate::status::{self, Status};

/// A connection that brings nothing for this long counts as broken. Cancelling a stalled download waits
/// at most this long.
const READ_TIMEOUT: Duration = Duration::from_secs(15);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);
const CHUNK: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    /// Bytes of the file on disk, of its size. Sent when the download starts, a few times a second while it
    /// runs, and when the whole file is there.
    Progress { received: u64, total: u64 },
    /// The connection broke, and is tried again from where it stopped.
    Retrying {
        /// Counted from the last try that brought more of the file.
        attempt: u32,
        /// The most tries before the download gives up.
        of: u32,
    },
}

/// Stops a download from another thread. It takes effect at the next read: at once while data flows, within
/// the read timeout on a stalled connection. What arrived is kept, and the next download goes on from it.
#[derive(Debug, Clone, Default)]
pub struct Cancel(Arc<AtomicBool>);

impl Cancel {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

#[derive(Debug)]
pub enum Error {
    /// An address that is not HTTPS. Plain HTTP is taken only to this machine, which nothing but tests
    /// serves.
    NotHttps(String),
    NotEnoughSpace {
        needed: u64,
        available: u64,
    },
    /// An HTTP status that another try would not change.
    Status(u16),
    /// The server's file is not the size pinned for it. What had arrived is deleted.
    WrongSize {
        expected: u64,
        actual: u64,
    },
    /// The whole file arrived and its SHA-256 is not the pinned one. It is deleted, so the next download
    /// starts over.
    HashMismatch {
        expected: String,
        actual: String,
    },
    /// No connection, or it broke more often than the download tries again. What arrived is kept.
    Connection(String),
    Cancelled,
    Io {
        path: PathBuf,
        error: io::Error,
    },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotHttps(url) => write!(f, "{url} is not an HTTPS address"),
            Self::NotEnoughSpace { needed, available } => write!(
                f,
                "not enough free space: {needed} bytes needed, {available} available"
            ),
            Self::Status(code) => write!(f, "the server answered HTTP {code}"),
            Self::WrongSize { expected, actual } => write!(
                f,
                "the server's file is {actual} bytes, not the pinned {expected}"
            ),
            Self::HashMismatch { expected, actual } => write!(
                f,
                "the downloaded file's SHA-256 is {actual}, not the pinned {expected}"
            ),
            Self::Connection(reason) => write!(f, "the download failed: {reason}"),
            Self::Cancelled => f.write_str("the download was cancelled"),
            Self::Io { path, error } => write!(f, "{}: {error}", path.display()),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { error, .. } => Some(error),
            _ => None,
        }
    }
}

/// Downloads model files. One is made once and used for every download; it keeps its connections.
#[derive(Debug, Clone)]
pub struct Downloader {
    agent: Agent,
    retries: u32,
    first_delay: Duration,
}

impl Default for Downloader {
    fn default() -> Self {
        Self::new()
    }
}

impl Downloader {
    /// Certificates are checked by the system's own verifier, so a certificate the user's system trusts or
    /// revokes counts the same here. A proxy set in the environment is used.
    pub fn new() -> Self {
        let tls = TlsConfig::builder()
            .provider(TlsProvider::Rustls)
            .root_certs(RootCerts::PlatformVerifier)
            .unversioned_rustls_crypto_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .build();
        let agent = Agent::config_builder()
            .tls_config(tls)
            .https_only(true)
            .http_status_as_error(false)
            .timeout_connect(Some(CONNECT_TIMEOUT))
            .timeout_per_read(Some(READ_TIMEOUT))
            .build()
            .new_agent();
        Self {
            agent,
            retries: 5,
            first_delay: Duration::from_secs(1),
        }
    }

    /// How often a broken connection is tried again before the download gives up, and the wait before the
    /// first try; each later wait is twice the one before. The count starts over whenever a try brings more
    /// of the file.
    pub fn retries(mut self, retries: u32, first_delay: Duration) -> Self {
        self.retries = retries;
        self.first_delay = first_delay;
        self
    }

    /// Brings the file into the model's folder and returns its path. A file already there is checked rather than fetched
    /// again, and a download that stopped earlier goes on from where it stopped. Nothing but the file's URL,
    /// and where it redirects to over HTTPS, is contacted. Blocks until the file is there, the download fails,
    /// or it is cancelled.
    pub fn download(
        &self,
        file: &ModelFile,
        dir: &Path,
        cancel: &Cancel,
        mut on_event: impl FnMut(Event),
    ) -> Result<PathBuf, Error> {
        let loopback = is_loopback_http(&file.url)?;
        let target = dir.join(&*file.name);
        let whole = Event::Progress {
            received: file.size,
            total: file.size,
        };
        match status::status(file, dir) {
            Status::Ready => {
                on_event(whole);
                return Ok(target);
            }
            Status::Unchecked if verify(file, dir, cancel)? => {
                on_event(whole);
                return Ok(target);
            }
            _ => {}
        }

        fs::create_dir_all(dir).map_err(|error| io_error(dir, error))?;
        for stale in status::stale_parts(file, dir) {
            fs::remove_file(&stale).map_err(|error| io_error(&stale, error))?;
        }
        let part_path = status::part_path(file, dir);
        let kept = fs::metadata(&part_path).map(|meta| meta.len()).unwrap_or(0);
        let needed = file.size - if kept <= file.size { kept } else { 0 };
        let available = free_space::available(dir).map_err(|error| io_error(dir, error))?;
        if available < needed {
            return Err(Error::NotEnoughSpace { needed, available });
        }

        let mut part = Part::open(&part_path, file.size, cancel)?;
        let mut progress = Progress::start(&mut on_event, part.received, file.size);
        match self.fetch(file, loopback, &mut part, cancel, &mut progress) {
            Ok(()) => {}
            Err(error @ Error::WrongSize { .. }) => {
                drop(part);
                let _ = fs::remove_file(&part_path);
                return Err(error);
            }
            Err(error) => return Err(error),
        }

        let actual = part.finish()?;
        if actual != file.sha256 {
            let _ = fs::remove_file(&part_path);
            return Err(Error::HashMismatch {
                expected: file.sha256.to_string(),
                actual,
            });
        }
        let record = status::record_path(file, dir);
        match fs::remove_file(&record) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(io_error(&record, error)),
        }
        fs::rename(&part_path, &target).map_err(|error| io_error(&target, error))?;
        write_record(file, dir)?;
        Ok(target)
    }

    /// Asks for the rest of the file until all of it has arrived, trying again when the connection breaks.
    fn fetch(
        &self,
        file: &ModelFile,
        loopback: bool,
        part: &mut Part,
        cancel: &Cancel,
        progress: &mut Progress<'_>,
    ) -> Result<(), Error> {
        let mut streak = 0;
        while part.received < file.size {
            let appended = part.appended;
            match self.attempt(file, loopback, part, cancel, progress) {
                Ok(()) => {}
                Err(Failure::Fatal(error)) => return Err(error),
                Err(Failure::Broken(reason)) => {
                    if part.appended > appended {
                        streak = 0;
                    }
                    if streak == self.retries {
                        return Err(Error::Connection(reason));
                    }
                    streak += 1;
                    progress.send(Event::Retrying {
                        attempt: streak,
                        of: self.retries,
                    });
                    let delay = self.first_delay.saturating_mul(1 << (streak - 1).min(16));
                    wait(delay, cancel)?;
                }
            }
        }
        Ok(())
    }

    /// One request for the file from where the part on disk ends.
    fn attempt(
        &self,
        file: &ModelFile,
        loopback: bool,
        part: &mut Part,
        cancel: &Cancel,
        progress: &mut Progress<'_>,
    ) -> Result<(), Failure> {
        if cancel.is_cancelled() {
            return Err(Failure::Fatal(Error::Cancelled));
        }
        let mut request = self.agent.get(&*file.url);
        if loopback {
            request = request.config().https_only(false).proxy(None).build();
        }
        if part.received > 0 {
            request = request.header("Range", format!("bytes={}-", part.received));
        }
        let response = request.call().map_err(failure)?;

        match response.status().as_u16() {
            200 => {
                part.restart()?;
                if let Some(length) = content_length(&response)
                    && length != file.size
                {
                    return Err(Failure::Fatal(Error::WrongSize {
                        expected: file.size,
                        actual: length,
                    }));
                }
            }
            206 => match content_range(&response) {
                Some((_, Some(total))) if total != file.size => {
                    return Err(Failure::Fatal(Error::WrongSize {
                        expected: file.size,
                        actual: total,
                    }));
                }
                Some((start, _)) if start == part.received => {}
                _ => {
                    part.restart()?;
                    return Err(Failure::Broken(
                        "the server sent another part of the file than the one asked for".into(),
                    ));
                }
            },
            416 => {
                part.restart()?;
                return Err(Failure::Broken(
                    "the server could not send the rest of the file".into(),
                ));
            }
            code @ 500..=599 => return Err(Failure::Broken(format!("HTTP {code}"))),
            code => return Err(Failure::Fatal(Error::Status(code))),
        }

        let mut body = response.into_body().into_reader();
        let mut buffer = vec![0; CHUNK];
        loop {
            if cancel.is_cancelled() {
                return Err(Failure::Fatal(Error::Cancelled));
            }
            let read = match body.read(&mut buffer) {
                Ok(0) => break,
                Ok(read) => read,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(Failure::Broken(error.to_string())),
            };
            let received = part.received + read as u64;
            if received > file.size {
                return Err(Failure::Fatal(Error::WrongSize {
                    expected: file.size,
                    actual: received,
                }));
            }
            part.append(&buffer[..read])?;
            progress.update(part.received);
        }
        if part.received < file.size {
            return Err(Failure::Broken(
                "the connection closed before the whole file arrived".into(),
            ));
        }
        Ok(())
    }
}

/// Reads a file already in the model's folder and, if its SHA-256 is the pinned one, records it as checked.
/// A file that does not match is left as it is; downloading replaces it.
pub fn verify(file: &ModelFile, dir: &Path, cancel: &Cancel) -> Result<bool, Error> {
    let path = dir.join(&*file.name);
    let mut reader = File::open(&path).map_err(|error| io_error(&path, error))?;
    let mut hasher = digest::Context::new(&digest::SHA256);
    let read = hash(&mut reader, u64::MAX, &mut hasher, cancel, &path)?;
    if read != file.size || hex(hasher.finish().as_ref()) != file.sha256 {
        return Ok(false);
    }
    write_record(file, dir)?;
    Ok(true)
}

enum Failure {
    /// Worth trying again from where the file on disk ends.
    Broken(String),
    Fatal(Error),
}

impl From<Error> for Failure {
    fn from(error: Error) -> Self {
        Self::Fatal(error)
    }
}

fn failure(error: ureq::Error) -> Failure {
    use ureq::Error as E;
    match error {
        E::Io(_)
        | E::Timeout(_)
        | E::HostNotFound
        | E::ConnectionFailed
        | E::Protocol(_)
        | E::BodyStalled => Failure::Broken(error.to_string()),
        E::RequireHttpsOnly(url) => Failure::Fatal(Error::NotHttps(url)),
        other => Failure::Fatal(Error::Connection(other.to_string())),
    }
}

/// The file being downloaded, before it is checked, and the hash of what it holds so far.
struct Part {
    file: File,
    path: PathBuf,
    hasher: digest::Context,
    received: u64,
    /// Every byte written to it in this download, also those a restart threw away.
    appended: u64,
}

impl Part {
    /// Opens what an earlier download left, or an empty file, and hashes what is there.
    fn open(path: &Path, size: u64, cancel: &Cancel) -> Result<Self, Error> {
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)
            .map_err(|error| io_error(path, error))?;
        let mut part = Self {
            file,
            path: path.to_path_buf(),
            hasher: digest::Context::new(&digest::SHA256),
            received: 0,
            appended: 0,
        };
        let len = part.file.metadata().map_err(|error| part.io(error))?.len();
        if len > size {
            part.restart()?;
            return Ok(part);
        }
        file = part.file.try_clone().map_err(|error| part.io(error))?;
        part.received = hash(&mut file, len, &mut part.hasher, cancel, path)?;
        part.file
            .seek(SeekFrom::Start(part.received))
            .map_err(|error| part.io(error))?;
        Ok(part)
    }

    fn restart(&mut self) -> Result<(), Error> {
        self.file.set_len(0).map_err(|error| self.io(error))?;
        self.file
            .seek(SeekFrom::Start(0))
            .map_err(|error| self.io(error))?;
        self.hasher = digest::Context::new(&digest::SHA256);
        self.received = 0;
        Ok(())
    }

    fn append(&mut self, bytes: &[u8]) -> Result<(), Error> {
        self.file.write_all(bytes).map_err(|error| self.io(error))?;
        self.hasher.update(bytes);
        self.received += bytes.len() as u64;
        self.appended += bytes.len() as u64;
        Ok(())
    }

    /// Flushes the file to disk and returns its SHA-256.
    fn finish(self) -> Result<String, Error> {
        self.file.sync_all().map_err(|error| self.io(error))?;
        Ok(hex(self.hasher.finish().as_ref()))
    }

    fn io(&self, error: io::Error) -> Error {
        io_error(&self.path, error)
    }
}

/// Sends progress a few times a second, and always when the whole file is there.
struct Progress<'a> {
    on_event: &'a mut dyn FnMut(Event),
    total: u64,
    last: Instant,
}

impl<'a> Progress<'a> {
    fn start(on_event: &'a mut dyn FnMut(Event), received: u64, total: u64) -> Self {
        on_event(Event::Progress { received, total });
        Self {
            on_event,
            total,
            last: Instant::now(),
        }
    }

    fn update(&mut self, received: u64) {
        if received == self.total || self.last.elapsed() >= PROGRESS_INTERVAL {
            let total = self.total;
            self.send(Event::Progress { received, total });
        }
    }

    fn send(&mut self, event: Event) {
        (self.on_event)(event);
        self.last = Instant::now();
    }
}

/// Feeds what the reader holds, up to the limit, to the hash, and returns how many bytes that was.
fn hash(
    reader: &mut impl Read,
    limit: u64,
    hasher: &mut digest::Context,
    cancel: &Cancel,
    path: &Path,
) -> Result<u64, Error> {
    let mut buffer = vec![0; 1024 * 1024];
    let mut read = 0;
    while read < limit {
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        let want =
            usize::try_from(limit - read).map_or(buffer.len(), |left| left.min(buffer.len()));
        match reader.read(&mut buffer[..want]) {
            Ok(0) => break,
            Ok(n) => {
                hasher.update(&buffer[..n]);
                read += n as u64;
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(io_error(path, error)),
        }
    }
    Ok(read)
}

fn write_record(file: &ModelFile, dir: &Path) -> Result<(), Error> {
    let record = status::record_path(file, dir);
    fs::write(&record, format!("{}\n", file.sha256)).map_err(|error| io_error(&record, error))
}

/// Whether the URL is plain HTTP to this machine; any other URL must be HTTPS.
fn is_loopback_http(url: &str) -> Result<bool, Error> {
    let not_https = || Error::NotHttps(url.to_owned());
    let uri: ureq::http::Uri = url.parse().map_err(|_| not_https())?;
    match uri.scheme_str() {
        Some("https") => Ok(false),
        Some("http") if uri.host().is_some_and(is_loopback) => Ok(true),
        _ => Err(not_https()),
    }
}

fn is_loopback(host: &str) -> bool {
    host.eq_ignore_ascii_case("localhost")
        || host
            .trim_start_matches('[')
            .trim_end_matches(']')
            .parse::<IpAddr>()
            .is_ok_and(|ip| ip.is_loopback())
}

fn content_length(response: &ureq::http::Response<ureq::Body>) -> Option<u64> {
    response
        .headers()
        .get("content-length")?
        .to_str()
        .ok()?
        .trim()
        .parse()
        .ok()
}

/// The first byte and the whole size from `Content-Range: bytes <first>-<last>/<size or *>`.
fn content_range(response: &ureq::http::Response<ureq::Body>) -> Option<(u64, Option<u64>)> {
    let value = response.headers().get("content-range")?.to_str().ok()?;
    let (span, total) = value.trim().strip_prefix("bytes ")?.split_once('/')?;
    let first = span.split_once('-')?.0.trim().parse().ok()?;
    let total = match total.trim() {
        "*" => None,
        total => Some(total.parse().ok()?),
    };
    Some((first, total))
}

/// Sleeps for the delay, waking early when the download is cancelled.
fn wait(delay: Duration, cancel: &Cancel) -> Result<(), Error> {
    let end = Instant::now() + delay;
    loop {
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        let now = Instant::now();
        if now >= end {
            return Ok(());
        }
        thread::sleep((end - now).min(Duration::from_millis(50)));
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn io_error(path: &Path, error: io::Error) -> Error {
    Error::Io {
        path: path.to_path_buf(),
        error,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn takes_https_and_plain_http_only_to_this_machine() {
        assert!(!is_loopback_http("https://huggingface.co/x").unwrap());
        assert!(is_loopback_http("http://127.0.0.1:8080/x").unwrap());
        assert!(is_loopback_http("http://localhost/x").unwrap());
        assert!(is_loopback_http("http://[::1]:9/x").unwrap());
        for url in [
            "http://huggingface.co/x",
            "http://127.0.0.1.example.org/x",
            "ftp://127.0.0.1/x",
            "not a url",
        ] {
            assert!(
                matches!(is_loopback_http(url), Err(Error::NotHttps(_))),
                "{url}"
            );
        }
    }

    #[test]
    fn hashes_as_sha256() {
        let mut hasher = digest::Context::new(&digest::SHA256);
        hasher.update(b"abc");
        assert_eq!(
            hex(hasher.finish().as_ref()),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
