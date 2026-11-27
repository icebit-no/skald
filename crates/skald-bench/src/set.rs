//! A test set: a folder of takes, each a WAV file with its reference beside it in a `.txt` file of the same
//! name, as research/clips/fetch.py writes them:
//!
//! ```text
//! setning-01-usb.wav   the take
//! setning-01-usb.txt   what was said, in bokmål, one line or several
//! manifest.toml        optional: the microphone of each take
//! ```
//!
//! The manifest names the microphone each take was recorded with, by the take's name (its file name
//! without `.wav`), so results can be broken down per microphone. A `*` in a name matches any run of
//! characters. A take matched by no line has no microphone; a take matched by two lines, or a line that
//! matches no take, is an error, since either is a mistake in the manifest.
//!
//! ```toml
//! [microphones]
//! "*-usb" = "usb"
//! "*-mac" = "built-in"
//! "*-far" = "distance"
//! ```
//!
//! Only the folder itself is read, not folders inside it, and files whose names start with a full stop
//! are skipped.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

pub const MANIFEST: &str = "manifest.toml";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Clip {
    /// The file name without `.wav`.
    pub name: String,
    pub wav: PathBuf,
    pub reference: String,
    pub microphone: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    #[serde(default)]
    microphones: BTreeMap<String, String>,
}

/// The takes of a test set, in the order of their names.
pub fn load(folder: &Path) -> Result<Vec<Clip>, String> {
    let entries = std::fs::read_dir(folder).map_err(|e| format!("{}: {e}", folder.display()))?;
    let mut wavs = Vec::new();
    for entry in entries {
        let path = entry
            .map_err(|e| format!("{}: {e}", folder.display()))?
            .path();
        let is_wav = path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("wav"));
        let hidden = path
            .file_name()
            .and_then(|name| name.to_str())
            .is_none_or(|name| name.starts_with('.'));
        if is_wav && !hidden && path.is_file() {
            wavs.push(path);
        }
    }
    wavs.sort();
    if wavs.is_empty() {
        return Err(format!("Ingen WAV-filer i {}.", folder.display()));
    }

    let mut clips = Vec::with_capacity(wavs.len());
    let mut missing = Vec::new();
    for wav in wavs {
        let name = wav
            .file_stem()
            .and_then(|stem| stem.to_str())
            .ok_or_else(|| format!("Navnet på {} er ikke gyldig UTF-8.", wav.display()))?
            .to_owned();
        let txt = wav.with_extension("txt");
        if !txt.is_file() {
            missing.push(format!("{name}.txt"));
            continue;
        }
        let reference = std::fs::read_to_string(&txt)
            .map_err(|e| format!("{}: {e}", txt.display()))?
            .trim()
            .to_owned();
        clips.push(Clip {
            name,
            wav,
            reference,
            microphone: None,
        });
    }
    if !missing.is_empty() {
        return Err(format!(
            "Hvert opptak i {} trenger referanseteksten sin i en .txt-fil med samme navn. Mangler: {}.",
            folder.display(),
            missing.join(", ")
        ));
    }

    let manifest_path = folder.join(MANIFEST);
    if manifest_path.is_file() {
        let text = std::fs::read_to_string(&manifest_path)
            .map_err(|e| format!("{}: {e}", manifest_path.display()))?;
        assign_microphones(&mut clips, &text)
            .map_err(|e| format!("{}: {e}", manifest_path.display()))?;
    }
    Ok(clips)
}

fn assign_microphones(clips: &mut [Clip], manifest: &str) -> Result<(), String> {
    let manifest: Manifest = toml::from_str(manifest).map_err(|e| e.to_string())?;
    let mut used: BTreeMap<&str, bool> = manifest
        .microphones
        .keys()
        .map(|pattern| (pattern.as_str(), false))
        .collect();
    for clip in clips.iter_mut() {
        let mut matched: Option<(&str, &str)> = None;
        for (pattern, microphone) in &manifest.microphones {
            if !glob_matches(pattern, &clip.name) {
                continue;
            }
            if let Some((first, _)) = matched {
                return Err(format!(
                    "Opptaket {} passer med både «{first}» og «{pattern}».",
                    clip.name
                ));
            }
            matched = Some((pattern, microphone));
            used.insert(pattern, true);
        }
        clip.microphone = matched.map(|(_, microphone)| microphone.to_owned());
    }
    let unused: Vec<String> = used
        .into_iter()
        .filter(|&(_, used)| !used)
        .map(|(pattern, _)| format!("«{pattern}»"))
        .collect();
    if !unused.is_empty() {
        return Err(format!("Ingen opptak passer med {}.", unused.join(", ")));
    }
    Ok(())
}

/// Whether `name` matches `pattern`, where `*` matches any run of characters, none included.
fn glob_matches(pattern: &str, name: &str) -> bool {
    let mut parts = pattern.split('*');
    let first = parts.next().unwrap_or_default();
    let Some(mut rest) = name.strip_prefix(first) else {
        return false;
    };
    let middle: Vec<&str> = parts.collect();
    let Some((last, middle)) = middle.split_last() else {
        return rest.is_empty();
    };
    for part in middle {
        match rest.find(part) {
            Some(at) => rest = &rest[at + part.len()..],
            None => return false,
        }
    }
    rest.len() >= last.len() && rest.ends_with(last)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Folder(PathBuf);

    impl Folder {
        fn new(name: &str) -> Self {
            let path =
                std::env::temp_dir().join(format!("skald-bench-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn write(&self, name: &str, contents: &str) -> &Self {
            std::fs::write(self.0.join(name), contents).unwrap();
            self
        }
    }

    impl Drop for Folder {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn clip(name: &str, microphone: Option<&str>) -> Clip {
        Clip {
            name: name.to_owned(),
            wav: PathBuf::new(),
            reference: String::new(),
            microphone: microphone.map(str::to_owned),
        }
    }

    #[test]
    fn takes_are_read_with_their_references_in_name_order() {
        let folder = Folder::new("order");
        folder
            .write("b.wav", "")
            .write("b.txt", "Andre setning.\n")
            .write("a.WAV", "")
            .write("a.txt", "  Første\nsetning. \n")
            .write("notes.txt", "not a take")
            .write(".hidden.wav", "");
        std::fs::create_dir(folder.0.join(".download")).unwrap();
        let clips = load(&folder.0).unwrap();
        let names: Vec<&str> = clips.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["a", "b"]);
        assert_eq!(clips[0].reference, "Første\nsetning.");
        assert_eq!(clips[1].wav, folder.0.join("b.wav"));
        assert!(clips.iter().all(|c| c.microphone.is_none()));
    }

    #[test]
    fn a_take_without_a_reference_is_an_error() {
        let folder = Folder::new("missing");
        folder
            .write("a.wav", "")
            .write("b.wav", "")
            .write("a.txt", "Hei.");
        let error = load(&folder.0).unwrap_err();
        assert!(error.ends_with("Mangler: b.txt."), "{error}");
    }

    #[test]
    fn a_folder_without_takes_is_an_error() {
        let folder = Folder::new("empty");
        assert!(
            load(&folder.0)
                .unwrap_err()
                .starts_with("Ingen WAV-filer i ")
        );
    }

    #[test]
    fn the_manifest_names_each_microphone() {
        let folder = Folder::new("manifest");
        for name in ["s01-usb", "s01-mac", "s01-far", "extra"] {
            folder
                .write(&format!("{name}.wav"), "")
                .write(&format!("{name}.txt"), "Hei.");
        }
        folder.write(
            MANIFEST,
            "[microphones]\n\"*-usb\" = \"usb\"\n\"*-mac\" = \"built-in\"\n\"s01-far\" = \"distance\"\n",
        );
        let clips = load(&folder.0).unwrap();
        let microphones: Vec<(&str, Option<&str>)> = clips
            .iter()
            .map(|c| (c.name.as_str(), c.microphone.as_deref()))
            .collect();
        assert_eq!(
            microphones,
            [
                ("extra", None),
                ("s01-far", Some("distance")),
                ("s01-mac", Some("built-in")),
                ("s01-usb", Some("usb")),
            ]
        );
    }

    #[test]
    fn a_take_matched_twice_is_an_error() {
        let mut clips = [clip("s01-usb", None)];
        let error = assign_microphones(
            &mut clips,
            "[microphones]\n\"s01-*\" = \"a\"\n\"*-usb\" = \"b\"\n",
        )
        .unwrap_err();
        assert_eq!(
            error,
            "Opptaket s01-usb passer med både «*-usb» og «s01-*»."
        );
    }

    #[test]
    fn a_line_that_matches_no_take_is_an_error() {
        let mut clips = [clip("s01-usb", None)];
        let error = assign_microphones(
            &mut clips,
            "[microphones]\n\"*-usb\" = \"usb\"\n\"s02-mac\" = \"built-in\"\n",
        )
        .unwrap_err();
        assert_eq!(error, "Ingen opptak passer med «s02-mac».");
    }

    #[test]
    fn an_unknown_section_is_an_error() {
        let mut clips = [clip("s01-usb", None)];
        assert!(assign_microphones(&mut clips, "[microphone]\n\"*\" = \"usb\"\n").is_err());
        assert!(assign_microphones(&mut clips, "").is_ok());
    }

    #[test]
    fn glob_patterns() {
        assert!(glob_matches("s01-usb", "s01-usb"));
        assert!(!glob_matches("s01-usb", "s01-usb2"));
        assert!(glob_matches("*-usb", "s01-usb"));
        assert!(!glob_matches("*-usb", "s01-mac"));
        assert!(glob_matches("s01-*", "s01-mac"));
        assert!(glob_matches("s*-*b", "s01-usb"));
        assert!(glob_matches("*", "anything"));
        assert!(glob_matches("a*a", "aa"));
        assert!(!glob_matches("a*a", "a"));
        assert!(glob_matches("*usb*", "s01-usb-2"));
    }
}
