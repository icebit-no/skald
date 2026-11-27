//! What of a model file is in its model's folder.
//!
//! A downloaded file takes its name only after its SHA-256 matched, and the hash it matched is then written
//! beside it (`ggml-model.bin.sha256`). So a file is known to be checked without reading it again, and a pin
//! that moves to a new file is told apart from the file of the old pin.

use std::fs;
use std::path::{Path, PathBuf};

use crate::registry::ModelFile;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Missing,
    /// A download stopped part way; the next one goes on from here.
    Partial {
        received: u64,
    },
    /// A file of the pinned size is there but was never checked against the pinned hash: put there by hand,
    /// or left by an earlier pin. Downloading checks it before fetching anything.
    Unchecked,
    /// A file of another size is there: from an earlier pin, or damaged. Downloading replaces it.
    Other {
        size: u64,
    },
    /// The pinned file, checked when it arrived.
    Ready,
}

pub fn status(file: &ModelFile, dir: &Path) -> Status {
    match fs::metadata(dir.join(&*file.name)) {
        Ok(meta) if meta.len() != file.size => Status::Other { size: meta.len() },
        Ok(_) if is_recorded(file, dir) => Status::Ready,
        Ok(_) => Status::Unchecked,
        Err(_) => match fs::metadata(part_path(file, dir)) {
            Ok(meta) if meta.len() > 0 => Status::Partial {
                received: meta.len().min(file.size),
            },
            _ => Status::Missing,
        },
    }
}

/// Where a download collects the file before it is checked. Its name carries the start of the pinned hash,
/// so a download never goes on from bytes of another file.
pub(crate) fn part_path(file: &ModelFile, dir: &Path) -> PathBuf {
    dir.join(format!("{}.{}.part", file.name, short_hash(file)))
}

pub(crate) fn record_path(file: &ModelFile, dir: &Path) -> PathBuf {
    dir.join(format!("{}.sha256", file.name))
}

pub(crate) fn is_recorded(file: &ModelFile, dir: &Path) -> bool {
    fs::read_to_string(record_path(file, dir)).is_ok_and(|text| text.trim() == file.sha256)
}

/// A download's leftovers for this file name from other pins.
pub(crate) fn stale_parts(file: &ModelFile, dir: &Path) -> Vec<PathBuf> {
    let current = part_path(file, dir);
    let prefix = format!("{}.", file.name);
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| *path != current)
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with(&prefix) && name.ends_with(".part"))
        })
        .collect()
}

fn short_hash(file: &ModelFile) -> &str {
    file.sha256.get(..8).unwrap_or(&file.sha256)
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use super::*;

    fn file() -> ModelFile {
        ModelFile {
            name: Cow::Borrowed("ggml-model.bin"),
            url: Cow::Borrowed("https://example.org/ggml-model.bin"),
            sha256: Cow::Borrowed(
                "0f2f66f22e11a7c7da3c582d8e5c89cb2c0011753ba9c7c9731e320a4ba33e76",
            ),
            size: 4,
        }
    }

    fn folder(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("skald-models-status-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn tells_every_state_apart() {
        let dir = folder("states");
        let file = file();
        assert_eq!(status(&file, &dir), Status::Missing);

        fs::write(part_path(&file, &dir), b"ab").unwrap();
        assert_eq!(status(&file, &dir), Status::Partial { received: 2 });

        fs::write(dir.join("ggml-model.bin"), b"abc").unwrap();
        assert_eq!(status(&file, &dir), Status::Other { size: 3 });

        fs::write(dir.join("ggml-model.bin"), b"abcd").unwrap();
        assert_eq!(status(&file, &dir), Status::Unchecked);

        fs::write(record_path(&file, &dir), "another hash\n").unwrap();
        assert_eq!(status(&file, &dir), Status::Unchecked);

        fs::write(record_path(&file, &dir), format!("{}\n", file.sha256)).unwrap();
        assert_eq!(status(&file, &dir), Status::Ready);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn finds_parts_left_by_other_pins() {
        let dir = folder("stale");
        let file = file();
        fs::write(part_path(&file, &dir), b"current").unwrap();
        fs::write(dir.join("ggml-model.bin.12345678.part"), b"old").unwrap();
        fs::write(dir.join("other-model.bin.12345678.part"), b"not ours").unwrap();
        assert_eq!(
            stale_parts(&file, &dir),
            vec![dir.join("ggml-model.bin.12345678.part")]
        );
        fs::remove_dir_all(&dir).unwrap();
    }
}
