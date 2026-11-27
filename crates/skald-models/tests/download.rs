//! Downloads from an HTTP server on this machine, scripted per connection: serving the file, breaking off,
//! hanging up, ignoring a range, redirecting.

use std::borrow::Cow;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use skald_models::{Cancel, Downloader, Error, Event, ModelFile, Status, remove, status};

#[derive(Debug, Clone, Copy)]
enum Answer {
    /// The file, or the part of it a range asks for.
    Serve,
    /// The whole file, whatever range was asked for.
    IgnoreRange,
    /// The headers for the answer `Serve` gives, this many bytes of its body, and then the connection closes.
    BreakAfter(usize),
    /// The answer `Serve` gives, 64 KiB at a time with a pause between.
    Slowly,
    /// Closes the connection without answering.
    HangUp,
    /// Sends the client to another path on this server.
    Redirect,
}

/// What a request asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Asked {
    path: String,
    range_from: Option<usize>,
}

struct Server {
    url: String,
    asked: Arc<Mutex<Vec<Asked>>>,
}

impl Server {
    /// Answers each connection with the next answer in `script`; the last one answers every connection after.
    fn start(data: &[u8], script: &[Answer]) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/ggml-model.bin", listener.local_addr().unwrap());
        let asked = Arc::new(Mutex::new(Vec::new()));
        let data = data.to_vec();
        let script = script.to_vec();
        let log = Arc::clone(&asked);
        thread::spawn(move || {
            for (i, stream) in listener.incoming().enumerate() {
                let Ok(stream) = stream else { continue };
                let answer = script[i.min(script.len() - 1)];
                answer_one(stream, &data, answer, &log);
            }
        });
        Self { url, asked }
    }

    fn asked(&self) -> Vec<Asked> {
        self.asked.lock().unwrap().clone()
    }

    fn ranges(&self) -> Vec<Option<usize>> {
        self.asked()
            .into_iter()
            .map(|asked| asked.range_from)
            .collect()
    }
}

fn answer_one(mut stream: TcpStream, data: &[u8], answer: Answer, log: &Mutex<Vec<Asked>>) {
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).is_err() {
        return;
    }
    let path = request_line
        .split_whitespace()
        .nth(1)
        .unwrap_or_default()
        .to_owned();
    let mut range_from = None;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).is_err() || line.trim().is_empty() {
            break;
        }
        let lower = line.to_ascii_lowercase();
        if let Some(value) = lower.strip_prefix("range: bytes=") {
            range_from = value.trim().trim_end_matches('-').parse().ok();
        }
    }
    log.lock().unwrap().push(Asked {
        path: path.clone(),
        range_from,
    });

    let from = match answer {
        Answer::HangUp => return,
        Answer::Redirect => {
            let _ = write!(
                stream,
                "HTTP/1.1 302 Found\r\nLocation: /cdn{path}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            );
            return;
        }
        Answer::IgnoreRange => 0,
        Answer::Serve | Answer::BreakAfter(_) | Answer::Slowly => range_from.unwrap_or(0),
    };
    if from >= data.len() {
        let _ = write!(
            stream,
            "HTTP/1.1 416 Range Not Satisfiable\r\nContent-Range: bytes */{}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            data.len()
        );
        return;
    }
    let body = &data[from..];
    let head = if from > 0 {
        format!(
            "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {from}-{}/{}\r\nConnection: close\r\n\r\n",
            body.len(),
            data.len() - 1,
            data.len()
        )
    } else {
        format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )
    };
    if stream.write_all(head.as_bytes()).is_err() {
        return;
    }
    // A client that stops reading (cancelled) makes these writes fail; that ends the answer.
    match answer {
        Answer::BreakAfter(bytes) => {
            let _ = stream.write_all(&body[..bytes.min(body.len())]);
            let _ = stream.flush();
            let _ = stream.shutdown(Shutdown::Both);
        }
        Answer::Slowly => {
            for chunk in body.chunks(64 * 1024) {
                if stream.write_all(chunk).is_err() {
                    return;
                }
                thread::sleep(Duration::from_millis(20));
            }
        }
        _ => {
            let _ = stream.write_all(body);
        }
    }
}

/// A megabyte and a little, not a whole number of chunks, that no two tests mistake for each other.
fn data() -> Vec<u8> {
    let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
    (0..1_000_003)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state as u8
        })
        .collect()
}

fn sha256(bytes: &[u8]) -> String {
    ring::digest::digest(&ring::digest::SHA256, bytes)
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn pinned(server: &Server, data: &[u8]) -> ModelFile {
    ModelFile {
        name: Cow::Borrowed("ggml-model.bin"),
        url: Cow::Owned(server.url.clone()),
        sha256: Cow::Owned(sha256(data)),
        size: data.len() as u64,
    }
}

/// An empty folder of the test's own, removed when the test ends.
struct Folder(PathBuf);

impl Folder {
    fn new(test: &str) -> Self {
        let root = std::env::temp_dir().join(format!("skald-models-{test}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        Self(root)
    }

    /// The model's folder inside it, which the download makes.
    fn model_dir(&self) -> PathBuf {
        self.0.join("nb-whisper-large")
    }
}

impl Drop for Folder {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn names_in(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .map(|entries| {
            entries
                .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

fn quick() -> Downloader {
    Downloader::new().retries(3, Duration::ZERO)
}

#[test]
fn downloads_a_whole_file() {
    let data = data();
    let server = Server::start(&data, &[Answer::Serve]);
    let file = pinned(&server, &data);
    let folder = Folder::new("whole");
    let dir = folder.model_dir();
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("ggml-model.bin.deadbeef.part"),
        b"left by another pin",
    )
    .unwrap();

    let mut events = Vec::new();
    let path = quick()
        .download(&file, &dir, &Cancel::new(), |event| events.push(event))
        .unwrap();

    assert_eq!(path, dir.join("ggml-model.bin"));
    assert_eq!(fs::read(&path).unwrap(), data);
    assert_eq!(status(&file, &dir), Status::Ready);
    assert_eq!(names_in(&dir), ["ggml-model.bin", "ggml-model.bin.sha256"]);
    assert_eq!(server.ranges(), [None]);

    let total = data.len() as u64;
    assert_eq!(
        events.first(),
        Some(&Event::Progress { received: 0, total })
    );
    assert_eq!(
        events.last(),
        Some(&Event::Progress {
            received: total,
            total
        })
    );
    let received: Vec<u64> = events
        .iter()
        .map(|event| match event {
            Event::Progress { received, .. } => *received,
            Event::Retrying { .. } => panic!("no retry expected: {event:?}"),
        })
        .collect();
    assert!(received.is_sorted(), "{received:?}");

    // There already, checked: nothing is fetched again.
    quick()
        .download(&file, &dir, &Cancel::new(), |_| {})
        .unwrap();
    assert_eq!(server.ranges(), [None]);
}

#[test]
fn resumes_a_download_that_stopped() {
    let data = data();
    let server = Server::start(&data, &[Answer::BreakAfter(400_000), Answer::Serve]);
    let file = pinned(&server, &data);
    let folder = Folder::new("resume");
    let dir = folder.model_dir();
    let no_retries = Downloader::new().retries(0, Duration::ZERO);

    let first = no_retries.download(&file, &dir, &Cancel::new(), |_| {});
    assert!(matches!(first, Err(Error::Connection(_))), "{first:?}");
    assert_eq!(status(&file, &dir), Status::Partial { received: 400_000 });

    let mut events = Vec::new();
    no_retries
        .download(&file, &dir, &Cancel::new(), |event| events.push(event))
        .unwrap();
    assert_eq!(fs::read(dir.join("ggml-model.bin")).unwrap(), data);
    assert_eq!(server.ranges(), [None, Some(400_000)]);
    assert_eq!(
        events.first(),
        Some(&Event::Progress {
            received: 400_000,
            total: data.len() as u64
        })
    );
}

#[test]
fn picks_up_a_broken_connection_by_itself() {
    let data = data();
    let server = Server::start(
        &data,
        &[
            Answer::BreakAfter(300_000),
            Answer::BreakAfter(300_000),
            Answer::Serve,
        ],
    );
    let file = pinned(&server, &data);
    let folder = Folder::new("broken");
    let dir = folder.model_dir();

    let mut retries = Vec::new();
    quick()
        .download(&file, &dir, &Cancel::new(), |event| {
            if let Event::Retrying { .. } = event {
                retries.push(event);
            }
        })
        .unwrap();

    assert_eq!(fs::read(dir.join("ggml-model.bin")).unwrap(), data);
    assert_eq!(server.ranges(), [None, Some(300_000), Some(600_000)]);
    // Each try brought more of the file, so the count started over each time.
    assert_eq!(retries, [Event::Retrying { attempt: 1, of: 3 }; 2]);
}

#[test]
fn gives_up_when_the_server_keeps_hanging_up() {
    let data = data();
    let server = Server::start(&data, &[Answer::HangUp]);
    let file = pinned(&server, &data);
    let folder = Folder::new("hang-up");
    let dir = folder.model_dir();

    let mut retries = Vec::new();
    let result = Downloader::new().retries(2, Duration::ZERO).download(
        &file,
        &dir,
        &Cancel::new(),
        |event| {
            if let Event::Retrying { attempt, .. } = event {
                retries.push(attempt);
            }
        },
    );

    assert!(matches!(result, Err(Error::Connection(_))), "{result:?}");
    assert_eq!(retries, [1, 2]);
    assert_eq!(server.asked().len(), 3);
    assert_eq!(status(&file, &dir), Status::Missing);
}

#[test]
fn deletes_a_file_whose_hash_does_not_match() {
    let data = data();
    let server = Server::start(&data, &[Answer::Serve]);
    let mut file = pinned(&server, &data);
    let expected = sha256(b"another file");
    file.sha256 = Cow::Owned(expected.clone());
    let folder = Folder::new("hash");
    let dir = folder.model_dir();

    let result = quick().download(&file, &dir, &Cancel::new(), |_| {});

    match result {
        Err(Error::HashMismatch {
            expected: wanted,
            actual,
        }) => {
            assert_eq!(wanted, expected);
            assert_eq!(actual, sha256(&data));
        }
        other => panic!("expected a hash mismatch, got {other:?}"),
    }
    assert_eq!(status(&file, &dir), Status::Missing);
    assert!(names_in(&dir).is_empty(), "{:?}", names_in(&dir));
}

#[test]
fn cancelling_keeps_what_arrived_for_the_next_download() {
    let data = data();
    let server = Server::start(&data, &[Answer::Slowly, Answer::Serve]);
    let file = pinned(&server, &data);
    let folder = Folder::new("cancel");
    let dir = folder.model_dir();

    let cancel = Cancel::new();
    let result = quick().download(&file, &dir, &cancel, |event| {
        if let Event::Progress { received, .. } = event
            && received > 0
        {
            cancel.cancel();
        }
    });
    assert!(matches!(result, Err(Error::Cancelled)), "{result:?}");
    let Status::Partial { received } = status(&file, &dir) else {
        panic!("expected part of the file, got {:?}", status(&file, &dir));
    };
    assert!(received > 0 && received < data.len() as u64, "{received}");

    quick()
        .download(&file, &dir, &Cancel::new(), |_| {})
        .unwrap();
    assert_eq!(fs::read(dir.join("ggml-model.bin")).unwrap(), data);
    assert_eq!(server.ranges(), [None, Some(received as usize)]);
}

#[test]
fn refuses_a_file_larger_than_the_free_space() {
    let data = data();
    let server = Server::start(&data, &[Answer::Serve]);
    let mut file = pinned(&server, &data);
    file.size = 1 << 60;
    let folder = Folder::new("space");
    let dir = folder.model_dir();

    let result = quick().download(&file, &dir, &Cancel::new(), |_| {});

    match result {
        Err(Error::NotEnoughSpace { needed, available }) => {
            assert_eq!(needed, 1 << 60);
            assert!(available < needed);
        }
        other => panic!("expected too little space, got {other:?}"),
    }
    assert!(server.asked().is_empty());
}

#[test]
fn starts_over_when_the_server_ignores_the_range() {
    let data = data();
    let server = Server::start(&data, &[Answer::BreakAfter(300_000), Answer::IgnoreRange]);
    let file = pinned(&server, &data);
    let folder = Folder::new("ignore-range");
    let dir = folder.model_dir();

    quick()
        .download(&file, &dir, &Cancel::new(), |_| {})
        .unwrap();

    assert_eq!(fs::read(dir.join("ggml-model.bin")).unwrap(), data);
    assert_eq!(server.ranges(), [None, Some(300_000)]);
}

#[test]
fn keeps_the_range_through_a_redirect() {
    let data = data();
    let server = Server::start(
        &data,
        &[
            Answer::Redirect,
            Answer::BreakAfter(300_000),
            Answer::Redirect,
            Answer::Serve,
        ],
    );
    let file = pinned(&server, &data);
    let folder = Folder::new("redirect");
    let dir = folder.model_dir();

    quick()
        .download(&file, &dir, &Cancel::new(), |_| {})
        .unwrap();

    assert_eq!(fs::read(dir.join("ggml-model.bin")).unwrap(), data);
    let asked = server.asked();
    let paths: Vec<&str> = asked.iter().map(|asked| asked.path.as_str()).collect();
    assert_eq!(
        paths,
        [
            "/ggml-model.bin",
            "/cdn/ggml-model.bin",
            "/ggml-model.bin",
            "/cdn/ggml-model.bin"
        ]
    );
    assert_eq!(server.ranges(), [None, None, Some(300_000), Some(300_000)]);
}

#[test]
fn rejects_a_server_file_of_another_size() {
    let data = data();
    let server = Server::start(&data, &[Answer::Serve]);
    let mut file = pinned(&server, &data);
    file.size += 1;
    let folder = Folder::new("size");
    let dir = folder.model_dir();

    let result = quick().download(&file, &dir, &Cancel::new(), |_| {});

    match result {
        Err(Error::WrongSize { expected, actual }) => {
            assert_eq!(expected, data.len() as u64 + 1);
            assert_eq!(actual, data.len() as u64);
        }
        other => panic!("expected the wrong size, got {other:?}"),
    }
    assert_eq!(status(&file, &dir), Status::Missing);
}

#[test]
fn checks_a_file_put_there_by_hand_before_fetching() {
    let data = data();
    let server = Server::start(&data, &[Answer::Serve]);
    let file = pinned(&server, &data);
    let folder = Folder::new("by-hand");
    let dir = folder.model_dir();
    fs::create_dir_all(&dir).unwrap();

    fs::write(dir.join("ggml-model.bin"), &data).unwrap();
    assert_eq!(status(&file, &dir), Status::Unchecked);
    quick()
        .download(&file, &dir, &Cancel::new(), |_| {})
        .unwrap();
    assert_eq!(status(&file, &dir), Status::Ready);
    assert!(server.asked().is_empty());

    // Another file of the same size is replaced by the pinned one.
    fs::remove_file(dir.join("ggml-model.bin.sha256")).unwrap();
    fs::write(dir.join("ggml-model.bin"), vec![0; data.len()]).unwrap();
    quick()
        .download(&file, &dir, &Cancel::new(), |_| {})
        .unwrap();
    assert_eq!(fs::read(dir.join("ggml-model.bin")).unwrap(), data);
    assert_eq!(status(&file, &dir), Status::Ready);
    assert_eq!(server.ranges(), [None]);
}

#[test]
fn removing_a_model_deletes_a_stopped_or_finished_download() {
    let data = data();
    let server = Server::start(&data, &[Answer::BreakAfter(300_000), Answer::Serve]);
    let file = pinned(&server, &data);
    let folder = Folder::new("remove");
    let dir = folder.model_dir();
    let no_retries = Downloader::new().retries(0, Duration::ZERO);

    let stopped = no_retries.download(&file, &dir, &Cancel::new(), |_| {});
    assert!(matches!(stopped, Err(Error::Connection(_))), "{stopped:?}");
    remove(&file, &dir).unwrap();
    assert_eq!(status(&file, &dir), Status::Missing);
    assert!(!dir.exists());

    no_retries
        .download(&file, &dir, &Cancel::new(), |_| {})
        .unwrap();
    assert_eq!(status(&file, &dir), Status::Ready);
    remove(&file, &dir).unwrap();
    assert_eq!(status(&file, &dir), Status::Missing);
    assert!(!dir.exists());
    // The part was gone, so the download after it started from the beginning.
    assert_eq!(server.ranges(), [None, None]);
}

#[test]
fn never_fetches_over_plain_http_from_another_machine() {
    let file = ModelFile {
        name: Cow::Borrowed("ggml-model.bin"),
        url: Cow::Borrowed("http://example.org/ggml-model.bin"),
        sha256: Cow::Owned(sha256(b"")),
        size: 1,
    };
    let folder = Folder::new("plain-http");
    let dir = folder.model_dir();
    let result = quick().download(&file, &dir, &Cancel::new(), |_| {});
    assert!(matches!(result, Err(Error::NotHttps(_))), "{result:?}");
}
