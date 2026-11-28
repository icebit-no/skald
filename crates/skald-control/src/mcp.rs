//! `skald mcp`: the commands as tools of the Model Context Protocol, for an agent that speaks it over
//! standard input and output (D-31).
//!
//! Each tool is a command, named as the request it makes and taking the request's fields as its
//! arguments. A call is carried out as the command line carries out the command, through the running
//! Skald or on the files, and answers with the object `skald <command> --json` prints, so the tools and the
//! command line never disagree.
//!
//! ```text
//! → {"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"fix_add","arguments":{"from":"mercha","to":"merget"}}}
//! ← {"jsonrpc":"2.0","id":3,"result":{"content":[{"type":"text","text":"{…}"}],"structuredContent":{"running":false,"from":"mercha","to":"merget","previous":null},"isError":false}}
//! ```

use std::io::{self, BufRead, Write};

use serde::Deserialize;
use serde_json::{Map, Value, json};
use skald_core::paths::Paths;

use crate::cli;
use crate::commands::SettingKey;
use crate::protocol::{Error, ErrorCode, Request, read_line};
use crate::skald::Machine;

/// The revision answered with when the client asks for one this server does not speak: the newest that
/// opens with the `initialize` handshake.
pub const PROTOCOL_VERSION: &str = "2025-11-25";

/// The revisions a client may ask for, newest first. They differ in nothing these tools use, so the
/// server answers each the same way.
const PROTOCOL_VERSIONS: [&str; 4] = [PROTOCOL_VERSION, "2025-06-18", "2025-03-26", "2024-11-05"];

const INSTRUCTIONS: &str = "Skald is the user's Norwegian push-to-talk dictation app. These tools are the \
    commands of its command line and answer with the same JSON as `skald <command> --json`; `running` says \
    whether Skald was running, and a change made while it was not applies when it starts. When the user \
    says Skald wrote a word wrong, read `last` or `history` first, add a correction with `fix_add` for the \
    wrong form as Skald wrote it, and, where the model needs to hear the word, add it with `vocab_add`. \
    Corrections come from evidence: add one only for a word that broke in a real dictation. The ordskatt \
    keeps only its last 223 tokens, so the most important word goes last. An error is \
    {\"error\": {\"code\", \"message\"}}, with the message in Norwegian bokmål for the user.";

const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;

/// Answers each message read until the input ends, one JSON-RPC message per line each way.
pub fn serve(
    mut input: impl BufRead,
    output: &mut dyn Write,
    paths: &Paths,
    machine: &dyn Machine,
) -> io::Result<()> {
    let server = Server { paths, machine };
    while let Some(line) = read_line(&mut input)? {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(answer) = server.handle(line) {
            let mut text = answer.to_string();
            text.push('\n');
            output.write_all(text.as_bytes())?;
            output.flush()?;
        }
    }
    Ok(())
}

struct Server<'a> {
    paths: &'a Paths,
    machine: &'a dyn Machine,
}

impl Server<'_> {
    /// The answer to a line, or `None` for a line that wants none: a notification, or a response to a
    /// request this server never sends.
    fn handle(&self, line: &str) -> Option<Value> {
        match serde_json::from_str::<Value>(line) {
            Ok(Value::Array(batch)) if batch.is_empty() => Some(failure(
                Value::Null,
                INVALID_REQUEST,
                "An empty batch".to_owned(),
            )),
            Ok(Value::Array(batch)) => {
                let answers: Vec<Value> = batch
                    .into_iter()
                    .filter_map(|message| self.message(message))
                    .collect();
                (!answers.is_empty()).then_some(Value::Array(answers))
            }
            Ok(message) => self.message(message),
            Err(error) => Some(failure(
                Value::Null,
                PARSE_ERROR,
                format!("Parse error: {error}"),
            )),
        }
    }

    fn message(&self, message: Value) -> Option<Value> {
        let Value::Object(message) = message else {
            return Some(failure(
                Value::Null,
                INVALID_REQUEST,
                "A message is a JSON object".to_owned(),
            ));
        };
        let method = message.get("method").and_then(Value::as_str);
        let id = message.get("id")?;
        if !(id.is_string() || id.is_number()) {
            return Some(failure(
                Value::Null,
                INVALID_REQUEST,
                "An id is a string or a number".to_owned(),
            ));
        }
        let Some(method) = method else {
            if message.contains_key("result") || message.contains_key("error") {
                return None;
            }
            return Some(failure(
                id.clone(),
                INVALID_REQUEST,
                "A request names its method".to_owned(),
            ));
        };
        let params = message.get("params").unwrap_or(&Value::Null);
        let answered = match method {
            "initialize" => Ok(initialize(params)),
            "ping" => Ok(json!({})),
            "tools/list" => {
                Ok(json!({ "tools": TOOLS.iter().map(Tool::describe).collect::<Vec<_>>() }))
            }
            "tools/call" => self.call(params),
            _ => Err((METHOD_NOT_FOUND, format!("Method not found: {method}"))),
        };
        Some(match answered {
            Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
            Err((code, message)) => failure(id.clone(), code, message),
        })
    }

    /// A tool call. A tool that fails answers with its error as the result, so the agent reads it; only a
    /// call to a tool that does not exist is an error of the protocol.
    fn call(&self, params: &Value) -> Result<Value, (i64, String)> {
        let name = params
            .get("name")
            .and_then(Value::as_str)
            .ok_or((INVALID_PARAMS, "A tool call names its tool".to_owned()))?;
        let tool = TOOLS
            .iter()
            .find(|tool| tool.name == name)
            .ok_or((INVALID_PARAMS, format!("Unknown tool: {name}")))?;
        let answered = match params.get("arguments") {
            None | Some(Value::Null) => self.run(tool, &Map::new()),
            Some(Value::Object(arguments)) => self.run(tool, arguments),
            Some(_) => Err(usage(format!("Argumentene til {name} er et JSON-objekt."))),
        };
        Ok(match answered {
            Ok(value) => tool_result(value, false),
            Err(error) => tool_result(json!({ "error": error }), true),
        })
    }

    fn run(&self, tool: &Tool, arguments: &Map<String, Value>) -> Result<Value, Error> {
        let request = cli::absolute(tool.request(arguments)?)?;
        let (result, running) =
            cli::send(&request, self.paths, self.machine, true, &mut io::sink());
        result.map(|output| cli::to_json(&output, running))
    }
}

fn initialize(params: &Value) -> Value {
    let asked = params.get("protocolVersion").and_then(Value::as_str);
    let version = PROTOCOL_VERSIONS
        .into_iter()
        .find(|version| Some(*version) == asked)
        .unwrap_or(PROTOCOL_VERSION);
    json!({
        "protocolVersion": version,
        "capabilities": { "tools": { "listChanged": false } },
        "serverInfo": { "name": "skald", "title": "Skald", "version": env!("CARGO_PKG_VERSION") },
        "instructions": INSTRUCTIONS,
    })
}

/// The answer as JSON text, for clients that read only text, and as the object itself.
fn tool_result(value: Value, is_error: bool) -> Value {
    json!({
        "content": [{ "type": "text", "text": value.to_string() }],
        "structuredContent": value,
        "isError": is_error,
    })
}

fn failure(id: Value, code: i64, message: String) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

fn usage(message: impl Into<String>) -> Error {
    Error::new(ErrorCode::Usage, message)
}

pub(crate) struct Tool {
    pub(crate) name: &'static str,
    /// What a client shows the user, in bokmål.
    title: &'static str,
    about: &'static str,
    pub(crate) params: &'static [Param],
    read_only: bool,
    /// Whether a call may overwrite or remove what is there, rather than only add to it.
    destructive: bool,
}

pub(crate) struct Param {
    pub(crate) name: &'static str,
    kind: Kind,
    required: bool,
    about: &'static str,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Text,
    Texts,
    /// A key of `skald config`.
    SettingKey,
}

const fn text(name: &'static str, about: &'static str) -> Param {
    Param {
        name,
        kind: Kind::Text,
        required: true,
        about,
    }
}

pub(crate) const TOOLS: [Tool; 17] = [
    Tool {
        name: "last",
        title: "Den siste teksten",
        about: "The latest text Skald inserted, after corrections, with when the take ended and the \
                microphone; null before the first take.",
        params: &[],
        read_only: true,
        destructive: false,
    },
    Tool {
        name: "history",
        title: "De fem siste tekstene",
        about: "The five latest texts Skald inserted, newest first.",
        params: &[],
        read_only: true,
        destructive: false,
    },
    Tool {
        name: "fix_list",
        title: "Rettingene",
        about: "The corrections in the order the user keeps them, each with how often it has applied.",
        params: &[],
        read_only: true,
        destructive: false,
    },
    Tool {
        name: "fix_add",
        title: "Ny retting",
        about: "Adds a correction: Skald writes `to` wherever the model writes `from`. It matches whole \
                words and ignores case; a lower-case replacement takes a capital where the text had one. \
                A rule for the same words takes the new replacement, and `previous` is the one it had. \
                Add one only for a word that broke in a real dictation, as Skald wrote it.",
        params: &[
            text(
                "from",
                "The words as the model writes them, as in `last` or `history`.",
            ),
            text("to", "The text to write instead."),
        ],
        read_only: false,
        destructive: true,
    },
    Tool {
        name: "fix_remove",
        title: "Fjern en retting",
        about: "Removes the correction for `from`.",
        params: &[text("from", "The words the correction replaces.")],
        read_only: false,
        destructive: true,
    },
    Tool {
        name: "vocab_list",
        title: "Ordskatten",
        about: "The ordskatt, the context sent to the model before each take: the sentence it opens with, \
                the words with the most important last, and while Skald runs the tokens used of the 223 \
                the model keeps.",
        params: &[],
        read_only: true,
        destructive: false,
    },
    Tool {
        name: "vocab_add",
        title: "Legg et ord i ordskatten",
        about: "Adds a word or name last in the ordskatt, where it weighs most, so the model hears it. A \
                word already there keeps its place and `changed` is false.",
        params: &[text("word", "One word, or a name with spaces.")],
        read_only: false,
        destructive: false,
    },
    Tool {
        name: "vocab_remove",
        title: "Ta et ord ut av ordskatten",
        about: "Takes a word out of the ordskatt.",
        params: &[text("word", "The word exactly as `vocab_list` shows it.")],
        read_only: false,
        destructive: true,
    },
    Tool {
        name: "vocab_sentence",
        title: "Setningen ordskatten begynner med",
        about: "Replaces the sentence the ordskatt opens with, which shows words in their inflected forms; \
                an empty sentence removes it. Answers like `vocab_list`.",
        params: &[text("sentence", "The whole sentence.")],
        read_only: false,
        destructive: true,
    },
    Tool {
        name: "config_get",
        title: "Innstillingene",
        about: "All of Skald's settings that `config_set` changes, or the one named.",
        params: &[Param {
            name: "key",
            kind: Kind::SettingKey,
            required: false,
            about: "The setting; without it, all of them.",
        }],
        read_only: true,
        destructive: false,
    },
    Tool {
        name: "config_set",
        title: "Endre en innstilling",
        about: "Changes one of Skald's settings. The model, the microphones, the ordskatt and the \
                corrections have tools of their own.",
        params: &[
            Param {
                name: "key",
                kind: Kind::SettingKey,
                required: true,
                about: "The setting.",
            },
            text(
                "value",
                "One of the values the setting takes; true and false are also taken as ja and nei, or på \
                 and av.",
            ),
        ],
        read_only: false,
        destructive: true,
    },
    Tool {
        name: "mic_list",
        title: "Mikrofonene",
        about: "The microphones: those in the user's order, with their place in it, then the pinned one \
                and the connected ones the order leaves out; which are connected, pinned, and the one the \
                next take uses.",
        params: &[],
        read_only: true,
        destructive: false,
    },
    Tool {
        name: "mic_use",
        title: "Bruk en mikrofon",
        about: "Uses this microphone ahead of the order while it is connected. Without `name`, removes that \
                choice, so the first connected microphone in the order is used.",
        params: &[Param {
            name: "name",
            kind: Kind::Text,
            required: false,
            about: "The microphone's name as `mic_list` gives it; case is ignored.",
        }],
        read_only: false,
        destructive: true,
    },
    Tool {
        name: "mic_order",
        title: "Rekkefølgen på mikrofonene",
        about: "Sets the order microphones are used in: the first connected one is used.",
        params: &[Param {
            name: "names",
            kind: Kind::Texts,
            required: true,
            about: "Every microphone in the order, first the one to use first, by the names `mic_list` \
                    gives.",
        }],
        read_only: false,
        destructive: true,
    },
    Tool {
        name: "model_list",
        title: "Talemodellene",
        about: "The speech models, made by the National Library of Norway, which are downloaded, and the \
                one in use.",
        params: &[],
        read_only: true,
        destructive: false,
    },
    Tool {
        name: "model_use",
        title: "Bytt talemodell",
        about: "Switches to a downloaded speech model.",
        params: &[text(
            "id",
            "The model's id or name, as `model_list` gives it.",
        )],
        read_only: false,
        destructive: true,
    },
    Tool {
        name: "transcribe",
        title: "Skriv ut en lydfil",
        about: "Transcribes a WAV file as a take and applies the corrections: `text` is what Skald would \
                insert, `model_text` the model's text before corrections, `corrections` the rules that \
                applied, and `heard_nothing` why it heard no speech, if it did not. Without a running \
                Skald it loads the model first, which takes several seconds.",
        params: &[text("path", "The WAV file's full path.")],
        read_only: true,
        destructive: false,
    },
];

impl Tool {
    /// The tool as `tools/list` gives it.
    fn describe(&self) -> Value {
        let properties: Map<String, Value> = self
            .params
            .iter()
            .map(|param| (param.name.to_owned(), param.schema()))
            .collect();
        let required: Vec<&str> = self
            .params
            .iter()
            .filter(|param| param.required)
            .map(|param| param.name)
            .collect();
        let mut annotations = json!({
            "title": self.title,
            "readOnlyHint": self.read_only,
            "openWorldHint": false,
        });
        if !self.read_only {
            annotations["destructiveHint"] = json!(self.destructive);
            annotations["idempotentHint"] = json!(true);
        }
        json!({
            "name": self.name,
            "title": self.title,
            "description": self.about,
            "inputSchema": {
                "type": "object",
                "properties": properties,
                "required": required,
                "additionalProperties": false,
            },
            "annotations": annotations,
        })
    }

    /// The request a call makes. The arguments are checked against the tool's own parameters first, so a
    /// misspelt one is refused rather than read as left out.
    fn request(&self, arguments: &Map<String, Value>) -> Result<Request, Error> {
        let names = || {
            let names: Vec<&str> = self.params.iter().map(|param| param.name).collect();
            if names.is_empty() {
                format!("{} tar ingen argumenter.", self.name)
            } else {
                format!("{} tar {}.", self.name, names.join(", "))
            }
        };
        if let Some(unknown) = arguments
            .keys()
            .find(|key| !self.params.iter().any(|param| param.name == *key))
        {
            return Err(usage(format!("Ukjent argument «{unknown}». {}", names())));
        }
        for param in self.params {
            match arguments.get(param.name) {
                None | Some(Value::Null) if param.required => {
                    return Err(usage(format!("{} trenger «{}».", self.name, param.name)));
                }
                None | Some(Value::Null) => {}
                Some(value) if param.takes(value) => {}
                Some(_) => {
                    return Err(usage(format!(
                        "«{}» til {} er {}.",
                        param.name,
                        self.name,
                        match param.kind {
                            Kind::Texts => "en liste med tekster",
                            Kind::Text | Kind::SettingKey => "en tekst",
                        }
                    )));
                }
            }
        }
        let mut fields = arguments.clone();
        fields.insert("command".to_owned(), Value::String(self.name.to_owned()));
        Request::deserialize(Value::Object(fields))
            .map_err(|error| usage(format!("Ugyldige argumenter til {}: {error}", self.name)))
    }
}

impl Param {
    fn takes(&self, value: &Value) -> bool {
        match self.kind {
            Kind::Text | Kind::SettingKey => value.is_string(),
            Kind::Texts => value
                .as_array()
                .is_some_and(|items| items.iter().all(Value::is_string)),
        }
    }

    fn schema(&self) -> Value {
        match self.kind {
            Kind::Text => json!({ "type": "string", "description": self.about }),
            Kind::Texts => json!({
                "type": "array",
                "items": { "type": "string" },
                "description": self.about,
            }),
            Kind::SettingKey => {
                let values: Vec<String> = SettingKey::ALL
                    .iter()
                    .map(|key| format!("{} ({})", key.name(), key.shown().join(", ")))
                    .collect();
                json!({
                    "type": "string",
                    "enum": SettingKey::ALL.iter().map(|key| key.name()).collect::<Vec<_>>(),
                    "description": format!("{} The settings and their values: {}.", self.about, values.join("; ")),
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A call for each tool with every argument it takes, and the request it must make.
    fn every_call() -> Vec<(&'static str, Value, Request)> {
        vec![
            ("last", json!({}), Request::Last),
            ("history", json!({}), Request::History),
            ("fix_list", json!({}), Request::FixList),
            (
                "fix_add",
                json!({"from": "mercha", "to": "merget"}),
                Request::FixAdd {
                    from: "mercha".into(),
                    to: "merget".into(),
                },
            ),
            (
                "fix_remove",
                json!({"from": "mercha"}),
                Request::FixRemove {
                    from: "mercha".into(),
                },
            ),
            ("vocab_list", json!({}), Request::VocabList),
            (
                "vocab_add",
                json!({"word": "merget"}),
                Request::VocabAdd {
                    word: "merget".into(),
                },
            ),
            (
                "vocab_remove",
                json!({"word": "merget"}),
                Request::VocabRemove {
                    word: "merget".into(),
                },
            ),
            (
                "vocab_sentence",
                json!({"sentence": "Vi merget den."}),
                Request::VocabSentence {
                    sentence: "Vi merget den.".into(),
                },
            ),
            (
                "config_get",
                json!({"key": "language"}),
                Request::ConfigGet {
                    key: Some("language".into()),
                },
            ),
            (
                "config_set",
                json!({"key": "sounds", "value": "av"}),
                Request::ConfigSet {
                    key: "sounds".into(),
                    value: "av".into(),
                },
            ),
            ("mic_list", json!({}), Request::MicList),
            (
                "mic_use",
                json!({"name": "Shure MV7"}),
                Request::MicUse {
                    name: Some("Shure MV7".into()),
                },
            ),
            (
                "mic_order",
                json!({"names": ["Shure MV7", "Yeti"]}),
                Request::MicOrder {
                    names: vec!["Shure MV7".into(), "Yeti".into()],
                },
            ),
            ("model_list", json!({}), Request::ModelList),
            (
                "model_use",
                json!({"id": "nb-whisper-large"}),
                Request::ModelUse {
                    id: "nb-whisper-large".into(),
                },
            ),
            (
                "transcribe",
                json!({"path": "/tmp/opptak.wav"}),
                Request::Transcribe {
                    path: "/tmp/opptak.wav".into(),
                },
            ),
        ]
    }

    fn tool(name: &str) -> &'static Tool {
        TOOLS.iter().find(|tool| tool.name == name).unwrap()
    }

    #[test]
    fn every_tool_makes_the_request_of_the_command_it_is_named_after() {
        let calls = every_call();
        assert_eq!(calls.len(), TOOLS.len());
        for (name, arguments, expected) in calls {
            let request = tool(name).request(arguments.as_object().unwrap()).unwrap();
            assert_eq!(request, expected, "{name}");
            let tagged = serde_json::to_value(&request).unwrap();
            assert_eq!(tagged["command"], name, "{name}");
        }
    }

    #[test]
    fn an_optional_argument_left_out_or_null_is_none() {
        let none = Map::new();
        assert_eq!(
            tool("mic_use").request(&none).unwrap(),
            Request::MicUse { name: None }
        );
        let null = json!({"name": null});
        assert_eq!(
            tool("mic_use").request(null.as_object().unwrap()).unwrap(),
            Request::MicUse { name: None }
        );
        assert_eq!(
            tool("config_get").request(&none).unwrap(),
            Request::ConfigGet { key: None }
        );
    }

    #[test]
    fn wrong_arguments_are_refused_with_what_the_tool_takes() {
        let refused = |name: &str, arguments: Value| {
            let error = tool(name)
                .request(arguments.as_object().unwrap())
                .unwrap_err();
            assert_eq!(error.code, ErrorCode::Usage, "{name}");
            error.message
        };
        assert_eq!(
            refused("mic_use", json!({"microphone": "Yeti"})),
            "Ukjent argument «microphone». mic_use tar name."
        );
        assert_eq!(
            refused("last", json!({"count": 2})),
            "Ukjent argument «count». last tar ingen argumenter."
        );
        assert_eq!(
            refused("fix_add", json!({"from": "mercha"})),
            "fix_add trenger «to»."
        );
        assert_eq!(
            refused("vocab_add", json!({"word": 5})),
            "«word» til vocab_add er en tekst."
        );
        assert_eq!(
            refused("mic_order", json!({"names": "Yeti"})),
            "«names» til mic_order er en liste med tekster."
        );
    }

    #[test]
    fn the_setting_keys_are_those_config_takes() {
        let schema = tool("config_set").describe();
        let key = &schema["inputSchema"]["properties"]["key"];
        let names: Vec<&str> = SettingKey::ALL.iter().map(|key| key.name()).collect();
        assert_eq!(key["enum"], json!(names));
        let about = key["description"].as_str().unwrap();
        assert!(about.contains("language (bokmal, nynorsk)"), "{about}");
        assert!(about.contains("sounds (true, false)"), "{about}");
    }

    #[test]
    fn the_skill_names_every_tool_and_only_these() {
        let commands = crate::skill::FILES
            .iter()
            .find(|file| file.name == "commands.md")
            .unwrap()
            .text;
        for tool in &TOOLS {
            assert!(
                commands.contains(&format!("tool `{}`", tool.name)),
                "commands.md does not name the tool {}",
                tool.name
            );
        }
        for (index, _) in commands.match_indices("tool `") {
            let name = commands[index + 6..].split('`').next().unwrap();
            assert!(
                TOOLS.iter().any(|tool| tool.name == name),
                "commands.md names a tool {name} that does not exist"
            );
        }
    }

    #[test]
    fn a_client_gets_the_revision_it_asks_for_when_it_is_one_this_server_speaks() {
        for version in PROTOCOL_VERSIONS {
            let answer = initialize(&json!({"protocolVersion": version}));
            assert_eq!(answer["protocolVersion"], version);
        }
        for asked in [json!({"protocolVersion": "2099-01-01"}), json!({})] {
            assert_eq!(initialize(&asked)["protocolVersion"], PROTOCOL_VERSION);
        }
    }
}
