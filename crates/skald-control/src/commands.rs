//! Every command, carried out on top of [`Skald`]: by the server for the running app, and by the command
//! line on the files while Skald is not running.

use skald_core::Config;
use skald_core::config::{HoldKey, Language, OverlayTheme};
use skald_core::corrections::Correction;
use skald_core::history::Entry;
use skald_core::silence_gate::Reason;
use skald_core::vocabulary::Vocabulary;

use crate::protocol::{
    CorrectionAdded, CorrectionCount, CorrectionList, CorrectionRemoved, CorrectionRule, Error,
    ErrorCode, HeardNothing, Last, MicrophoneEntry, MicrophoneList, MicrophoneOrder,
    MicrophonePinned, ModelEntry, ModelList, ModelUsed, Output, Request, Setting, Settings, Text,
    Texts, Transcription, Value, VocabularyList, WordChange,
};
use crate::skald::{Microphones, Skald, Transcribed};

pub fn execute(request: &Request, skald: &dyn Skald) -> Result<Output, Error> {
    match request {
        Request::Last => Ok(Output::Last(Last {
            last: skald.history()?.latest().map(text),
        })),
        Request::History => Ok(Output::History(Texts {
            texts: skald.history()?.entries().iter().map(text).collect(),
        })),
        Request::FixList => fix_list(skald),
        Request::FixAdd { from, to } => fix_add(skald, from, to),
        Request::FixRemove { from } => fix_remove(skald, from),
        Request::VocabList => {
            let vocabulary = skald.config()?.vocabulary;
            Ok(Output::Vocabulary(vocabulary_list(skald, &vocabulary)))
        }
        Request::VocabAdd { word } => vocab_add(skald, word),
        Request::VocabRemove { word } => vocab_remove(skald, word),
        Request::VocabSentence { sentence } => vocab_sentence(skald, sentence),
        Request::ConfigGet { key: None } => {
            let config = skald.config()?;
            Ok(Output::Settings(Settings {
                settings: SettingKey::ALL
                    .iter()
                    .map(|key| (key.name().to_owned(), key.get(&config)))
                    .collect(),
            }))
        }
        Request::ConfigGet { key: Some(name) } => {
            let key = SettingKey::find(name)?;
            Ok(Output::Setting(Setting {
                key: key.name().to_owned(),
                value: key.get(&skald.config()?),
            }))
        }
        Request::ConfigSet { key, value } => config_set(skald, key, value),
        Request::MicList => mic_list(skald),
        Request::MicUse { name } => mic_use(skald, name.as_deref()),
        Request::MicOrder { names } => mic_order(skald, names),
        Request::ModelList => model_list(skald),
        Request::ModelUse { id } => model_use(skald, id),
        Request::Transcribe { path } => transcribe(skald, path),
    }
}

fn text(entry: &Entry) -> Text {
    Text {
        text: entry.text.clone(),
        unix_ms: entry.unix_ms,
        microphone: entry.microphone.clone(),
    }
}

fn fix_list(skald: &dyn Skald) -> Result<Output, Error> {
    let config = skald.config()?;
    let counts = skald.correction_counts()?;
    Ok(Output::Corrections(CorrectionList {
        corrections: config
            .corrections
            .rules
            .iter()
            .map(|rule| CorrectionCount {
                from: rule.from.clone(),
                to: rule.to.clone(),
                applied: counts.get(&rule.from),
            })
            .collect(),
    }))
}

fn fix_add(skald: &dyn Skald, from: &str, to: &str) -> Result<Output, Error> {
    let (from, to) = (from.trim(), to.trim());
    if from.is_empty() {
        return Err(invalid("En retting trenger ordene som skal byttes ut."));
    }
    if to.is_empty() {
        return Err(invalid(
            "En retting trenger teksten ordene skal byttes med.",
        ));
    }
    let mut previous = None;
    skald.change_config(&mut |config| {
        let before = config.corrections.rules.clone();
        config
            .corrections
            .add(from, to)
            .map_err(|_| invalid("En retting trenger ordene som skal byttes ut."))?;
        let after = &config.corrections.rules;
        // A rule for the same words takes the new replacement in place; one that was already exactly
        // this leaves the rules as they were.
        previous = (after.len() == before.len()).then(|| match first_difference(&before, after) {
            Some(index) => before[index].to.clone(),
            None => to.to_owned(),
        });
        Ok(())
    })?;
    Ok(Output::CorrectionAdded(CorrectionAdded {
        from: from.to_owned(),
        to: to.to_owned(),
        previous,
    }))
}

fn fix_remove(skald: &dyn Skald, from: &str) -> Result<Output, Error> {
    let mut removed = None;
    skald.change_config(&mut |config| {
        let before = config.corrections.rules.clone();
        if !config.corrections.remove(from) {
            return Err(not_found(format!(
                "Fant ingen retting for «{}».",
                from.trim()
            )));
        }
        removed = first_difference(&before, &config.corrections.rules).map(|i| before[i].clone());
        Ok(())
    })?;
    let rule = removed.expect("a removed rule leaves the rules different");
    Ok(Output::CorrectionRemoved(CorrectionRemoved {
        from: rule.from,
        to: rule.to,
    }))
}

/// The first place the rules differ, counting a rule missing at the end of `after` as a difference.
fn first_difference(before: &[Correction], after: &[Correction]) -> Option<usize> {
    (0..before.len()).find(|&index| after.get(index) != Some(&before[index]))
}

fn vocabulary_list(skald: &dyn Skald, vocabulary: &Vocabulary) -> VocabularyList {
    VocabularyList {
        sentence: vocabulary.sentence.clone(),
        words: vocabulary.words.clone(),
        tokens: skald.prompt_tokens(&vocabulary.prompt()),
    }
}

fn vocab_add(skald: &dyn Skald, word: &str) -> Result<Output, Error> {
    let word = word.trim();
    if word.is_empty() {
        return Err(invalid("Gi ordet som skal inn i ordskatten."));
    }
    let mut added = false;
    let mut vocabulary = Vocabulary::default();
    skald.change_config(&mut |config| {
        added = config.vocabulary.add_word(word);
        vocabulary = config.vocabulary.clone();
        Ok(())
    })?;
    Ok(Output::WordAdded(WordChange {
        word: word.to_owned(),
        changed: added,
        tokens: skald.prompt_tokens(&vocabulary.prompt()),
    }))
}

fn vocab_remove(skald: &dyn Skald, word: &str) -> Result<Output, Error> {
    let word = word.trim();
    if word.is_empty() {
        return Err(invalid("Gi ordet som skal ut av ordskatten."));
    }
    let mut vocabulary = Vocabulary::default();
    skald.change_config(&mut |config| {
        if !config.vocabulary.remove_word(word) {
            return Err(not_found(format!("«{word}» står ikke i ordskatten.")));
        }
        vocabulary = config.vocabulary.clone();
        Ok(())
    })?;
    Ok(Output::WordRemoved(WordChange {
        word: word.to_owned(),
        changed: true,
        tokens: skald.prompt_tokens(&vocabulary.prompt()),
    }))
}

fn vocab_sentence(skald: &dyn Skald, sentence: &str) -> Result<Output, Error> {
    let mut vocabulary = Vocabulary::default();
    skald.change_config(&mut |config| {
        config.vocabulary.sentence = sentence.trim().to_owned();
        vocabulary = config.vocabulary.clone();
        Ok(())
    })?;
    Ok(Output::Vocabulary(vocabulary_list(skald, &vocabulary)))
}

fn config_set(skald: &dyn Skald, name: &str, value: &str) -> Result<Output, Error> {
    let key = SettingKey::find(name)?;
    let mut written = None;
    skald.change_config(&mut |config| {
        key.set(config, value.trim())?;
        written = Some(key.get(config));
        Ok(())
    })?;
    Ok(Output::Setting(Setting {
        key: key.name().to_owned(),
        value: written.expect("a set setting has a value"),
    }))
}

fn mic_list(skald: &dyn Skald) -> Result<Output, Error> {
    let config = skald.config()?;
    let Microphones { connected, active } = skald.microphones()?;
    let pinned = config.pinned_microphone.clone();
    let mut names: Vec<&String> = Vec::new();
    for name in config.microphones.iter().chain(&pinned).chain(&connected) {
        if !names.contains(&name) {
            names.push(name);
        }
    }
    let microphones = names
        .into_iter()
        .map(|name| MicrophoneEntry {
            name: name.clone(),
            position: config
                .microphones
                .iter()
                .position(|entry| entry == name)
                .map(|index| index + 1),
            connected: connected.contains(name),
            pinned: pinned.as_ref() == Some(name),
            active: active.as_ref() == Some(name),
        })
        .collect();
    Ok(Output::Microphones(MicrophoneList {
        microphones,
        pinned,
        active,
    }))
}

/// The name of a microphone the user means: one that is connected, in the order or pinned, matched as
/// written, else ignoring case.
fn known_microphone(
    name: &str,
    config: &Config,
    microphones: &Microphones,
) -> Result<String, Error> {
    let name = name.trim();
    let known: Vec<&String> = microphones
        .connected
        .iter()
        .chain(&config.microphones)
        .chain(&config.pinned_microphone)
        .collect();
    known
        .iter()
        .find(|known| known.as_str() == name)
        .or_else(|| {
            known
                .iter()
                .find(|known| known.to_lowercase() == name.to_lowercase())
        })
        .map(|known| (*known).clone())
        .ok_or_else(|| {
            let connected = if microphones.connected.is_empty() {
                "Ingen mikrofon er koblet til nå.".to_owned()
            } else {
                format!("Koblet til nå: {}.", microphones.connected.join(", "))
            };
            not_found(format!(
                "Kjenner ingen mikrofon som heter «{name}». {connected}"
            ))
        })
}

fn mic_use(skald: &dyn Skald, name: Option<&str>) -> Result<Output, Error> {
    let pinned = match name {
        Some(name) => Some(known_microphone(
            name,
            &skald.config()?,
            &skald.microphones()?,
        )?),
        None => None,
    };
    skald.change_config(&mut |config| {
        config.pinned_microphone = pinned.clone();
        Ok(())
    })?;
    Ok(Output::MicrophonePinned(MicrophonePinned { pinned }))
}

fn mic_order(skald: &dyn Skald, names: &[String]) -> Result<Output, Error> {
    if names.is_empty() {
        return Err(Error::new(
            ErrorCode::Usage,
            "Gi mikrofonene i den rekkefølgen de skal brukes.",
        ));
    }
    let config = skald.config()?;
    let microphones = skald.microphones()?;
    let mut order: Vec<String> = Vec::new();
    for name in names {
        let name = known_microphone(name, &config, &microphones)?;
        if order.contains(&name) {
            return Err(invalid(format!("«{name}» står to ganger i rekkefølgen.")));
        }
        order.push(name);
    }
    skald.change_config(&mut |config| {
        config.microphones = order.clone();
        Ok(())
    })?;
    Ok(Output::MicrophoneOrder(MicrophoneOrder { order }))
}

fn model_list(skald: &dyn Skald) -> Result<Output, Error> {
    let active = skald.config()?.model;
    Ok(Output::Models(ModelList {
        models: skald
            .models()?
            .into_iter()
            .map(|model| ModelEntry {
                active: model.id == active,
                id: model.id,
                name: model.name,
                downloaded: model.downloaded,
            })
            .collect(),
    }))
}

fn model_use(skald: &dyn Skald, id: &str) -> Result<Output, Error> {
    let id = id.trim();
    let models = skald.models()?;
    let model = models
        .iter()
        .find(|model| model.id == id)
        .or_else(|| {
            models.iter().find(|model| {
                model.id.eq_ignore_ascii_case(id) || model.name.to_lowercase() == id.to_lowercase()
            })
        })
        .ok_or_else(|| {
            let ids: Vec<&str> = models.iter().map(|model| model.id.as_str()).collect();
            not_found(format!(
                "Kjenner ingen talemodell som heter «{id}». Velg en av: {}.",
                ids.join(", ")
            ))
        })?;
    if !model.downloaded {
        return Err(invalid(format!("{} er ikke lastet ned ennå.", model.name)));
    }
    skald.change_config(&mut |config| {
        config.model = model.id.clone();
        Ok(())
    })?;
    Ok(Output::ModelUsed(ModelUsed {
        id: model.id.clone(),
        name: model.name.clone(),
    }))
}

fn transcribe(skald: &dyn Skald, path: &std::path::Path) -> Result<Output, Error> {
    if !path.is_absolute() {
        return Err(invalid(format!(
            "Lydfilen må gis med full sti: {}",
            path.display()
        )));
    }
    let transcription = match skald.transcribe(path)? {
        Transcribed::Text(model_text) => {
            let corrections = skald.config()?.corrections;
            let corrected = corrections.apply(&model_text);
            let rules = &corrections.rules;
            let mut used: Vec<usize> = Vec::new();
            for &index in &corrected.replacements {
                if !used.contains(&index) {
                    used.push(index);
                }
            }
            Transcription {
                text: Some(corrected.text),
                model_text: Some(model_text),
                corrections: used
                    .into_iter()
                    .map(|index| CorrectionRule {
                        from: rules[index].from.clone(),
                        to: rules[index].to.clone(),
                    })
                    .collect(),
                heard_nothing: None,
            }
        }
        Transcribed::HeardNothing(reason) => Transcription {
            text: None,
            model_text: None,
            corrections: Vec::new(),
            heard_nothing: Some(match reason {
                Reason::BelowSpeechLevel => HeardNothing::BelowSpeechLevel,
                Reason::TooLittleSpeech => HeardNothing::TooLittleSpeech,
                Reason::NoWords => HeardNothing::NoWords,
                Reason::KnownPhrase => HeardNothing::KnownPhrase,
            }),
        },
    };
    Ok(Output::Transcription(transcription))
}

fn invalid(message: impl Into<String>) -> Error {
    Error::new(ErrorCode::Invalid, message)
}

fn not_found(message: impl Into<String>) -> Error {
    Error::new(ErrorCode::NotFound, message)
}

/// The settings `skald config` reads and changes: those in the settings window's Generelt tab that have no
/// command of their own. Each is named by its key in the configuration file and takes the values the file
/// does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SettingKey {
    HoldKey,
    Language,
    OverlayTheme,
    CopyToClipboard,
    StartAtLogin,
    Sounds,
}

enum Choices {
    Bool,
    Words(&'static [&'static str]),
}

impl SettingKey {
    pub(crate) const ALL: [SettingKey; 6] = [
        SettingKey::HoldKey,
        SettingKey::Language,
        SettingKey::OverlayTheme,
        SettingKey::CopyToClipboard,
        SettingKey::StartAtLogin,
        SettingKey::Sounds,
    ];

    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::HoldKey => "hold_key",
            Self::Language => "language",
            Self::OverlayTheme => "overlay_theme",
            Self::CopyToClipboard => "copy_to_clipboard",
            Self::StartAtLogin => "start_at_login",
            Self::Sounds => "sounds",
        }
    }

    fn choices(self) -> Choices {
        match self {
            Self::HoldKey => Choices::Words(&["right-command", "right-control"]),
            Self::Language => Choices::Words(&["bokmal", "nynorsk"]),
            Self::OverlayTheme => Choices::Words(&["runepinne", "smidd-jern", "kjedelig"]),
            Self::CopyToClipboard | Self::StartAtLogin | Self::Sounds => Choices::Bool,
        }
    }

    /// The values it takes, as the file writes them.
    pub(crate) fn accepted(self) -> &'static [&'static str] {
        match self.choices() {
            Choices::Bool => &["true", "false"],
            Choices::Words(words) => words,
        }
    }

    /// The values it takes, «a, b eller c».
    pub(crate) fn values(self) -> String {
        either(self.accepted())
    }

    /// A key as the file writes it, or with hyphens for underscores.
    fn find(name: &str) -> Result<SettingKey, Error> {
        let name = name.trim();
        let normalised = name.replace('-', "_");
        if let Some(key) = Self::ALL.into_iter().find(|key| key.name() == normalised) {
            return Ok(key);
        }
        let elsewhere = match normalised.split('.').next().unwrap_or_default() {
            "model" => Some("Talemodellen byttes med «skald model use»."),
            "microphones" | "pinned_microphone" => {
                Some("Mikrofonene styres med «skald mic order» og «skald mic use».")
            }
            "vocabulary" => Some("Ordskatten endres med «skald vocab»."),
            "corrections" => Some("Rettingene endres med «skald fix»."),
            _ => None,
        };
        let message = match elsewhere {
            Some(hint) => format!("«{name}» endres ikke med «skald config». {hint}"),
            None => {
                let names: Vec<&str> = Self::ALL.iter().map(|key| key.name()).collect();
                format!(
                    "Kjenner ingen innstilling som heter «{name}». Innstillingene er: {}.",
                    names.join(", ")
                )
            }
        };
        Err(not_found(message))
    }

    pub(crate) fn get(self, config: &Config) -> Value {
        match self {
            Self::HoldKey => Value::Text(config.hold_key.clone().into()),
            Self::Language => Value::Text(
                match config.language {
                    Language::Bokmal => "bokmal",
                    Language::Nynorsk => "nynorsk",
                }
                .to_owned(),
            ),
            Self::OverlayTheme => Value::Text(config.overlay_theme.clone().into()),
            Self::CopyToClipboard => Value::Bool(config.copy_to_clipboard),
            Self::StartAtLogin => Value::Bool(config.start_at_login),
            Self::Sounds => Value::Bool(config.sounds),
        }
    }

    /// Sets a value this version knows; a value only a newer Skald knows is kept when the file has it,
    /// but never written from here.
    fn set(self, config: &mut Config, value: &str) -> Result<(), Error> {
        match self.choices() {
            Choices::Bool => {
                let value = parse_bool(value).ok_or_else(|| {
                    invalid(format!(
                        "«{value}» er ikke en verdi for {}. Velg {}.",
                        self.name(),
                        self.values()
                    ))
                })?;
                match self {
                    Self::CopyToClipboard => config.copy_to_clipboard = value,
                    Self::StartAtLogin => config.start_at_login = value,
                    Self::Sounds => config.sounds = value,
                    _ => unreachable!("only these keys take true or false"),
                }
            }
            Choices::Words(words) => {
                let word = if self == Self::Language && value == "bokmål" {
                    "bokmal"
                } else {
                    value
                };
                if !words.contains(&word) {
                    return Err(invalid(format!(
                        "«{value}» er ikke en verdi for {}. Velg {}.",
                        self.name(),
                        self.values()
                    )));
                }
                match self {
                    Self::HoldKey => config.hold_key = HoldKey::from(word.to_owned()),
                    Self::Language => {
                        config.language = if word == "nynorsk" {
                            Language::Nynorsk
                        } else {
                            Language::Bokmal
                        }
                    }
                    Self::OverlayTheme => {
                        config.overlay_theme = OverlayTheme::from(word.to_owned())
                    }
                    _ => unreachable!("only these keys take one of a set of words"),
                }
            }
        }
        Ok(())
    }
}

fn parse_bool(value: &str) -> Option<bool> {
    match value.to_lowercase().as_str() {
        "true" | "ja" | "på" => Some(true),
        "false" | "nei" | "av" => Some(false),
        _ => None,
    }
}

/// «a, b eller c».
fn either(words: &[&str]) -> String {
    match words {
        [] => String::new(),
        [only] => (*only).to_owned(),
        [rest @ .., last] => format!("{} eller {last}", rest.join(", ")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_setting_reads_as_the_file_writes_it() {
        let mut changed = Config::default();
        changed.hold_key = HoldKey::RightControl;
        changed.language = Language::Nynorsk;
        changed.overlay_theme = OverlayTheme::SmiddJern;
        changed.copy_to_clipboard = true;
        changed.start_at_login = true;
        changed.sounds = false;
        for config in [Config::default(), changed] {
            let file: toml::Table = toml::from_str(&config.to_toml().unwrap()).unwrap();
            for key in SettingKey::ALL {
                let written = &file[key.name()];
                match key.get(&config) {
                    Value::Bool(value) => assert_eq!(written.as_bool(), Some(value)),
                    Value::Text(value) => assert_eq!(written.as_str(), Some(value.as_str())),
                }
            }
        }
    }

    #[test]
    fn every_choice_is_a_value_the_configuration_knows() {
        for key in SettingKey::ALL {
            let Choices::Words(words) = key.choices() else {
                continue;
            };
            for word in words {
                let mut config = Config::default();
                key.set(&mut config, word).unwrap();
                assert_eq!(key.get(&config), Value::Text((*word).to_owned()));
                assert_ne!(config.hold_key, HoldKey::Unknown((*word).to_owned()));
                assert_ne!(
                    config.overlay_theme,
                    OverlayTheme::Unknown((*word).to_owned())
                );
            }
        }
    }

    #[test]
    fn a_value_only_a_newer_skald_knows_is_refused() {
        let mut config = Config::default();
        let error = SettingKey::HoldKey
            .set(&mut config, "right-option")
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::Invalid);
        assert_eq!(
            error.message,
            "«right-option» er ikke en verdi for hold_key. Velg right-command eller right-control."
        );
        assert_eq!(config, Config::default());
    }

    #[test]
    fn booleans_take_norwegian_words_too() {
        for (word, expected) in [("ja", true), ("Av", false), ("på", true), ("FALSE", false)] {
            assert_eq!(parse_bool(word), Some(expected), "{word}");
        }
        assert_eq!(parse_bool("kanskje"), None);
    }

    #[test]
    fn either_joins_with_eller() {
        assert_eq!(either(&["a"]), "a");
        assert_eq!(either(&["a", "b", "c"]), "a, b eller c");
    }
}
