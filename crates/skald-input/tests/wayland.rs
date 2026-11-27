//! Insertion on a real Wayland compositor: the clipboard through data-control and Ctrl+V through wtype.
//! The test needs a session with both protocols, and wl-clipboard, wtype and wev to look with; CI runs it on
//! a headless sway (`.github/workflows/linux.yml`).

#![cfg(target_os = "linux")]

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use skald_input::{InsertOptions, Inserted, Inserter};

const SENSITIVE: &str = "x-kde-passwordManagerHint";

fn copy(mime: &str, data: &str) {
    let mut child = Command::new("wl-copy")
        .args(["--type", mime])
        .stdin(Stdio::piped())
        .spawn()
        .expect("wl-copy runs");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(data.as_bytes())
        .unwrap();
    assert!(child.wait().unwrap().success());
}

fn clear() {
    assert!(
        Command::new("wl-copy")
            .arg("--clear")
            .status()
            .unwrap()
            .success()
    );
}

/// The clipboard's data as `mime`, or `None` when it holds nothing of that type.
fn paste(mime: &str) -> Option<String> {
    let output = Command::new("wl-paste")
        .args(["--no-newline", "--type", mime])
        .output()
        .expect("wl-paste runs");
    output
        .status
        .success()
        .then(|| String::from_utf8(output.stdout).unwrap())
}

fn types() -> Vec<String> {
    let output = Command::new("wl-paste")
        .arg("--list-types")
        .output()
        .unwrap();
    String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect()
}

/// What the clipboard looked like while the text was on it: whether it offered the secret marker, and its
/// text, watched from before the insertion starts until `stop`.
fn watch_clipboard(stop: mpsc::Receiver<()>) -> thread::JoinHandle<(bool, Vec<String>)> {
    thread::spawn(move || {
        let (mut marked, mut texts) = (false, Vec::new());
        while stop.try_recv().is_err() {
            let offered = types();
            marked |= offered.iter().any(|mime| mime == SENSITIVE);
            if let Some(text) = paste("text/plain")
                && texts.last() != Some(&text)
            {
                texts.push(text);
            }
            thread::sleep(Duration::from_millis(40));
        }
        (marked, texts)
    })
}

/// A window in front that writes the keys it gets to a file.
struct KeyWindow {
    child: Child,
    log: PathBuf,
}

impl KeyWindow {
    fn open() -> Self {
        let log = std::env::temp_dir().join(format!("skald-wev-{}.log", std::process::id()));
        let child = Command::new("stdbuf")
            .args(["-oL", "wev"])
            .stdout(fs::File::create(&log).unwrap())
            .stderr(Stdio::null())
            .spawn()
            .expect("wev runs");
        let window = Self { child, log };
        let opened = Instant::now();
        // A seat without a keyboard gives keyboard focus only once wtype brings one, so the window waits
        // to be the active one.
        while !window.text().contains("activated") {
            assert!(
                opened.elapsed() < Duration::from_secs(5),
                "wev was not activated"
            );
            thread::sleep(Duration::from_millis(50));
        }
        window
    }

    fn text(&self) -> String {
        fs::read_to_string(&self.log).unwrap_or_default()
    }
}

impl Drop for KeyWindow {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = fs::remove_file(&self.log);
    }
}

// One test, since every part shares the session's one clipboard.
#[test]
#[ignore = "needs a Wayland compositor with data-control and a virtual keyboard, wl-clipboard, wtype and wev"]
fn text_is_pasted_with_ctrl_v_and_the_previous_clipboard_put_back() {
    let window = KeyWindow::open();
    let mut inserter = Inserter::new();
    let options = InsertOptions::default();

    // Every type the previous contents had comes back, after the text was offered as a secret.
    copy("text/html", "<b>før</b>");
    let (stop, stopped) = mpsc::channel();
    let watching = watch_clipboard(stopped);
    thread::sleep(Duration::from_millis(200));
    assert_eq!(
        inserter.insert("Hei, dette er en prøve.", &options),
        Ok(Inserted::PreviousRestored)
    );
    stop.send(()).unwrap();
    let (marked, texts) = watching.join().unwrap();
    assert!(marked, "the text was offered as a secret");
    assert!(
        texts.iter().any(|text| text == "Hei, dette er en prøve. "),
        "{texts:?}"
    );
    assert_eq!(paste("text/html").as_deref(), Some("<b>før</b>"));
    assert!(!types().iter().any(|mime| mime == SENSITIVE));
    let keys = window.text();
    assert!(
        keys.contains("sym: v") && keys.contains("Control"),
        "the window got Ctrl+V:\n{keys}"
    );

    // Contents Skald put back are read from Skald itself, and put back again.
    assert_eq!(
        inserter.insert("Igjen", &options),
        Ok(Inserted::PreviousRestored)
    );
    assert_eq!(paste("text/html").as_deref(), Some("<b>før</b>"));

    // The text stays when the user chose to copy it.
    let keep = InsertOptions {
        keep_text_on_clipboard: true,
        ..InsertOptions::default()
    };
    assert_eq!(inserter.insert("Hei", &keep), Ok(Inserted::TextKept));
    assert_eq!(paste("text/plain").as_deref(), Some("Hei "));
    assert!(!types().iter().any(|mime| mime == SENSITIVE));

    // A secret is neither read nor put back.
    copy(SENSITIVE, "secret");
    assert_eq!(
        inserter.insert("Hei", &options),
        Ok(Inserted::PreviousMarkedPrivate)
    );
    assert_eq!(paste("text/plain").as_deref(), Some("Hei "));

    // Something copied while the text is on the clipboard stays.
    copy("text/plain", "før");
    let copier = thread::spawn(|| {
        thread::sleep(Duration::from_millis(300));
        copy("text/plain", "annet");
    });
    assert_eq!(
        inserter.insert("Hei", &options),
        Ok(Inserted::CopiedMeanwhile)
    );
    copier.join().unwrap();
    assert_eq!(paste("text/plain").as_deref(), Some("annet"));

    // An empty clipboard is empty again afterwards.
    clear();
    assert_eq!(
        inserter.insert("Hei", &options),
        Ok(Inserted::PreviousRestored)
    );
    assert_eq!(paste("text/plain"), None);
}
