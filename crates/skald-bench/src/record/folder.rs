//! The folder a test set is recorded into, which is also the record of how far it has come: a sentence is
//! recorded once its take and its reference are both there.

use std::collections::BTreeSet;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use skald_audio::SAMPLE_RATE;

use super::Label;
use crate::set::{self, MANIFEST, Manifest};

pub(super) struct Folder {
    path: PathBuf,
    label: Label,
    /// Digits in a take's number, so that the names sort in the sentences' order.
    width: usize,
}

impl Folder {
    pub(super) fn new(path: PathBuf, label: Label, sentences: usize) -> Self {
        Self {
            path,
            label,
            width: sentences.to_string().len().max(2),
        }
    }

    pub(super) fn path(&self) -> &Path {
        &self.path
    }

    pub(super) fn label(&self) -> Label {
        self.label
    }

    /// The take's name: the label and the sentence's number, as in «usb-07».
    pub(super) fn name(&self, number: usize) -> String {
        format!("{}-{number:0width$}", self.label, width = self.width)
    }

    /// The sentences, numbered from 1 to `count`, whose take and reference are both in the folder.
    pub(super) fn recorded(&self, count: usize) -> BTreeSet<usize> {
        (1..=count)
            .filter(|&number| {
                let name = self.name(number);
                self.path.join(format!("{name}.wav")).is_file()
                    && self.path.join(format!("{name}.txt")).is_file()
            })
            .collect()
    }

    /// Writes a take, 16 kHz mono 16-bit, and its reference, replacing an earlier take of the sentence. The
    /// reference goes first and the take is moved into place last, so a take is never there without its
    /// reference, and one a crash cut short is not counted as recorded.
    pub(super) fn write(
        &self,
        number: usize,
        sentence: &str,
        samples: &[f32],
    ) -> Result<(), String> {
        let name = self.name(number);
        let txt = self.path.join(format!("{name}.txt"));
        std::fs::write(&txt, format!("{sentence}\n"))
            .map_err(|e| format!("{}: {e}", txt.display()))?;
        let partial = self.path.join(format!(".{name}.wav.part"));
        write_wav(&partial, samples).map_err(|e| format!("{}: {e}", partial.display()))?;
        let wav = self.path.join(format!("{name}.wav"));
        std::fs::rename(&partial, &wav).map_err(|e| format!("{}: {e}", wav.display()))
    }

    /// Makes sure the manifest names the label as the microphone of its takes, adding the line when it is
    /// missing, and that the folder still reads as a test set. Called once the folder has a take with the
    /// label, since a manifest line that matches no take is an error.
    pub(super) fn ensure_manifest(&self) -> Result<(), String> {
        let path = self.path.join(MANIFEST);
        let in_manifest = |message: String| format!("{}: {message}", path.display());
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == ErrorKind::NotFound => String::new(),
            Err(e) => return Err(in_manifest(e.to_string())),
        };
        let manifest = Manifest::parse(&text).map_err(in_manifest)?;
        let label = self.label.to_string();
        let pattern = format!("{label}-*");
        let line = format!("\"{pattern}\" = \"{label}\"");
        match manifest.microphones.get(&pattern) {
            Some(named) if *named == label => return set::load(&self.path).map(drop),
            Some(named) => {
                return Err(in_manifest(format!(
                    "«{pattern}» er «{named}», men opptakene er gjort med «{label}»."
                )));
            }
            None => {}
        }

        let mut extended = text.clone();
        if !extended.is_empty() && !extended.ends_with('\n') {
            extended.push('\n');
        }
        if !text.lines().any(|l| l.trim() == "[microphones]") {
            extended.push_str("[microphones]\n");
        }
        extended.push_str(&line);
        extended.push('\n');
        // A table written another way, on one line say, takes no line added below it.
        let added =
            Manifest::parse(&extended).is_ok_and(|m| m.microphones.get(&pattern) == Some(&label));
        if !added {
            return Err(in_manifest(format!(
                "Legg til linjen {line} under [microphones]."
            )));
        }
        let partial = self.path.join(format!(".{MANIFEST}.part"));
        std::fs::write(&partial, extended).map_err(|e| format!("{}: {e}", partial.display()))?;
        std::fs::rename(&partial, &path).map_err(|e| in_manifest(e.to_string()))?;
        set::load(&self.path).map(drop)
    }
}

fn write_wav(path: &Path, samples: &[f32]) -> Result<(), hound::Error> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: SAMPLE_RATE,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec)?;
    for &sample in samples {
        writer.write_sample((sample.clamp(-1.0, 1.0) * f32::from(i16::MAX)).round() as i16)?;
    }
    writer.finalize()
}

#[cfg(test)]
mod tests {
    use super::super::tests::Scratch;
    use super::*;

    #[test]
    fn names_are_padded_to_sort_in_order() {
        let folder = Folder::new(PathBuf::new(), Label::Usb, 60);
        assert_eq!(folder.name(7), "usb-07");
        assert_eq!(folder.name(60), "usb-60");
        let folder = Folder::new(PathBuf::new(), Label::BuiltIn, 120);
        assert_eq!(folder.name(7), "built-in-007");
    }

    #[test]
    fn a_take_is_written_with_its_reference_at_16_khz_mono() {
        let scratch = Scratch::new("write");
        let folder = Folder::new(scratch.path().to_owned(), Label::Distance, 3);
        folder.write(2, "Hei på deg.", &[0.5; 8000]).unwrap();
        let reader = hound::WavReader::open(scratch.path().join("distance-02.wav")).unwrap();
        let spec = reader.spec();
        assert_eq!(
            (spec.channels, spec.sample_rate, spec.bits_per_sample),
            (1, 16_000, 16)
        );
        assert_eq!(reader.duration(), 8000);
        let reference = std::fs::read_to_string(scratch.path().join("distance-02.txt")).unwrap();
        assert_eq!(reference, "Hei på deg.\n");
        assert_eq!(folder.recorded(3), BTreeSet::from([2]));
        assert!(!scratch.path().join(".distance-02.wav.part").exists());
    }

    #[test]
    fn a_take_without_its_reference_or_half_written_is_not_recorded() {
        let scratch = Scratch::new("partial");
        let folder = Folder::new(scratch.path().to_owned(), Label::Usb, 3);
        std::fs::write(scratch.path().join("usb-01.wav"), "").unwrap();
        std::fs::write(scratch.path().join("usb-02.txt"), "Hei.").unwrap();
        std::fs::write(scratch.path().join(".usb-02.wav.part"), "").unwrap();
        assert!(folder.recorded(3).is_empty());
    }

    fn with_take(scratch: &Scratch, label: Label) -> Folder {
        let folder = Folder::new(scratch.path().to_owned(), label, 3);
        folder.write(1, "Hei.", &[0.1; 1600]).unwrap();
        folder
    }

    #[test]
    fn the_manifest_gets_one_line_per_microphone() {
        let scratch = Scratch::new("manifest");
        let usb = with_take(&scratch, Label::Usb);
        usb.ensure_manifest().unwrap();
        usb.ensure_manifest().unwrap();
        let built_in = with_take(&scratch, Label::BuiltIn);
        built_in.ensure_manifest().unwrap();
        let manifest = std::fs::read_to_string(scratch.path().join(MANIFEST)).unwrap();
        assert_eq!(
            manifest,
            "[microphones]\n\"usb-*\" = \"usb\"\n\"built-in-*\" = \"built-in\"\n"
        );
        let clips = set::load(scratch.path()).unwrap();
        let microphones: Vec<(&str, Option<&str>)> = clips
            .iter()
            .map(|c| (c.name.as_str(), c.microphone.as_deref()))
            .collect();
        assert_eq!(
            microphones,
            [("built-in-01", Some("built-in")), ("usb-01", Some("usb"))]
        );
    }

    #[test]
    fn a_manifest_written_by_hand_is_added_to() {
        let scratch = Scratch::new("by-hand");
        std::fs::write(scratch.path().join("old-far.wav"), "").unwrap();
        std::fs::write(scratch.path().join("old-far.txt"), "Hei.").unwrap();
        std::fs::write(
            scratch.path().join(MANIFEST),
            "# Older takes\n[microphones]\n\"*-far\" = \"distance\"",
        )
        .unwrap();
        with_take(&scratch, Label::Usb).ensure_manifest().unwrap();
        let manifest = std::fs::read_to_string(scratch.path().join(MANIFEST)).unwrap();
        assert_eq!(
            manifest,
            "# Older takes\n[microphones]\n\"*-far\" = \"distance\"\n\"usb-*\" = \"usb\"\n"
        );
    }

    #[test]
    fn a_manifest_that_cannot_take_the_line_is_left_alone() {
        let scratch = Scratch::new("inline");
        let inline = "microphones = { \"*-far\" = \"distance\" }\n";
        std::fs::write(scratch.path().join(MANIFEST), inline).unwrap();
        let error = with_take(&scratch, Label::Usb)
            .ensure_manifest()
            .unwrap_err();
        assert!(
            error.ends_with("Legg til linjen \"usb-*\" = \"usb\" under [microphones]."),
            "{error}"
        );
        let manifest = std::fs::read_to_string(scratch.path().join(MANIFEST)).unwrap();
        assert_eq!(manifest, inline);
    }

    #[test]
    fn a_manifest_naming_another_microphone_is_an_error() {
        let scratch = Scratch::new("conflict");
        std::fs::write(
            scratch.path().join(MANIFEST),
            "[microphones]\n\"usb-*\" = \"headset\"\n",
        )
        .unwrap();
        let error = with_take(&scratch, Label::Usb)
            .ensure_manifest()
            .unwrap_err();
        assert!(
            error.ends_with("«usb-*» er «headset», men opptakene er gjort med «usb»."),
            "{error}"
        );
    }

    #[test]
    fn a_manifest_the_bench_would_refuse_is_reported() {
        let scratch = Scratch::new("refused");
        std::fs::write(
            scratch.path().join(MANIFEST),
            "[microphones]\n\"*\" = \"any\"\n",
        )
        .unwrap();
        let error = with_take(&scratch, Label::Usb)
            .ensure_manifest()
            .unwrap_err();
        assert!(error.contains("passer med både"), "{error}");
    }
}
