//! Where the speech model and Silero VAD are found: given on the command line, or in the models folder,
//! the speech model in a folder named after its id. A missing file stops Skald at start, with where it
//! goes and where to get it.

use std::path::PathBuf;

use skald_core::paths::Paths;

use crate::say::Exit;

/// NbAiLab's own file name for NB-Whisper in whisper.cpp's format, which the engine loads as it is.
pub(crate) const WHISPER_FILE: &str = "ggml-model.bin";

pub(crate) const VAD_FILE: &str = "silero-vad-v6.2-F32.gguf";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ModelFiles {
    pub(crate) model: PathBuf,
    pub(crate) vad: PathBuf,
}

impl ModelFiles {
    /// The files given, else `<models>/<id>/ggml-model.bin` and `<models>/silero-vad-v6.2-F32.gguf`.
    pub(crate) fn locate(
        paths: &Paths,
        model_id: &str,
        model: Option<PathBuf>,
        vad: Option<PathBuf>,
    ) -> Self {
        let models = paths.models_dir();
        Self {
            model: model.unwrap_or_else(|| models.join(model_id).join(WHISPER_FILE)),
            vad: vad.unwrap_or_else(|| models.join(VAD_FILE)),
        }
    }

    /// Fails before anything else starts when a file is missing, saying where it goes.
    pub(crate) fn check(&self, model_id: &str) -> Result<(), Exit> {
        if !self.model.is_file() {
            return Err(Exit::ModelMissing {
                path: self.model.clone(),
                id: model_id.to_owned(),
            });
        }
        if !self.vad.is_file() {
            return Err(Exit::VadMissing {
                path: self.vad.clone(),
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use super::*;
    use crate::files::tests::Scratch;

    #[test]
    fn the_files_sit_in_the_models_folder_unless_given() {
        let paths = Paths::in_one_folder(Path::new("/skald"));
        let files = ModelFiles::locate(&paths, "nb-whisper-large", None, None);
        assert_eq!(
            files.model,
            Path::new("/skald/models/nb-whisper-large/ggml-model.bin")
        );
        assert_eq!(
            files.vad,
            Path::new("/skald/models/silero-vad-v6.2-F32.gguf")
        );

        let given = ModelFiles::locate(
            &paths,
            "nb-whisper-large",
            Some("/m.bin".into()),
            Some("/v.gguf".into()),
        );
        assert_eq!(given.model, Path::new("/m.bin"));
        assert_eq!(given.vad, Path::new("/v.gguf"));
    }

    #[test]
    fn a_missing_file_is_named_the_model_first() {
        let scratch = Scratch::new("models");
        let paths = Paths::in_one_folder(&scratch.0);
        let files = ModelFiles::locate(&paths, "nb-whisper-large", None, None);
        assert!(matches!(
            files.check("nb-whisper-large"),
            Err(Exit::ModelMissing { ref path, ref id }) if *path == files.model && id == "nb-whisper-large"
        ));

        fs::create_dir_all(files.model.parent().unwrap()).unwrap();
        fs::write(&files.model, b"model").unwrap();
        assert!(matches!(
            files.check("nb-whisper-large"),
            Err(Exit::VadMissing { ref path }) if *path == files.vad
        ));

        fs::write(&files.vad, b"vad").unwrap();
        assert!(files.check("nb-whisper-large").is_ok());
    }
}
