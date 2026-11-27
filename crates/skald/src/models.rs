//! Where the speech model and Silero VAD are found, and whether they are there: the files given on the
//! command line, or those the models folder holds by skald-models' layout (D-26). A model file put in the
//! folder by hand is checked against its pinned SHA-256 once, as a downloaded one was; a missing or wrong
//! file stops Skald at start, with where it goes and where to get it.

use std::path::{Path, PathBuf};

use skald_core::paths::Paths;
use skald_models::{Cancel, Model, Status};

use crate::say::Exit;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ModelFiles {
    pub(crate) model: PathBuf,
    pub(crate) vad: PathBuf,
    /// The registry's entry when the model file is the one it pins, so it is checked against it; `None` for
    /// a file given on the command line.
    pinned: Option<&'static Model>,
}

impl ModelFiles {
    /// The files given, else the configured model's file and Silero VAD in the models folder.
    pub(crate) fn locate(
        paths: &Paths,
        model_id: &str,
        model: Option<PathBuf>,
        vad: Option<PathBuf>,
    ) -> Result<Self, Exit> {
        let models_dir = paths.models_dir();
        let vad = vad.unwrap_or_else(|| skald_models::vad_path(&models_dir));
        if let Some(model) = model {
            return Ok(Self {
                model,
                vad,
                pinned: None,
            });
        }
        let pinned =
            skald_models::find(model_id).ok_or_else(|| Exit::UnknownModel(model_id.to_owned()))?;
        let model = pinned
            .path(&models_dir)
            .ok_or(Exit::ModelNotOffered(pinned.name))?;
        Ok(Self {
            model,
            vad,
            pinned: Some(pinned),
        })
    }

    /// Fails before anything else starts when a file is missing or is not the pinned one. A pinned file that
    /// was never checked is read once to check it, which takes a few seconds; `checking` is called first.
    pub(crate) fn check(&self, checking: impl FnOnce()) -> Result<(), Exit> {
        match self.pinned {
            Some(pinned) => check_pinned(pinned, &self.model, checking)?,
            None if !self.model.is_file() => {
                return Err(Exit::ModelMissing {
                    path: self.model.clone(),
                    pinned: None,
                });
            }
            None => {}
        }
        if !self.vad.is_file() {
            return Err(Exit::VadMissing {
                path: self.vad.clone(),
            });
        }
        Ok(())
    }
}

fn check_pinned(pinned: &'static Model, path: &Path, checking: impl FnOnce()) -> Result<(), Exit> {
    let (Some(file), Some(dir)) = (pinned.file(), path.parent()) else {
        return Err(Exit::ModelNotOffered(pinned.name));
    };
    let missing = || Exit::ModelMissing {
        path: path.to_owned(),
        pinned: Some(pinned),
    };
    let wrong = || Exit::ModelWrongFile {
        path: path.to_owned(),
        pinned,
    };
    match skald_models::status(file, dir) {
        Status::Ready => Ok(()),
        Status::Missing | Status::Partial { .. } => Err(missing()),
        Status::Other { .. } => Err(wrong()),
        Status::Unchecked => {
            checking();
            match skald_models::verify(file, dir, &Cancel::new()) {
                Ok(true) => Ok(()),
                Ok(false) => Err(wrong()),
                Err(error) => Err(Exit::ModelUnreadable(error.to_string())),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use skald_core::config::DEFAULT_MODEL;

    use super::*;
    use crate::files::tests::Scratch;

    fn quiet() {}

    #[test]
    fn the_files_sit_where_skald_models_puts_them_unless_given() {
        let paths = Paths::in_one_folder(Path::new("/skald"));
        let files = ModelFiles::locate(&paths, DEFAULT_MODEL, None, None).unwrap();
        assert_eq!(
            files.model,
            Path::new("/skald/models/nb-whisper-large/ggml-model.bin")
        );
        assert_eq!(
            files.vad,
            Path::new("/skald/models/silero-vad-v6.2-F32.gguf")
        );
        assert_eq!(files.pinned.map(|m| m.id), Some(DEFAULT_MODEL));

        let given = ModelFiles::locate(
            &paths,
            "nb-asr-bokfink",
            Some("/m.bin".into()),
            Some("/v.gguf".into()),
        )
        .unwrap();
        assert_eq!(given.model, Path::new("/m.bin"));
        assert_eq!(given.vad, Path::new("/v.gguf"));
        assert_eq!(given.pinned, None);
    }

    #[test]
    fn a_model_without_a_file_to_fetch_or_unknown_needs_one_given() {
        let paths = Paths::in_one_folder(Path::new("/skald"));
        assert_eq!(
            ModelFiles::locate(&paths, "nb-asr-bokfink", None, None).unwrap_err(),
            Exit::ModelNotOffered("NB-ASR Bokfink")
        );
        assert_eq!(
            ModelFiles::locate(&paths, "nb-whisper-tiny", None, None).unwrap_err(),
            Exit::UnknownModel("nb-whisper-tiny".into())
        );
    }

    #[test]
    fn a_missing_or_wrong_file_is_named_the_model_first() {
        let scratch = Scratch::new("models");
        let paths = Paths::in_one_folder(&scratch.0);
        let files = ModelFiles::locate(&paths, DEFAULT_MODEL, None, None).unwrap();
        assert!(matches!(
            files.check(quiet),
            Err(Exit::ModelMissing { ref path, pinned: Some(_) }) if *path == files.model
        ));

        // Not the pinned size: another file, never read.
        fs::create_dir_all(files.model.parent().unwrap()).unwrap();
        fs::write(&files.model, b"not a model").unwrap();
        assert!(matches!(
            files.check(quiet),
            Err(Exit::ModelWrongFile { ref path, .. }) if *path == files.model
        ));
    }

    #[test]
    fn a_file_given_on_the_command_line_is_taken_as_it_is() {
        let scratch = Scratch::new("models-given");
        let model = scratch.0.join("model.bin");
        let vad = scratch.0.join("vad.gguf");
        let paths = Paths::in_one_folder(&scratch.0);
        let files = ModelFiles::locate(
            &paths,
            DEFAULT_MODEL,
            Some(model.clone()),
            Some(vad.clone()),
        )
        .unwrap();
        assert!(matches!(
            files.check(quiet),
            Err(Exit::ModelMissing { ref path, pinned: None }) if *path == model
        ));

        fs::write(&model, b"any model").unwrap();
        assert!(matches!(
            files.check(quiet),
            Err(Exit::VadMissing { ref path }) if *path == vad
        ));

        fs::write(&vad, b"vad").unwrap();
        assert_eq!(files.check(quiet), Ok(()));
    }
}
