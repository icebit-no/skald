//! The named pipe: one Skald at a time, a pipe of its own per folder, and requests and answers over it.
//! Who may open it is tested beside the pipe's code, which reads its access list.

#![cfg(windows)]

mod common;

use std::sync::Arc;
use std::thread;

use common::{FakeApp, Folder};
use skald_control::protocol::{CorrectionAdded, Output};
use skald_control::{BindError, Client, ErrorCode, Request, Server};
use skald_core::Config;

fn app() -> Arc<FakeApp> {
    let mut config = Config::default();
    config.corrections.add("mercha", "merget").unwrap();
    Arc::new(FakeApp::new(config, Default::default(), Default::default()))
}

#[test]
fn no_pipe_means_skald_is_not_running() {
    let folder = Folder::new();
    assert!(
        Client::connect(&folder.path.join("skald.sock"))
            .unwrap()
            .is_none()
    );
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
fn a_pipe_bound_but_not_yet_answering_holds_its_name() {
    let folder = Folder::new();
    let socket = folder.path.join("skald.sock");
    let _server = Server::bind(&socket).unwrap();
    assert!(matches!(
        Server::bind(&socket),
        Err(BindError::AlreadyRunning)
    ));
}

#[test]
fn each_folder_has_a_pipe_of_its_own() {
    let (first, second) = (Folder::new(), Folder::new());
    let first = Server::bind(&first.path.join("skald.sock"))
        .unwrap()
        .serve(app());
    let second = Server::bind(&second.path.join("skald.sock"))
        .unwrap()
        .serve(app());
    first.stop();
    second.stop();
}

#[test]
fn stopping_takes_the_pipe_away() {
    let folder = Folder::new();
    let socket = folder.path.join("skald.sock");
    let serving = Server::bind(&socket).unwrap().serve(app());
    serving.stop();
    assert!(Client::connect(&socket).unwrap().is_none());

    let serving = Server::bind(&socket).unwrap().serve(app());
    drop(serving);
    assert!(
        Client::connect(&socket).unwrap().is_none(),
        "dropping stops it too"
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
    assert!(matches!(
        client.send(&Request::FixList).unwrap(),
        Output::Corrections(list) if list.corrections.len() == 2
    ));
    let error = client
        .send(&Request::FixRemove {
            from: "finnes ikke".into(),
        })
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::NotFound);

    assert_eq!(app.config.lock().unwrap().corrections.rules.len(), 2);
    serving.stop();
}

#[test]
fn several_clients_are_answered_at_once() {
    let folder = Folder::new();
    let socket = folder.path.join("skald.sock");
    let serving = Server::bind(&socket).unwrap().serve(app());
    // A connection left open does not keep the next one out.
    let mut idle = Client::connect(&socket).unwrap().unwrap();

    let clients: Vec<_> = (0..4)
        .map(|_| {
            let socket = socket.clone();
            thread::spawn(move || {
                let mut client = Client::connect(&socket).unwrap().unwrap();
                for _ in 0..5 {
                    assert!(matches!(
                        client.send(&Request::FixList).unwrap(),
                        Output::Corrections(_)
                    ));
                }
            })
        })
        .collect();
    for client in clients {
        client.join().unwrap();
    }
    assert!(matches!(
        idle.send(&Request::FixList).unwrap(),
        Output::Corrections(_)
    ));
    serving.stop();
}
