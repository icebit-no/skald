//! The microphone the recorder takes from: found by the name the system gives it, and recorded through
//! `skald-audio` as Skald records a take, resampled to 16 kHz mono.

use skald_audio::{CaptureError, Connected, Microphone, TakeEvent};

use super::session::{Capture, Captured, Heard, Running};

/// The microphone named, or the system's default input when none is. A name matches a microphone
/// that has it exactly, then one that has it in another case, then the one microphone whose name
/// contains it.
pub(super) fn find(connected: &Connected, name: Option<&str>) -> Result<Microphone, String> {
    let all = &connected.microphones;
    if all.is_empty() {
        return Err("Ingen mikrofon er koblet til.".to_owned());
    }
    let Some(name) = name else {
        return skald_audio::choose(&[], connected)
            .cloned()
            .ok_or_else(|| "Ingen mikrofon er koblet til.".to_owned());
    };
    let wanted = name.to_lowercase();
    let exact = all.iter().find(|m| m.name == name);
    let other_case = || all.iter().find(|m| m.name.to_lowercase() == wanted);
    if let Some(microphone) = exact.or_else(other_case) {
        return Ok(microphone.clone());
    }
    let containing: Vec<&Microphone> = all
        .iter()
        .filter(|m| m.name.to_lowercase().contains(&wanted))
        .collect();
    match containing.as_slice() {
        [one] => Ok((*one).clone()),
        [] => Err(format!(
            "Ingen mikrofon heter «{name}». Koblet til: {}.",
            names(all.iter())
        )),
        many => Err(format!(
            "«{name}» passer med flere mikrofoner: {}. Skriv mer av navnet.",
            names(many.iter().copied())
        )),
    }
}

fn names<'a>(microphones: impl Iterator<Item = &'a Microphone>) -> String {
    microphones
        .map(|m| format!("«{}»", m.name))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Why a microphone could not be listed or opened, as the rest of a sentence.
pub(super) fn reason(error: &CaptureError) -> String {
    match error {
        CaptureError::NotConnected => "mikrofonen er ikke koblet til".into(),
        CaptureError::PermissionDenied => "Skald har ikke tilgang til mikrofonen".into(),
        CaptureError::Busy => "et annet program bruker den".into(),
        CaptureError::UnsupportedFormat(detail) => format!("lydformatet støttes ikke ({detail})"),
        CaptureError::System(detail) => format!("lydsystemet feilet ({detail})"),
    }
}

pub(super) struct System(pub(super) Microphone);

impl Capture for System {
    type Take = skald_audio::Take;

    fn start(&mut self) -> Result<skald_audio::Take, String> {
        skald_audio::Take::start(&self.0).map_err(|e| reason(&e))
    }
}

impl Running for skald_audio::Take {
    fn heard(&mut self) -> Vec<Heard> {
        self.events()
            .try_iter()
            .map(|event| match event {
                TakeEvent::Level(reading) => Heard::Level(reading),
                TakeEvent::Capped => Heard::Capped,
                TakeEvent::MicrophoneLost => Heard::Lost,
            })
            .collect()
    }

    fn stop(self) -> Captured {
        let recording = skald_audio::Take::stop(self);
        Captured {
            loudest_dbfs: recording.loudest_dbfs(),
            samples: recording.samples,
        }
    }
}

#[cfg(test)]
mod tests {
    use skald_audio::MicId;

    use super::*;

    fn connected(names: &[&str], default: Option<usize>) -> Connected {
        Connected {
            microphones: names
                .iter()
                .enumerate()
                .map(|(i, name)| Microphone {
                    id: MicId::new(format!("id-{i}")),
                    name: (*name).to_owned(),
                })
                .collect(),
            default: default.map(|i| MicId::new(format!("id-{i}"))),
        }
    }

    fn found(connected: &Connected, name: Option<&str>) -> Result<String, String> {
        find(connected, name).map(|m| m.name)
    }

    #[test]
    fn without_a_name_the_default_input_is_used() {
        let both = connected(&["MacBook Pro-mikrofon", "Shure MV7"], Some(1));
        assert_eq!(found(&both, None).unwrap(), "Shure MV7");
        let no_default = connected(&["MacBook Pro-mikrofon", "Shure MV7"], None);
        assert_eq!(found(&no_default, None).unwrap(), "MacBook Pro-mikrofon");
    }

    #[test]
    fn a_name_matches_exactly_then_in_any_case_then_in_part() {
        let mics = connected(
            &["Shure MV7", "shure mv7 x", "MacBook Pro-mikrofon"],
            Some(2),
        );
        assert_eq!(found(&mics, Some("shure mv7 x")).unwrap(), "shure mv7 x");
        assert_eq!(found(&mics, Some("SHURE MV7")).unwrap(), "Shure MV7");
        assert_eq!(
            found(&mics, Some("macbook")).unwrap(),
            "MacBook Pro-mikrofon"
        );
    }

    #[test]
    fn a_name_that_matches_none_or_several_is_explained() {
        let mics = connected(&["Shure MV7", "Shure MV7 X", "MacBook Pro-mikrofon"], None);
        assert_eq!(
            found(&mics, Some("Yeti")).unwrap_err(),
            "Ingen mikrofon heter «Yeti». Koblet til: «Shure MV7», «Shure MV7 X», «MacBook Pro-mikrofon»."
        );
        assert_eq!(
            found(&mics, Some("shure mv")).unwrap_err(),
            "«shure mv» passer med flere mikrofoner: «Shure MV7», «Shure MV7 X». Skriv mer av navnet."
        );
        assert_eq!(
            found(&connected(&[], None), Some("Yeti")).unwrap_err(),
            "Ingen mikrofon er koblet til."
        );
    }
}
