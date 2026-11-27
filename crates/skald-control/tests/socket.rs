//! The socket: who may open it, a socket left by a crashed Skald, and requests and answers over it.

#![cfg(unix)]

mod common;

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::{FileTypeExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::Arc;

use common::{FakeApp, Folder};
use skald_control::protocol::{CorrectionAdded, CorrectionCount, CorrectionList, Output};
use skald_control::{BindError, Client, ErrorCode, Request, Server};
use skald_core::Config;

fn mode(path: &std::path::Path) -> u32 {
    fs::symlink_metadata(path).unwrap().permissions().mode() & 0o777
}

fn app() -> Arc<FakeApp> {
    let mut config = Config::default();
    config.corrections.add("mercha", "merget").unwrap();
    Arc::new(FakeApp::new(config, Default::default(), Default::default()))
}

#[test]
fn only_the_user_can_open_the_socket() {
    let folder = Folder::new();
    let runtime = folder.path.join("run");
    fs::create_dir_all(&runtime).unwrap();
    fs::set_permissions(&runtime, fs::Permissions::from_mode(0o755)).unwrap();
    let socket = runtime.join("skald.sock");

    let server = Server::bind(&socket).unwrap();
    assert_eq!(mode(&runtime), 0o700, "the folder");
    assert!(
        fs::symlink_metadata(&socket)
            .unwrap()
            .file_type()
            .is_socket()
    );
    assert_eq!(mode(&socket), 0o600, "the socket");
    drop(server);
}

#[test]
fn makes_the_folder_when_it_is_missing() {
    let folder = Folder::new();
    let socket = folder.path.join("new/skald.sock");
    let _server = Server::bind(&socket).unwrap();
    assert_eq!(mode(&folder.path.join("new")), 0o700);
}

#[test]
fn a_socket_left_by_a_crashed_skald_is_replaced() {
    let folder = Folder::new();
    let socket = folder.path.join("skald.sock");
    // A listener dropped without removing its file is what a crash leaves behind.
    drop(UnixListener::bind(&socket).unwrap());
    assert!(socket.exists());
    assert!(
        Client::connect(&socket).unwrap().is_none(),
        "nobody answers"
    );

    let serving = Server::bind(&socket).unwrap().serve(app());
    let mut client = Client::connect(&socket)
        .unwrap()
        .expect("the new Skald answers");
    assert!(matches!(
        client.send(&Request::FixList).unwrap(),
        Output::Corrections(_)
    ));
    serving.stop();
}

#[test]
fn a_second_skald_finds_the_first_running() {
    let folder = Folder::new();
    let socket = folder.path.join("skald.sock");
    let serving = Server::bind(&socket).unwrap().serve(app());
    assert!(matches!(
        Server::bind(&socket),
        Err(BindError::AlreadyRunning)
    ));
    // The first one still answers.
    assert!(Client::connect(&socket).unwrap().is_some());
    serving.stop();
}

#[test]
fn something_else_at_the_path_is_left_alone() {
    let folder = Folder::new();
    let socket = folder.path.join("skald.sock");
    fs::write(&socket, "mine").unwrap();
    assert!(matches!(
        Server::bind(&socket),
        Err(BindError::NotASocket(path)) if path == socket
    ));
    assert_eq!(fs::read_to_string(&socket).unwrap(), "mine");
}

#[test]
fn stopping_removes_the_socket() {
    let folder = Folder::new();
    let socket = folder.path.join("skald.sock");
    let serving = Server::bind(&socket).unwrap().serve(app());
    assert!(socket.exists());
    serving.stop();
    assert!(!socket.exists());
    assert!(Client::connect(&socket).unwrap().is_none());

    let serving = Server::bind(&socket).unwrap().serve(app());
    drop(serving);
    assert!(!socket.exists(), "dropping stops it too");
}

#[test]
fn no_socket_means_skald_is_not_running() {
    let folder = Folder::new();
    assert!(
        Client::connect(&folder.path.join("skald.sock"))
            .unwrap()
            .is_none()
    );
}

#[test]
fn one_connection_carries_several_requests_and_changes_reach_the_app() {
    let folder = Folder::new();
    let socket = folder.path.join("skald.sock");
    let app = app();
    let serving = Server::bind(&socket).unwrap().serve(app.clone());
    let mut client = Client::connect(&socket).unwrap().unwrap();

    let added = client
        .send(&Request::FixAdd {
            from: "Post gres SQL".into(),
            to: "PostgreSQL".into(),
        })
        .unwrap();
    assert_eq!(
        added,
        Output::CorrectionAdded(CorrectionAdded {
            from: "Post gres SQL".into(),
            to: "PostgreSQL".into(),
            previous: None,
        })
    );
    assert_eq!(
        client.send(&Request::FixList).unwrap(),
        Output::Corrections(CorrectionList {
            corrections: vec![
                CorrectionCount {
                    from: "mercha".into(),
                    to: "merget".into(),
                    applied: 0,
                },
                CorrectionCount {
                    from: "Post gres SQL".into(),
                    to: "PostgreSQL".into(),
                    applied: 0,
                },
            ],
        })
    );
    let error = client
        .send(&Request::FixRemove {
            from: "finnes ikke".into(),
        })
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::NotFound);
    assert_eq!(error.message, "Fant ingen retting for «finnes ikke».");

    assert_eq!(app.config.lock().unwrap().corrections.rules.len(), 2);
    assert_eq!(app.changes.load(std::sync::atomic::Ordering::SeqCst), 1);
    serving.stop();
}

fn raw_exchange(socket: &std::path::Path, line: &str) -> String {
    let mut stream = UnixStream::connect(socket).unwrap();
    stream.write_all(line.as_bytes()).unwrap();
    stream.write_all(b"\n").unwrap();
    let mut answer = String::new();
    BufReader::new(stream).read_line(&mut answer).unwrap();
    answer
}

#[test]
fn a_client_of_another_version_is_told_to_restart_skald() {
    let folder = Folder::new();
    let socket = folder.path.join("skald.sock");
    let serving = Server::bind(&socket).unwrap().serve(app());
    let answer = raw_exchange(&socket, r#"{"v":2,"request":{"command":"something_new"}}"#);
    let answer: serde_json::Value = serde_json::from_str(&answer).unwrap();
    assert_eq!(answer["v"], 1);
    assert_eq!(answer["error"]["code"], "version");
    assert!(
        answer["error"]["message"]
            .as_str()
            .unwrap()
            .contains("start den på nytt")
    );
    serving.stop();
}

#[test]
fn a_raw_request_gets_a_raw_answer() {
    let folder = Folder::new();
    let socket = folder.path.join("skald.sock");
    let serving = Server::bind(&socket).unwrap().serve(app());
    assert_eq!(
        raw_exchange(&socket, r#"{"v":1,"request":{"command":"last"}}"#),
        "{\"v\":1,\"ok\":{\"output\":\"last\",\"last\":null}}\n"
    );
    let answer = raw_exchange(&socket, "ikke json");
    assert!(answer.contains(r#""code":"protocol""#), "{answer}");
    serving.stop();
}

#[test]
fn a_line_too_long_closes_the_connection() {
    let folder = Folder::new();
    let socket = folder.path.join("skald.sock");
    let serving = Server::bind(&socket).unwrap().serve(app());
    let mut stream = UnixStream::connect(&socket).unwrap();
    let long = vec![b'a'; (1 << 20) + 10];
    // The server may close while this is still being written.
    let _ = stream.write_all(&long);
    let _ = stream.write_all(b"\n");
    let mut answer = String::new();
    let read = BufReader::new(stream).read_line(&mut answer).unwrap_or(0);
    assert_eq!(read, 0, "{answer}");
    // And it still serves others.
    assert!(Client::connect(&socket).unwrap().is_some());
    serving.stop();
}
