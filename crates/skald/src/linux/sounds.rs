//! The start and end sounds on Linux: the freedesktop sound theme's own files, played with PipeWire's
//! pw-play, or with paplay where PulseAudio's tools are there instead, each in a process of its own as on
//! macOS. Without the theme's files or either player the sounds stay silent.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;

use crate::sounds::{Chime, Sound};

const THEME: &str = "/usr/share/sounds/freedesktop/stereo";

/// The players tried, in order; one that is not installed is passed over.
const PLAYERS: [&str; 2] = ["pw-play", "paplay"];

/// The theme's file for each sound: a short tick as a take arms, as macOS's «Tink», and the chime of
/// something done as its text is inserted, as «Pop».
fn file(sound: Sound) -> PathBuf {
    Path::new(THEME).join(match sound {
        Sound::Armed => "audio-volume-change.oga",
        Sound::Inserted => "complete.oga",
    })
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ThemeSounds;

impl Chime for ThemeSounds {
    fn play(&self, sound: Sound) {
        let file = file(sound);
        if !file.is_file() {
            return;
        }
        for player in PLAYERS {
            let spawned = Command::new(player)
                .arg(&file)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn();
            if let Ok(mut playing) = spawned {
                // Waited for on a thread of its own, so the finished process does not linger.
                let _ = thread::Builder::new()
                    .name("skald-sound".into())
                    .spawn(move || playing.wait());
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_sound_is_a_file_of_the_freedesktop_theme() {
        assert_eq!(
            file(Sound::Armed),
            Path::new("/usr/share/sounds/freedesktop/stereo/audio-volume-change.oga")
        );
        assert_eq!(
            file(Sound::Inserted),
            Path::new("/usr/share/sounds/freedesktop/stereo/complete.oga")
        );
    }
}
