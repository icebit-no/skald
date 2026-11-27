//! What travels over the socket: one JSON object per line. The command line writes a request line, the
//! running Skald answers with one response line, and a connection may carry several in turn.
//!
//! ```text
//! → {"v":1,"request":{"command":"fix_add","from":"mercha","to":"merget"}}
//! ← {"v":1,"ok":{"output":"correction_added","from":"mercha","to":"merget","previous":null}}
//! ← {"v":1,"error":{"code":"not_found","message":"Fant ingen retting for «mercha»."}}
//! ```
//!
//! Every line carries the protocol version. The app and the command line are one binary (D-25), so the
//! versions differ only when Skald was updated while it ran; either side then answers with a `version`
//! error that tells the user to start Skald again.

use std::fmt;
use std::io::{self, BufRead, Read};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub const VERSION: u32 = 1;

/// A line longer than this is refused, so a broken peer cannot fill memory.
const MAX_LINE: usize = 1 << 20;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum Request {
    Last,
    History,
    FixList,
    FixAdd {
        from: String,
        to: String,
    },
    FixRemove {
        from: String,
    },
    VocabList,
    VocabAdd {
        word: String,
    },
    VocabRemove {
        word: String,
    },
    /// The sentence the ordskatt opens with; an empty one removes it.
    VocabSentence {
        sentence: String,
    },
    /// One setting, or all of them.
    ConfigGet {
        key: Option<String>,
    },
    ConfigSet {
        key: String,
        value: String,
    },
    MicList,
    /// Pins a microphone ahead of the order (D-20); `None` removes the pin, so the first connected
    /// microphone in the order is used.
    MicUse {
        name: Option<String>,
    },
    MicOrder {
        names: Vec<String>,
    },
    ModelList,
    ModelUse {
        id: String,
    },
    /// An absolute path, since the running Skald does not share the command line's working folder.
    Transcribe {
        path: PathBuf,
    },
    /// The hold key went down, as a key binding of the desktop reports it where Skald cannot hear the
    /// keyboard itself. Skald applies the rules it applies to a key it hears.
    Start,
    /// The hold key came up, as [`Request::Start`] reports the press.
    Stop,
    /// The hands-free chord was pressed, as a key binding of the desktop reports it: a hands-free take
    /// starts, or the one running stops.
    Toggle,
}

impl Request {
    /// Whether the request changes the configuration.
    pub fn changes_settings(&self) -> bool {
        matches!(
            self,
            Self::FixAdd { .. }
                | Self::FixRemove { .. }
                | Self::VocabAdd { .. }
                | Self::VocabRemove { .. }
                | Self::VocabSentence { .. }
                | Self::ConfigSet { .. }
                | Self::MicUse { .. }
                | Self::MicOrder { .. }
                | Self::ModelUse { .. }
        )
    }
}

/// The answer to a request, tagged by `output`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "output", rename_all = "snake_case")]
pub enum Output {
    Last(Last),
    History(Texts),
    Corrections(CorrectionList),
    CorrectionAdded(CorrectionAdded),
    CorrectionRemoved(CorrectionRemoved),
    Vocabulary(VocabularyList),
    WordAdded(WordChange),
    WordRemoved(WordChange),
    Settings(Settings),
    Setting(Setting),
    Microphones(MicrophoneList),
    MicrophonePinned(MicrophonePinned),
    MicrophoneOrder(MicrophoneOrder),
    Models(ModelList),
    ModelUsed(ModelUsed),
    Transcription(Transcription),
    Pressed,
    Released,
    Toggled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Last {
    /// `None` before the first take.
    pub last: Option<Text>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Texts {
    /// Newest first.
    pub texts: Vec<Text>,
}

/// A text Skald inserted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Text {
    /// After corrections, as it was inserted.
    pub text: String,
    /// When the take ended, in milliseconds since 1970-01-01 00:00 UTC; 0 when unknown.
    pub unix_ms: i64,
    pub microphone: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CorrectionList {
    /// In the order the user keeps them.
    pub corrections: Vec<CorrectionCount>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CorrectionCount {
    pub from: String,
    pub to: String,
    /// How many times the rule has applied.
    pub applied: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CorrectionAdded {
    pub from: String,
    pub to: String,
    /// The replacement the rule for these words had before, when there was one.
    pub previous: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CorrectionRemoved {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VocabularyList {
    pub sentence: String,
    /// In prompt order: the most important word is last.
    pub words: Vec<String>,
    pub tokens: Option<Tokens>,
}

/// The ordskatt meter: tokens the prompt takes, of the most the model keeps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tokens {
    pub used: usize,
    pub limit: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WordChange {
    pub word: String,
    /// False when adding a word that was already there.
    pub changed: bool,
    pub tokens: Option<Tokens>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    pub settings: std::collections::BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Setting {
    pub key: String,
    pub value: Value,
}

/// A setting's value, as the configuration file writes it: a JSON string or a JSON boolean.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Value {
    Bool(bool),
    Text(String),
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bool(value) => write!(f, "{value}"),
            Self::Text(value) => f.write_str(value),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MicrophoneList {
    /// The order's entries first, then the pinned microphone and the connected ones the order leaves out.
    pub microphones: Vec<MicrophoneEntry>,
    pub pinned: Option<String>,
    /// The microphone the next take uses; `None` when none is connected.
    pub active: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MicrophoneEntry {
    pub name: String,
    /// Its place in the order, from 1; `None` when it is not in the order.
    pub position: Option<usize>,
    pub connected: bool,
    pub pinned: bool,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MicrophonePinned {
    pub pinned: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MicrophoneOrder {
    pub order: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelList {
    pub models: Vec<ModelEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelEntry {
    pub id: String,
    pub name: String,
    pub downloaded: bool,
    /// The model the configuration names.
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelUsed {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Transcription {
    /// What Skald would insert: the model's text after corrections; `None` when it heard nothing.
    pub text: Option<String>,
    /// The model's text before corrections.
    pub model_text: Option<String>,
    /// The rules that changed the text, each once, in the order they first applied.
    pub corrections: Vec<CorrectionRule>,
    pub heard_nothing: Option<HeardNothing>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CorrectionRule {
    pub from: String,
    pub to: String,
}

/// Why a take ended with «Hørte ingenting» (docs/design.md, Silence).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HeardNothing {
    BelowSpeechLevel,
    TooLittleSpeech,
    NoWords,
    KnownPhrase,
}

/// A command that failed, with a message in bokmål that says plainly what is wrong.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Error {
    pub code: ErrorCode,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    /// The command line was given the wrong arguments.
    Usage,
    /// A correction, word, microphone, model or setting that does not exist.
    NotFound,
    /// A value the setting or command does not take.
    Invalid,
    /// A file Skald keeps that could not be read as it is.
    Unreadable,
    /// Reading or writing a file, or talking over the socket, failed.
    Io,
    /// The running Skald speaks another protocol version.
    Version,
    /// A line that is not a request or a response of this protocol.
    Protocol,
    /// The work itself failed, such as loading the model or transcribing.
    Failed,
    /// Skald is not running, and the command works only while it runs.
    NotRunning,
}

impl Error {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Error {}

#[derive(Debug, Serialize, Deserialize)]
struct RequestLine {
    v: u32,
    request: Request,
}

#[derive(Debug, Serialize, Deserialize)]
struct ResponseLine {
    v: u32,
    #[serde(flatten)]
    reply: Reply,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Reply {
    Ok(Output),
    Error(Error),
}

pub(crate) fn encode_request(request: &Request) -> String {
    let line = RequestLine {
        v: VERSION,
        request: request.clone(),
    };
    serde_json::to_string(&line).expect("a request always serialises")
}

/// Reads a request line. The version is checked before the request, so a client of another version gets
/// a `version` error rather than one about a command it named.
pub(crate) fn decode_request(line: &str) -> Result<Request, Error> {
    let value: serde_json::Value = serde_json::from_str(line)
        .map_err(|error| protocol_error(format!("Ugyldig forespørsel: {error}")))?;
    check_version(value.get("v").and_then(serde_json::Value::as_u64))?;
    let request = value
        .get("request")
        .ok_or_else(|| protocol_error("Forespørselen mangler kommandoen.".to_owned()))?;
    Request::deserialize(request)
        .map_err(|error| protocol_error(format!("Ugyldig forespørsel: {error}")))
}

pub(crate) fn encode_response(result: &Result<Output, Error>) -> String {
    let reply = match result {
        Ok(output) => Reply::Ok(output.clone()),
        Err(error) => Reply::Error(error.clone()),
    };
    let line = ResponseLine { v: VERSION, reply };
    serde_json::to_string(&line).expect("a response always serialises")
}

pub(crate) fn decode_response(line: &str) -> Result<Output, Error> {
    let value: serde_json::Value = serde_json::from_str(line)
        .map_err(|error| protocol_error(format!("Ugyldig svar fra Skald: {error}")))?;
    check_version(value.get("v").and_then(serde_json::Value::as_u64))?;
    let line = ResponseLine::deserialize(value)
        .map_err(|error| protocol_error(format!("Ugyldig svar fra Skald: {error}")))?;
    match line.reply {
        Reply::Ok(output) => Ok(output),
        Reply::Error(error) => Err(error),
    }
}

/// One line without its line break; `None` at the end of the stream. A line over [`MAX_LINE`] is an
/// error.
pub(crate) fn read_line(reader: &mut impl BufRead) -> io::Result<Option<String>> {
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

fn check_version(version: Option<u64>) -> Result<(), Error> {
    match version {
        Some(version) if version == u64::from(VERSION) => Ok(()),
        _ => Err(Error::new(
            ErrorCode::Version,
            "Skald som kjører, er en annen versjon enn denne kommandoen. Avslutt Skald og start den på \
             nytt.",
        )),
    }
}

fn protocol_error(message: String) -> Error {
    Error::new(ErrorCode::Protocol, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn every_request() -> Vec<Request> {
        vec![
            Request::Last,
            Request::History,
            Request::FixList,
            Request::FixAdd {
                from: "mercha".into(),
                to: "merget".into(),
            },
            Request::FixRemove {
                from: "Post gres SQL".into(),
            },
            Request::VocabList,
            Request::VocabAdd {
                word: "pull-requesten".into(),
            },
            Request::VocabRemove {
                word: "Nasjonalbiblioteket".into(),
            },
            Request::VocabSentence {
                sentence: "Vi merget «pull-requesten» i går.".into(),
            },
            Request::ConfigGet { key: None },
            Request::ConfigGet {
                key: Some("language".into()),
            },
            Request::ConfigSet {
                key: "sounds".into(),
                value: "av".into(),
            },
            Request::MicList,
            Request::MicUse {
                name: Some("MacBook Pro-mikrofon".into()),
            },
            Request::MicUse { name: None },
            Request::MicOrder {
                names: vec!["Shure MV7".into(), "Innebygd mikrofon".into()],
            },
            Request::ModelList,
            Request::ModelUse {
                id: "nb-whisper-large".into(),
            },
            Request::Transcribe {
                path: PathBuf::from("/Users/a/opptak æøå.wav"),
            },
            Request::Start,
            Request::Stop,
            Request::Toggle,
        ]
    }

    #[test]
    fn every_request_round_trips() {
        for request in every_request() {
            let line = encode_request(&request);
            assert!(!line.contains('\n'), "{line}");
            assert_eq!(decode_request(&line).unwrap(), request, "{line}");
        }
    }

    #[test]
    fn a_request_line_has_the_version_and_a_command() {
        let line = encode_request(&Request::FixAdd {
            from: "mercha".into(),
            to: "merget".into(),
        });
        assert_eq!(
            line,
            r#"{"v":1,"request":{"command":"fix_add","from":"mercha","to":"merget"}}"#
        );
    }

    #[test]
    fn every_output_round_trips() {
        let outputs = vec![
            Output::Last(Last {
                last: Some(Text {
                    text: "Hei, dette er en prøve.\nTo linjer.".into(),
                    unix_ms: 1_764_250_000_000,
                    microphone: "Shure MV7".into(),
                }),
            }),
            Output::Last(Last { last: None }),
            Output::Corrections(CorrectionList {
                corrections: vec![CorrectionCount {
                    from: "mercha".into(),
                    to: "merget".into(),
                    applied: 3,
                }],
            }),
            Output::Vocabulary(VocabularyList {
                sentence: "Vi merget pull-requesten.".into(),
                words: vec!["PostgreSQL".into()],
                tokens: Some(Tokens {
                    used: 12,
                    limit: 223,
                }),
            }),
            Output::Settings(Settings {
                settings: [
                    ("sounds".to_owned(), Value::Bool(false)),
                    ("language".to_owned(), Value::Text("nynorsk".into())),
                ]
                .into(),
            }),
            Output::Transcription(Transcription {
                text: None,
                model_text: None,
                corrections: vec![],
                heard_nothing: Some(HeardNothing::TooLittleSpeech),
            }),
            Output::Pressed,
            Output::Released,
            Output::Toggled,
        ];
        for output in outputs {
            let line = encode_response(&Ok(output.clone()));
            assert_eq!(decode_response(&line).unwrap(), output, "{line}");
        }
    }

    #[test]
    fn an_error_round_trips() {
        let error = Error::new(ErrorCode::NotFound, "Fant ingen retting for «mercha».");
        let line = encode_response(&Err(error.clone()));
        assert_eq!(
            line,
            r#"{"v":1,"error":{"code":"not_found","message":"Fant ingen retting for «mercha»."}}"#
        );
        assert_eq!(decode_response(&line).unwrap_err(), error);
    }

    #[test]
    fn another_version_is_refused_before_the_command_is_read() {
        let error = decode_request(r#"{"v":2,"request":{"command":"something_new"}}"#).unwrap_err();
        assert_eq!(error.code, ErrorCode::Version);
        let error = decode_request(r#"{"request":{"command":"last"}}"#).unwrap_err();
        assert_eq!(error.code, ErrorCode::Version);
        let error = decode_response(r#"{"v":2,"ok":{"output":"last","last":null}}"#).unwrap_err();
        assert_eq!(error.code, ErrorCode::Version);
    }

    #[test]
    fn a_broken_line_is_a_protocol_error() {
        for line in [
            "",
            "ikke json",
            r#"{"v":1}"#,
            r#"{"v":1,"request":{"command":"nope"}}"#,
            r#"{"v":1,"request":{"command":"fix_add","from":"x"}}"#,
        ] {
            assert_eq!(
                decode_request(line).unwrap_err().code,
                ErrorCode::Protocol,
                "{line}"
            );
        }
    }
}
