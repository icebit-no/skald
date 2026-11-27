//! Every command as a person or an agent runs it: the JSON each prints, the same whether Skald runs or
//! not, the text a person reads, and the configuration file written directly while Skald is not running.

mod common;

use std::fs;
use std::sync::Arc;

use common::{BUILT_IN, FakeApp, Folder, USB, skald};
use serde_json::json;
use skald_core::Config;
use skald_core::corrections::Counts;
use skald_core::history::{Entry, History};
use skald_core::paths::Paths;
use skald_core::state::State;

const CONFIG: &str = r#"
future_flag = true
hold_key = "right-command"
language = "bokmal"
microphones = ["Shure MV7", "Yeti"]

[vocabulary]
sentence = "Vi merget pull-requesten i går."
words = ["Skald", "PostgreSQL"]
weights = [1, 2]

[[corrections]]
from = "mercha"
to = "merget"
enabled = true

[[corrections]]
from = "Post gres SQL"
to = "PostgreSQL"

[overlay]
position = "top"
"#;

fn state(config: &Config) -> State {
    let mut counts = Counts::default();
    config
        .corrections
        .apply_and_count("Vi mercha den. Han mercha.", &mut counts);
    let mut history = History::default();
    history.push(Entry {
        text: "Hei, dette er en prøve.".into(),
        unix_ms: 1_795_799_000_000,
        microphone: BUILT_IN.into(),
    });
    history.push(Entry {
        text: "Vi merget pull-requesten.".into(),
        unix_ms: 1_795_800_000_000,
        microphone: USB.into(),
    });
    State { history, counts }
}

/// Skald's folder with the configuration and state above, and two audio files.
fn folder() -> (Folder, Paths) {
    let folder = Folder::new();
    let paths = folder.paths();
    fs::write(paths.config_file(), CONFIG).unwrap();
    let config = Config::from_toml(CONFIG).unwrap();
    fs::write(paths.state_file(), state(&config).to_toml().unwrap()).unwrap();
    fs::write(folder.path.join("opptak.wav"), b"").unwrap();
    fs::write(folder.path.join("stille.wav"), b"").unwrap();
    (folder, paths)
}

/// The same folder with Skald running on it, holding the same configuration and state.
#[cfg(unix)]
fn running(paths: &Paths) -> (Arc<FakeApp>, skald_control::Serving) {
    let config = Config::from_toml(CONFIG).unwrap();
    let State { history, counts } = state(&config);
    let app = Arc::new(FakeApp::new(config, history, counts));
    let serving = skald_control::Server::bind(&paths.socket())
        .unwrap()
        .serve(app.clone());
    (app, serving)
}

fn wav(folder: &Folder, name: &str) -> String {
    folder.path.join(name).to_string_lossy().into_owned()
}

/// Each command with the JSON it prints while Skald is not running, in order: later ones see what the
/// earlier ones changed.
fn commands_and_json(folder: &Folder) -> Vec<(Vec<String>, serde_json::Value)> {
    let opptak = wav(folder, "opptak.wav");
    let stille = wav(folder, "stille.wav");
    let cases: Vec<(Vec<&str>, serde_json::Value)> = vec![
        (
            vec!["last"],
            json!({"running": false, "last": {
                "text": "Vi merget pull-requesten.", "unix_ms": 1_795_800_000_000_i64,
                "microphone": USB}}),
        ),
        (
            vec!["history"],
            json!({"running": false, "texts": [
                {"text": "Vi merget pull-requesten.", "unix_ms": 1_795_800_000_000_i64,
                 "microphone": USB},
                {"text": "Hei, dette er en prøve.", "unix_ms": 1_795_799_000_000_i64,
                 "microphone": BUILT_IN},
            ]}),
        ),
        (
            vec!["fix", "list"],
            json!({"running": false, "corrections": [
                {"from": "mercha", "to": "merget", "applied": 2},
                {"from": "Post gres SQL", "to": "PostgreSQL", "applied": 0},
            ]}),
        ),
        (
            vec!["fix", "add", "Mercha", "merge"],
            json!({"running": false, "from": "Mercha", "to": "merge", "previous": "merget"}),
        ),
        (
            vec!["fix", "add", "Hei", "Hallo"],
            json!({"running": false, "from": "Hei", "to": "Hallo", "previous": null}),
        ),
        (
            vec!["fix", "remove", "hei"],
            json!({"running": false, "from": "Hei", "to": "Hallo"}),
        ),
        (
            vec!["vocab", "list"],
            json!({"running": false, "sentence": "Vi merget pull-requesten i går.",
                   "words": ["Skald", "PostgreSQL"], "tokens": null}),
        ),
        (
            vec!["vocab", "add", "merget"],
            json!({"running": false, "word": "merget", "changed": true, "tokens": null}),
        ),
        (
            vec!["vocab", "add", " merget "],
            json!({"running": false, "word": "merget", "changed": false, "tokens": null}),
        ),
        (
            vec!["vocab", "remove", "Skald"],
            json!({"running": false, "word": "Skald", "changed": true, "tokens": null}),
        ),
        (
            vec!["vocab", "sentence", "Vi merget den."],
            json!({"running": false, "sentence": "Vi merget den.",
                   "words": ["PostgreSQL", "merget"], "tokens": null}),
        ),
        (
            vec!["config", "get"],
            json!({"running": false, "settings": {
                "hold_key": "right-command", "language": "bokmal", "overlay_theme": "runepinne",
                "copy_to_clipboard": false, "start_at_login": false, "sounds": true}}),
        ),
        (
            vec!["config", "set", "language", "nynorsk"],
            json!({"running": false, "key": "language", "value": "nynorsk"}),
        ),
        (
            vec!["config", "set", "sounds", "av"],
            json!({"running": false, "key": "sounds", "value": false}),
        ),
        (
            vec!["config", "set", "overlay-theme", "smidd-jern"],
            json!({"running": false, "key": "overlay_theme", "value": "smidd-jern"}),
        ),
        (
            vec!["config", "get", "language"],
            json!({"running": false, "key": "language", "value": "nynorsk"}),
        ),
        (
            vec!["mic", "list"],
            json!({"running": false, "pinned": null, "active": USB, "microphones": [
                {"name": USB, "position": 1, "connected": true, "pinned": false, "active": true},
                {"name": "Yeti", "position": 2, "connected": false, "pinned": false,
                 "active": false},
                {"name": BUILT_IN, "position": null, "connected": true, "pinned": false,
                 "active": false},
            ]}),
        ),
        (
            vec!["mic", "use", "macbook pro-mikrofon"],
            json!({"running": false, "pinned": BUILT_IN}),
        ),
        (
            vec!["mic", "list"],
            json!({"running": false, "pinned": BUILT_IN, "active": BUILT_IN, "microphones": [
                {"name": USB, "position": 1, "connected": true, "pinned": false, "active": false},
                {"name": "Yeti", "position": 2, "connected": false, "pinned": false,
                 "active": false},
                {"name": BUILT_IN, "position": null, "connected": true, "pinned": true,
                 "active": true},
            ]}),
        ),
        (
            vec!["mic", "use", "--first"],
            json!({"running": false, "pinned": null}),
        ),
        (
            vec!["mic", "order", BUILT_IN, "yeti", USB],
            json!({"running": false, "order": [BUILT_IN, "Yeti", USB]}),
        ),
        (
            vec!["model", "list"],
            json!({"running": false, "models": [
                {"id": "nb-whisper-large", "name": "NB-Whisper Large", "downloaded": true,
                 "active": true},
                {"id": "nb-asr-bokfink", "name": "NB-ASR Bokfink", "downloaded": false,
                 "active": false},
            ]}),
        ),
        (
            vec!["model", "use", "NB-Whisper Large"],
            json!({"running": false, "id": "nb-whisper-large", "name": "NB-Whisper Large"}),
        ),
        (
            vec!["transcribe", &opptak],
            json!({"running": false, "text": "Vi merge pull-requesten.",
                   "model_text": "Vi mercha pull-requesten.",
                   "corrections": [{"from": "Mercha", "to": "merge"}], "heard_nothing": null}),
        ),
        (
            vec!["transcribe", &stille],
            json!({"running": false, "text": null, "model_text": null, "corrections": [],
                   "heard_nothing": "too_little_speech"}),
        ),
    ];
    cases
        .into_iter()
        .map(|(words, expected)| {
            let mut args: Vec<String> = words.into_iter().map(str::to_owned).collect();
            args.push("--json".to_owned());
            (args, expected)
        })
        .collect()
}

fn args(words: &[String]) -> Vec<&str> {
    words.iter().map(String::as_str).collect()
}

#[test]
fn every_command_prints_its_json_while_skald_is_not_running() {
    let (folder, paths) = folder();
    for (words, expected) in commands_and_json(&folder) {
        let ran = skald(&paths, &args(&words));
        assert_eq!(ran.code, 0, "{words:?}: {ran:?}");
        assert_eq!(ran.json(), expected, "{words:?}");
        assert_eq!(ran.err, "", "{words:?}");
    }
}

#[cfg(unix)]
#[test]
fn every_command_prints_the_same_json_through_the_running_skald() {
    let (folder, paths) = folder();
    let file_before = fs::read_to_string(paths.config_file()).unwrap();
    let (app, serving) = running(&paths);
    for (words, mut expected) in commands_and_json(&folder) {
        let ran = skald(&paths, &args(&words));
        assert_eq!(ran.code, 0, "{words:?}: {ran:?}");
        let mut printed = ran.json();
        assert_eq!(printed["running"], true, "{words:?}");
        printed["running"] = json!(false);
        // Only a running Skald has the model to count tokens with.
        if let Some(tokens) = printed.get_mut("tokens") {
            assert_eq!(tokens["limit"], 223, "{words:?}");
            *tokens = serde_json::Value::Null;
        }
        expected["running"] = json!(false);
        assert_eq!(printed, expected, "{words:?}");
    }
    assert_eq!(
        fs::read_to_string(paths.config_file()).unwrap(),
        file_before,
        "while Skald runs, the command line leaves the file to it"
    );
    assert_eq!(
        app.config.lock().unwrap().language,
        skald_core::config::Language::Nynorsk
    );
    serving.stop();
}

#[cfg(unix)]
#[test]
fn the_running_skald_counts_the_tokens() {
    let (_folder, paths) = folder();
    let (_app, serving) = running(&paths);
    let printed = skald(&paths, &["vocab", "list", "--json"]).json();
    let prompt = "Vi merget pull-requesten i går. Skald, PostgreSQL.";
    assert_eq!(
        printed["tokens"],
        json!({"used": prompt.chars().count(), "limit": 223})
    );
    let ran = skald(&paths, &["vocab", "list"]);
    assert!(ran.out.ends_with("Tokens: 50 av 223\n"), "{}", ran.out);
    serving.stop();
}

#[test]
fn direct_writes_keep_the_keys_this_version_does_not_know() {
    let (folder, paths) = folder();
    for (words, _) in commands_and_json(&folder) {
        assert_eq!(skald(&paths, &args(&words)).code, 0, "{words:?}");
    }
    let written = fs::read_to_string(paths.config_file()).unwrap();
    let table: toml::Table = toml::from_str(&written).unwrap();
    assert_eq!(table["future_flag"].as_bool(), Some(true), "{written}");
    assert_eq!(
        table["vocabulary"]["weights"].as_array().unwrap().len(),
        2,
        "{written}"
    );
    assert_eq!(
        table["corrections"][0]["enabled"].as_bool(),
        Some(true),
        "{written}"
    );
    assert_eq!(
        table["overlay"]["position"].as_str(),
        Some("top"),
        "{written}"
    );
    // And what the commands changed is there.
    let config = Config::from_toml(&written).unwrap();
    assert_eq!(config.language, skald_core::config::Language::Nynorsk);
    assert!(!config.sounds);
    assert_eq!(config.microphones, [BUILT_IN, "Yeti", USB]);
    assert_eq!(config.pinned_microphone, None);
    assert_eq!(config.vocabulary.words, ["PostgreSQL", "merget"]);
    assert_eq!(config.corrections.rules[0].to, "merge");
}

#[test]
fn a_change_without_a_file_writes_one() {
    let folder = Folder::new();
    let paths = Paths::in_one_folder(&folder.path.join("Skald"));
    let ran = skald(&paths, &["vocab", "add", "PostgreSQL"]);
    assert_eq!(ran.code, 0, "{ran:?}");
    assert_eq!(
        ran.out,
        "La til «PostgreSQL» sist i ordskatten.\nSkald kjører ikke. Endringen gjelder når Skald \
         starter.\n"
    );
    let config = Config::from_toml(&fs::read_to_string(paths.config_file()).unwrap()).unwrap();
    assert_eq!(config.vocabulary.words, ["PostgreSQL"]);
    assert_eq!(config.model, skald_core::config::DEFAULT_MODEL);
}

#[test]
fn reading_without_files_shows_the_defaults() {
    let folder = Folder::new();
    let paths = folder.paths();
    assert_eq!(skald(&paths, &["last"]).out, "Ingen tekster ennå.\n");
    assert_eq!(
        skald(&paths, &["fix", "list"]).out,
        "Ingen rettinger ennå.\n"
    );
    assert_eq!(
        skald(&paths, &["config", "get", "language"]).out,
        "bokmal\n"
    );
    assert!(!paths.config_file().exists(), "reading writes nothing");
}

#[test]
fn a_change_that_changes_nothing_leaves_the_file_as_it_is() {
    let (_folder, paths) = folder();
    let with_comment = format!("# Mine ord\n{CONFIG}");
    fs::write(paths.config_file(), &with_comment).unwrap();
    for words in [
        &["vocab", "add", "Skald"][..],
        &["fix", "add", "mercha", "merget"],
        &["config", "set", "language", "bokmål"],
        &["mic", "use", "--first"],
    ] {
        let ran = skald(&paths, words);
        assert_eq!(ran.code, 0, "{words:?}: {ran:?}");
    }
    assert_eq!(
        skald(&paths, &["vocab", "add", "Skald"]).out,
        "«Skald» står allerede i ordskatten.\n"
    );
    assert_eq!(
        skald(&paths, &["fix", "add", "mercha", "merget"]).out,
        "Rettingen «mercha» → «merget» finnes allerede.\n"
    );
    assert_eq!(
        fs::read_to_string(paths.config_file()).unwrap(),
        with_comment
    );
}

#[test]
fn a_broken_file_is_reported_and_left_alone() {
    let (_folder, paths) = folder();
    let broken = "language = \"bokmal\"\nsounds = \"ja\"\n";
    fs::write(paths.config_file(), broken).unwrap();
    let ran = skald(&paths, &["vocab", "add", "merget", "--json"]);
    assert_eq!(ran.code, 1);
    let printed = ran.json();
    assert_eq!(printed["error"]["code"], "unreadable");
    let message = printed["error"]["message"].as_str().unwrap();
    assert!(message.starts_with("Kan ikke lese "), "{message}");
    assert!(message.contains("config.toml"), "{message}");
    assert!(message.contains("sounds"), "{message}");
    assert_eq!(fs::read_to_string(paths.config_file()).unwrap(), broken);
}

#[test]
fn commands_run_at_once_lose_no_change() {
    let (_folder, paths) = folder();
    let words: Vec<String> = (0..8).map(|n| format!("ord{n}")).collect();
    std::thread::scope(|scope| {
        for word in &words {
            let paths = &paths;
            scope.spawn(move || {
                let ran = skald(paths, &["vocab", "add", word]);
                assert_eq!(ran.code, 0, "{ran:?}");
            });
        }
    });
    let config = Config::from_toml(&fs::read_to_string(paths.config_file()).unwrap()).unwrap();
    for word in &words {
        assert!(config.vocabulary.words.contains(word), "{word}");
    }
}

#[test]
fn errors_say_what_is_wrong_in_bokmål() {
    let (_folder, paths) = folder();
    let cases: [(&[&str], u8, &str, &str); 9] = [
        (
            &["fix", "remove", "finnes ikke"],
            1,
            "not_found",
            "Fant ingen retting for «finnes ikke».",
        ),
        (
            &["vocab", "remove", "finnes"],
            1,
            "not_found",
            "«finnes» står ikke i ordskatten.",
        ),
        (
            &["config", "set", "language", "svensk"],
            1,
            "invalid",
            "«svensk» er ikke en verdi for language. Velg bokmal eller nynorsk.",
        ),
        (
            &["config", "set", "model", "x"],
            1,
            "not_found",
            "«model» endres ikke med «skald config». Talemodellen byttes med «skald model use».",
        ),
        (
            &["config", "get", "farge"],
            1,
            "not_found",
            "Kjenner ingen innstilling som heter «farge». Innstillingene er: hold_key, language, \
             overlay_theme, copy_to_clipboard, start_at_login, sounds.",
        ),
        (
            &["mic", "use", "AirPods"],
            1,
            "not_found",
            "Kjenner ingen mikrofon som heter «AirPods». Koblet til nå: MacBook Pro-mikrofon, \
             Shure MV7.",
        ),
        (
            &["model", "use", "nb-asr-bokfink"],
            1,
            "invalid",
            "NB-ASR Bokfink er ikke lastet ned ennå.",
        ),
        (
            &["model", "use", "whisper-tiny"],
            1,
            "not_found",
            "Kjenner ingen talemodell som heter «whisper-tiny». Velg en av: nb-whisper-large, \
             nb-asr-bokfink.",
        ),
        (
            &["fix", "add", "mercha", " "],
            1,
            "invalid",
            "En retting trenger teksten ordene skal byttes med.",
        ),
    ];
    for (words, code, kind, message) in cases {
        let ran = skald(&paths, words);
        assert_eq!(ran.code, code, "{words:?}: {ran:?}");
        assert_eq!(ran.out, "", "{words:?}");
        assert_eq!(ran.err, format!("{message}\n"), "{words:?}");

        let mut with_json = words.to_vec();
        with_json.push("--json");
        let ran = skald(&paths, &with_json);
        assert_eq!(ran.code, code, "{words:?}");
        assert_eq!(
            ran.json(),
            json!({"error": {"code": kind, "message": message}}),
            "{words:?}"
        );
    }
}

#[test]
fn wrong_arguments_exit_with_two() {
    let (_folder, paths) = folder();
    let ran = skald(&paths, &["fix", "add", "mercha"]);
    assert_eq!(ran.code, 2);
    assert_eq!(
        ran.err,
        "Bruk: skald fix add <fra> <til>\nSe «skald help».\n"
    );
    let ran = skald(&paths, &["fix", "add", "mercha", "--json"]);
    assert_eq!(ran.code, 2);
    assert_eq!(ran.json()["error"]["code"], "usage");
}

#[test]
fn help_lists_every_command_and_setting() {
    let (_folder, paths) = folder();
    let ran = skald(&paths, &["help"]);
    assert_eq!(ran.code, 0);
    for line in [
        "Bruk: skald <kommando> [--json]",
        "  fix add <fra> <til>          Skriv <til> der modellen skriver <fra>",
        "  transcribe <fil.wav>         Skriv ut teksten i en lydfil, med rettingene brukt",
        "  overlay_theme      runepinne, smidd-jern eller kjedelig",
        "  sounds             true eller false",
    ] {
        assert!(
            ran.out.lines().any(|l| l == line),
            "{line:?} in:\n{}",
            ran.out
        );
    }
}

#[test]
fn a_file_to_transcribe_is_found_from_here() {
    let (folder, paths) = folder();
    let ran = skald(&paths, &["transcribe", "finnes-ikke.wav"]);
    assert_eq!(ran.code, 1);
    assert_eq!(ran.err, "Finner ikke lydfilen finnes-ikke.wav.\n");

    let ran = skald(&paths, &["transcribe", &wav(&folder, "opptak.wav")]);
    assert_eq!(ran.code, 0, "{ran:?}");
    assert_eq!(
        ran.out,
        "Vi merget pull-requesten.\nFør rettinger: Vi mercha pull-requesten.\n"
    );
    assert_eq!(
        ran.err,
        "Skald kjører ikke, så talemodellen lastes her. Det tar noen sekunder.\n"
    );
    let ran = skald(&paths, &["transcribe", &wav(&folder, "stille.wav")]);
    assert_eq!(ran.out, "Hørte ingenting: fant for lite tale.\n");
}

#[test]
fn a_person_reads_plain_bokmål() {
    let (_folder, paths) = folder();
    let ran = skald(&paths, &["fix", "list"]);
    assert_eq!(
        ran.out,
        "mercha → merget · brukt 2 ganger\nPost gres SQL → PostgreSQL · ikke brukt ennå\n"
    );
    let ran = skald(&paths, &["vocab"]);
    assert_eq!(
        ran.out,
        "Setning: Vi merget pull-requesten i går.\nOrd, det viktigste sist:\n  Skald\n  \
         PostgreSQL\nTokens: telles når Skald kjører, med talemodellen.\n"
    );
    let ran = skald(&paths, &["mic"]);
    assert_eq!(
        ran.out,
        "1. Shure MV7 · koblet til · brukes nå\n2. Yeti · ikke koblet til\n   MacBook \
         Pro-mikrofon · koblet til · ikke i rekkefølgen\n"
    );
    let ran = skald(&paths, &["model"]);
    assert_eq!(
        ran.out,
        "NB-Whisper Large (nb-whisper-large) · lastet ned · i bruk\nNB-ASR Bokfink \
         (nb-asr-bokfink) · ikke lastet ned\n"
    );
    let ran = skald(&paths, &["config"]);
    assert_eq!(
        ran.out,
        "hold_key = right-command\nlanguage = bokmal\noverlay_theme = runepinne\n\
         copy_to_clipboard = false\nstart_at_login = false\nsounds = true\n"
    );
    let ran = skald(&paths, &["last"]);
    assert_eq!(ran.out, "Vi merget pull-requesten.\n");
    let ran = skald(&paths, &["fix", "add", "mercha", "merge"]);
    assert_eq!(
        ran.out,
        "Skriver nå «merge» der modellen skriver «mercha» (før: «merget»).\nSkald kjører ikke. \
         Endringen gjelder når Skald starter.\n"
    );
    let ran = skald(&paths, &["mic", "use", USB]);
    assert_eq!(
        ran.out,
        "Bruker «Shure MV7» foran rekkefølgen så lenge den er koblet til.\nSkald kjører ikke. \
         Endringen gjelder når Skald starter.\n"
    );
}
