//! The start and end sounds: macOS's «Tink» when a take arms and «Pop» as its text is inserted. Each plays
//! in an `afplay` process of its own, so a sound output that will not start, which can hold a thread for
//! seconds, holds up neither the keys nor the insertion.

use std::process::{Command, Stdio};
use std::thread;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Sound {
    Armed,
    Inserted,
}

impl Sound {
    fn file(self) -> &'static str {
        match self {
            Sound::Armed => "/System/Library/Sounds/Tink.aiff",
            Sound::Inserted => "/System/Library/Sounds/Pop.aiff",
        }
    }
}

/// Plays a sound without waiting for it. Whether sounds are on is the caller's to check.
pub(crate) trait Chime: Send + Sync {
    fn play(&self, sound: Sound);
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Afplay;

impl Chime for Afplay {
    fn play(&self, sound: Sound) {
        let spawned = Command::new("/usr/bin/afplay")
            .arg(sound.file())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
        // A sound that cannot play is no reason to tell the user anything.
        let Ok(mut player) = spawned else {
            return;
        };
        // Waited for on a thread of its own, so the finished process does not linger.
        let _ = thread::Builder::new()
            .name("skald-sound".into())
            .spawn(move || player.wait());
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::path::Path;
    use std::sync::{Arc, Mutex};

    use super::*;

    /// Remembers what it was asked to play.
    #[derive(Debug, Default, Clone)]
    pub(crate) struct Played(pub(crate) Arc<Mutex<Vec<Sound>>>);

    impl Played {
        pub(crate) fn sounds(&self) -> Vec<Sound> {
            self.0.lock().unwrap().clone()
        }
    }

    impl Chime for Played {
        fn play(&self, sound: Sound) {
            self.0.lock().unwrap().push(sound);
        }
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn the_sounds_are_the_systems_own() {
        for sound in [Sound::Armed, Sound::Inserted] {
            assert!(Path::new(sound.file()).is_file(), "{sound:?}");
        }
        assert!(Path::new("/usr/bin/afplay").is_file());
    }
}
