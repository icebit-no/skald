//! `skald mcp` as an agent's client speaks to it: the handshake, the tools it lists, and a call to every
//! tool, answered with the JSON the command line prints for the same command.

mod common;

use common::{FakeMachine, Folder, commands_and_json, folder, skald};
use serde_json::{Value, json};
use skald_control::mcp::PROTOCOL_VERSION;
use skald_core::paths::Paths;

/// Every message `lines` holds sent in one session, and every line answered, in order.
fn session(paths: &Paths, lines: &[Value]) -> Vec<Value> {
    let mut input = String::new();
    for line in lines {
        input.push_str(&line.to_string());
        input.push('\n');
    }
    let mut output = Vec::new();
    skald_control::mcp::serve(input.as_bytes(), &mut output, paths, &FakeMachine).unwrap();
    String::from_utf8(output)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap_or_else(|error| panic!("{error}: {line}")))
        .collect()
}

fn initialize() -> Vec<Value> {
    vec![
        json!({"jsonrpc": "2.0", "id": 0, "method": "initialize", "params": {
            "protocolVersion": PROTOCOL_VERSION, "capabilities": {},
            "clientInfo": {"name": "test", "version": "0"}}}),
        json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
    ]
}

fn call(id: usize, tool: &str, arguments: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "method": "tools/call",
           "params": {"name": tool, "arguments": arguments}})
}

/// The answer to one tool call after the handshake.
fn call_once(paths: &Paths, tool: &str, arguments: Value) -> Value {
    let mut lines = initialize();
    lines.push(call(1, tool, arguments));
    let answers = session(paths, &lines);
    assert_eq!(answers.len(), 2, "{answers:?}");
    answers[1].clone()
}

/// The object a tool answered with, checked to be the same as its text.
fn structured(answer: &Value) -> &Value {
    let result = &answer["result"];
    let text: Value = serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(result["content"][0]["type"], "text");
    assert_eq!(text, result["structuredContent"], "{answer}");
    &result["structuredContent"]
}

/// The tool call for each of the command line's commands in `commands_and_json`, in the same order.
fn tool_calls(folder: &Folder) -> Vec<(&'static str, Value)> {
    let opptak = common::wav(folder, "opptak.wav");
    let stille = common::wav(folder, "stille.wav");
    vec![
        ("last", json!({})),
        ("history", json!({})),
        ("fix_list", json!({})),
        ("fix_add", json!({"from": "Mercha", "to": "merge"})),
        ("fix_add", json!({"from": "Hei", "to": "Hallo"})),
        ("fix_remove", json!({"from": "hei"})),
        ("vocab_list", json!({})),
        ("vocab_add", json!({"word": "merget"})),
        ("vocab_add", json!({"word": " merget "})),
        ("vocab_remove", json!({"word": "Skald"})),
        ("vocab_sentence", json!({"sentence": "Vi merget den."})),
        ("config_get", json!({})),
        ("config_set", json!({"key": "language", "value": "nynorsk"})),
        ("config_set", json!({"key": "sounds", "value": "av"})),
        (
            "config_set",
            json!({"key": "overlay-theme", "value": "smidd-jern"}),
        ),
        ("config_get", json!({"key": "language"})),
        ("mic_list", json!({})),
        ("mic_use", json!({"name": "macbook pro-mikrofon"})),
        ("mic_list", json!({})),
        ("mic_use", json!({})),
        (
            "mic_order",
            json!({"names": [common::BUILT_IN, "yeti", common::USB]}),
        ),
        ("model_list", json!({})),
        ("model_use", json!({"id": "NB-Whisper Large"})),
        ("transcribe", json!({"path": opptak})),
        ("transcribe", json!({"path": stille})),
    ]
}

/// Each tool call beside the command and the JSON the command line prints for it.
fn calls_and_json(folder: &Folder) -> Vec<(&'static str, Value, Value)> {
    let commands = commands_and_json(folder);
    let calls = tool_calls(folder);
    assert_eq!(calls.len(), commands.len());
    calls
        .into_iter()
        .zip(commands)
        .map(|((tool, arguments), (words, expected))| {
            let command = words[..tool.split('_').count()].join("_");
            assert_eq!(tool, command, "the calls follow the commands: {words:?}");
            (tool, arguments, expected)
        })
        .collect()
}

#[test]
fn the_handshake_names_skald_and_what_it_offers() {
    let (_folder, paths) = folder();
    let mut lines = initialize();
    lines.push(json!({"jsonrpc": "2.0", "id": "ping-1", "method": "ping"}));
    let answers = session(&paths, &lines);
    assert_eq!(
        answers.len(),
        2,
        "a notification is not answered: {answers:?}"
    );
    let result = &answers[0]["result"];
    assert_eq!(answers[0]["id"], 0);
    assert_eq!(answers[0]["jsonrpc"], "2.0");
    assert_eq!(result["protocolVersion"], PROTOCOL_VERSION);
    assert_eq!(
        result["capabilities"],
        json!({"tools": {"listChanged": false}})
    );
    assert_eq!(result["serverInfo"]["name"], "skald");
    assert_eq!(result["serverInfo"]["version"], env!("CARGO_PKG_VERSION"));
    assert!(result["instructions"].as_str().unwrap().contains("fix_add"));
    assert_eq!(
        answers[1],
        json!({"jsonrpc": "2.0", "id": "ping-1", "result": {}})
    );
}

#[test]
fn an_older_client_is_answered_in_its_own_revision() {
    let (_folder, paths) = folder();
    let answers = session(
        &paths,
        &[json!({"jsonrpc": "2.0", "id": 1, "method": "initialize",
                 "params": {"protocolVersion": "2025-06-18", "capabilities": {},
                            "clientInfo": {"name": "test", "version": "0"}}})],
    );
    assert_eq!(answers[0]["result"]["protocolVersion"], "2025-06-18");
}

#[test]
fn every_command_is_a_tool_and_install_and_mcp_are_not() {
    let (_folder, paths) = folder();
    let mut lines = initialize();
    lines.push(json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"}));
    let answers = session(&paths, &lines);
    let tools = answers[1]["result"]["tools"].as_array().unwrap();
    let names: Vec<&str> = tools
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        [
            "last",
            "history",
            "fix_list",
            "fix_add",
            "fix_remove",
            "vocab_list",
            "vocab_add",
            "vocab_remove",
            "vocab_sentence",
            "config_get",
            "config_set",
            "mic_list",
            "mic_use",
            "mic_order",
            "model_list",
            "model_use",
            "transcribe",
        ]
    );
    for tool in tools {
        let schema = &tool["inputSchema"];
        assert_eq!(schema["type"], "object", "{tool}");
        assert_eq!(schema["additionalProperties"], false, "{tool}");
        for required in schema["required"].as_array().unwrap() {
            assert!(
                schema["properties"]
                    .get(required.as_str().unwrap())
                    .is_some(),
                "{tool}"
            );
        }
        assert!(!tool["description"].as_str().unwrap().is_empty(), "{tool}");
        assert!(!tool["title"].as_str().unwrap().is_empty(), "{tool}");
        assert_eq!(tool["annotations"]["openWorldHint"], false, "{tool}");
    }
    let read_only = |name: &str| {
        tools.iter().find(|tool| tool["name"] == name).unwrap()["annotations"]["readOnlyHint"]
            .clone()
    };
    assert_eq!(read_only("history"), true);
    assert_eq!(read_only("fix_add"), false);
}

#[test]
fn every_tool_answers_as_the_command_line_does_while_skald_is_not_running() {
    let (folder, paths) = folder();
    let cases = calls_and_json(&folder);
    let mut lines = initialize();
    for (index, (tool, arguments, _)) in cases.iter().enumerate() {
        lines.push(call(index + 1, tool, arguments.clone()));
    }
    let answers = session(&paths, &lines);
    assert_eq!(answers.len(), cases.len() + 1);
    for (index, (tool, _, expected)) in cases.iter().enumerate() {
        let answer = &answers[index + 1];
        assert_eq!(answer["id"], index + 1, "{tool}");
        assert_eq!(answer["result"]["isError"], false, "{tool}: {answer}");
        assert_eq!(structured(answer), expected, "{tool}");
    }
}

#[cfg(any(unix, windows))]
#[test]
fn every_tool_answers_through_the_running_skald() {
    let (folder, paths) = folder();
    let file_before = std::fs::read_to_string(paths.config_file()).unwrap();
    let (app, serving) = common::running(&paths);
    let cases = calls_and_json(&folder);
    let mut lines = initialize();
    for (index, (tool, arguments, _)) in cases.iter().enumerate() {
        lines.push(call(index + 1, tool, arguments.clone()));
    }
    let answers = session(&paths, &lines);
    for (index, (tool, _, expected)) in cases.into_iter().enumerate() {
        let mut answered = structured(&answers[index + 1]).clone();
        assert_eq!(answered["running"], true, "{tool}");
        let mut expected = expected;
        answered["running"] = json!(false);
        expected["running"] = json!(false);
        // Only a running Skald has the model to count tokens with.
        if let Some(tokens) = answered.get_mut("tokens") {
            assert_eq!(tokens["limit"], 223, "{tool}");
            *tokens = Value::Null;
        }
        assert_eq!(answered, expected, "{tool}");
    }
    assert_eq!(
        std::fs::read_to_string(paths.config_file()).unwrap(),
        file_before,
        "while Skald runs, the tools leave the file to it"
    );
    assert_eq!(
        app.config.lock().unwrap().language,
        skald_core::config::Language::Nynorsk
    );
    serving.stop();
}

#[test]
fn a_change_made_by_a_tool_is_what_the_command_line_reads() {
    let (_folder, paths) = folder();
    let answer = call_once(
        &paths,
        "fix_add",
        json!({"from": "Post gres", "to": "Postgres"}),
    );
    assert_eq!(answer["result"]["isError"], false, "{answer}");
    let listed = skald(&paths, &["fix", "list", "--json"]).json();
    assert_eq!(
        listed["corrections"][2],
        json!({"from": "Post gres", "to": "Postgres", "applied": 0})
    );
}

#[test]
fn a_tool_that_fails_answers_with_the_commands_error() {
    let (_folder, paths) = folder();
    for (tool, arguments, code, message) in [
        (
            "fix_remove",
            json!({"from": "finnes ikke"}),
            "not_found",
            "Fant ingen retting for «finnes ikke».",
        ),
        (
            "model_use",
            json!({"id": "nb-asr-bokfink"}),
            "invalid",
            "NB-ASR Bokfink er ikke lastet ned ennå.",
        ),
        (
            "transcribe",
            json!({"path": "finnes-ikke.wav"}),
            "not_found",
            "Finner ikke lydfilen finnes-ikke.wav.",
        ),
        (
            "fix_add",
            json!({"from": "mercha"}),
            "usage",
            "fix_add trenger «to».",
        ),
        (
            "mic_use",
            json!({"microphone": "Yeti"}),
            "usage",
            "Ukjent argument «microphone». mic_use tar name.",
        ),
    ] {
        let answer = call_once(&paths, tool, arguments);
        assert_eq!(answer["result"]["isError"], true, "{tool}: {answer}");
        assert_eq!(
            structured(&answer),
            &json!({"error": {"code": code, "message": message}}),
            "{tool}"
        );
    }
    let answer = call_once(&paths, "fix_list", json!(["not", "an", "object"]));
    assert_eq!(answer["result"]["isError"], true, "{answer}");
}

#[test]
fn a_message_that_is_no_request_of_these_is_a_protocol_error() {
    let (_folder, paths) = folder();
    let error = |answer: &Value| answer["error"]["code"].as_i64().unwrap();

    let answer = call_once(&paths, "skill_install", json!({}));
    assert_eq!(error(&answer), -32602, "{answer}");
    assert_eq!(answer["error"]["message"], "Unknown tool: skill_install");

    let mut lines = initialize();
    lines.push(json!({"jsonrpc": "2.0", "id": 1, "method": "resources/list"}));
    lines.push(json!({"jsonrpc": "2.0", "id": 2}));
    lines.push(json!(7));
    lines.push(json!({"jsonrpc": "2.0", "id": 3, "result": {}}));
    lines.push(json!({"jsonrpc": "2.0", "id": null, "method": "ping"}));
    lines.push(json!([]));
    let answers = session(&paths, &lines);
    let codes: Vec<(Value, i64)> = answers[1..]
        .iter()
        .map(|answer| (answer["id"].clone(), error(answer)))
        .collect();
    assert_eq!(
        codes,
        [
            (json!(1), -32601),
            (json!(2), -32600),
            (Value::Null, -32600),
            (Value::Null, -32600),
            (Value::Null, -32600),
        ],
        "a response from the client is not answered: {answers:?}"
    );
}

#[test]
fn a_line_that_is_not_json_is_answered_and_the_session_goes_on() {
    let (_folder, paths) = folder();
    let input = "{not json\n\r\n{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}\r\n";
    let mut output = Vec::new();
    skald_control::mcp::serve(input.as_bytes(), &mut output, &paths, &FakeMachine).unwrap();
    let answers: Vec<Value> = String::from_utf8(output)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(answers.len(), 2, "{answers:?}");
    assert_eq!(answers[0]["id"], Value::Null);
    assert_eq!(answers[0]["error"]["code"], -32700);
    assert_eq!(answers[1], json!({"jsonrpc": "2.0", "id": 1, "result": {}}));
}

#[test]
fn a_batch_is_answered_with_a_batch() {
    let (_folder, paths) = folder();
    let answers = session(
        &paths,
        &[json!([
            {"jsonrpc": "2.0", "id": 1, "method": "ping"},
            {"jsonrpc": "2.0", "method": "notifications/initialized"},
            {"jsonrpc": "2.0", "id": 2, "method": "tools/call",
             "params": {"name": "last", "arguments": {}}},
        ])],
    );
    let batch = answers[0].as_array().unwrap();
    assert_eq!(batch.len(), 2, "{batch:?}");
    assert_eq!(batch[0]["id"], 1);
    assert_eq!(
        structured(&batch[1])["last"]["text"],
        "Vi merget pull-requesten."
    );
}
