//! Silero VAD, built into the binary (MIT, Silero Team; `assets/models/README.md` names its source and
//! SHA-256). The engine loads it from a file, so it is written into the models folder whenever the file
//! there is missing or is not this one; a first run downloads only the speech model.

use std::fs;
use std::io::{self, ErrorKind};
use std::path::Path;

pub(crate) const SILERO_VAD: &[u8] =
    include_bytes!("../../../assets/models/silero-vad-v6.2-F32.gguf");

/// Puts the built-in Silero VAD at `path` unless the file there already holds it. Returns whether it
/// wrote the file. The file appears whole or not at all: it is written beside its place and then moved in.
pub(crate) fn put_in_place(path: &Path) -> io::Result<bool> {
    match fs::read(path) {
        Ok(bytes) if bytes == SILERO_VAD => return Ok(false),
        Ok(_) => {}
        Err(error) if error.kind() == ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let beside = path.with_extension("gguf.new");
    fs::write(&beside, SILERO_VAD)?;
    fs::rename(&beside, path)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use skald_models::{Cancel, ModelFile, VAD_FILE};

    use super::*;
    use crate::files::tests::Scratch;

    /// The file `assets/models/README.md` names.
    fn pinned() -> ModelFile {
        ModelFile {
            name: Cow::Borrowed(VAD_FILE),
            url: Cow::Borrowed(
                "https://huggingface.co/handy-computer/silero-vad-v6.2-gguf/resolve/a9b2d61c20ae4ddf3644edb34969e8185a485448/silero-vad-v6.2-F32.gguf",
            ),
            sha256: Cow::Borrowed(
                "0d871a800c420fdd5d1b17a3aea4296d1c39608f0ff4a9295eb76457eb1c88e6",
            ),
            size: 1_241_056,
        }
    }

    #[test]
    fn the_built_in_file_is_the_pinned_one() {
        let scratch = Scratch::new("vad-pinned");
        let path = skald_models::vad_path(&scratch.0);
        assert!(put_in_place(&path).unwrap());
        assert!(skald_models::verify(&pinned(), &scratch.0, &Cancel::new()).unwrap());
    }

    #[test]
    fn a_missing_or_other_file_is_replaced_and_the_right_one_left_alone() {
        let scratch = Scratch::new("vad-place");
        let path = scratch.0.join("models").join(VAD_FILE);
        assert!(put_in_place(&path).unwrap(), "missing, with its folder");
        assert_eq!(fs::read(&path).unwrap(), SILERO_VAD);
        assert!(!path.with_extension("gguf.new").exists());

        assert!(!put_in_place(&path).unwrap(), "already in place");

        fs::write(&path, &SILERO_VAD[..1000]).unwrap();
        assert!(put_in_place(&path).unwrap(), "cut short");
        assert_eq!(fs::read(&path).unwrap(), SILERO_VAD);
    }

    #[test]
    fn a_folder_in_the_way_is_an_error() {
        let scratch = Scratch::new("vad-blocked");
        let path = scratch.0.join(VAD_FILE);
        fs::create_dir_all(&path).unwrap();
        assert!(put_in_place(&path).is_err());
    }
}
