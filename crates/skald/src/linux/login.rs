//! Start at login, the setting, on Linux: an XDG autostart entry, `skald.desktop` in the user's autostart
//! folder, which desktops that follow the autostart specification start at login, as Hyprland does when
//! uwsm runs it. A compositor that ignores autostart starts Skald from its own configuration instead, as
//! with Hyprland's `exec-once` (packaging/linux). The entry names the binary that wrote it, so one left by a
//! binary that has since moved is written again at the next start.

use std::fs;
use std::path::{Path, PathBuf};

use crate::say::Line;

const ENTRY: &str = "skald.desktop";

/// A line only Skald's own entry holds, so an entry the user wrote under the same name is left alone.
const MARK: &str = "X-Skald-StartAtLogin=true";

/// Brings the autostart entry to the setting; says what changed, or why it could not be.
pub(crate) fn apply(wanted: bool) -> Option<Line> {
    let Some(folder) = autostart_folder() else {
        return wanted.then(|| Line::LoginItemFailed("fant ikke hjemmemappen".into()));
    };
    let executable = match std::env::current_exe() {
        Ok(executable) => executable,
        Err(error) => return wanted.then(|| Line::LoginItemFailed(error.to_string())),
    };
    apply_in(&folder, &executable, wanted)
}

/// `$XDG_CONFIG_HOME/autostart`, or `~/.config/autostart`.
fn autostart_folder() -> Option<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .filter(|folder| !folder.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .map(|config| config.join("autostart"))
}

fn apply_in(folder: &Path, executable: &Path, wanted: bool) -> Option<Line> {
    let path = folder.join(ENTRY);
    let present = fs::read_to_string(&path).ok();
    if wanted {
        let entry = entry(executable);
        if present.as_deref() == Some(entry.as_str()) {
            return None;
        }
        let written = fs::create_dir_all(folder).and_then(|()| fs::write(&path, entry));
        return Some(match written {
            Ok(()) => Line::LoginItemOn,
            Err(error) => Line::LoginItemFailed(error.to_string()),
        });
    }
    if !present.is_some_and(|entry| entry.lines().any(|line| line == MARK)) {
        return None;
    }
    Some(match fs::remove_file(&path) {
        Ok(()) => Line::LoginItemOff,
        Err(error) => Line::LoginItemFailed(error.to_string()),
    })
}

fn entry(executable: &Path) -> String {
    format!(
        "[Desktop Entry]\nType=Application\nName=Skald\nComment=Norsk tale til tekst\nExec={}\n\
         Terminal=false\nNoDisplay=true\n{MARK}\n",
        exec_argument(&executable.to_string_lossy())
    )
}

/// The binary's path as the Exec key reads it (Desktop Entry Specification, «The Exec key»). A path with a
/// reserved character goes in double quotes, with `"`, `` ` ``, `$` and `\` escaped inside them; the
/// file's own escaping then doubles each backslash, and `%` is written `%%` either way.
fn exec_argument(path: &str) -> String {
    const RESERVED: &[char] = &[
        ' ', '\t', '\n', '"', '\'', '\\', '>', '<', '~', '|', '&', ';', '$', '*', '?', '#', '(',
        ')', '`',
    ];
    let path = path.replace('%', "%%");
    if !path.contains(RESERVED) {
        return path;
    }
    let mut quoted = String::from("\"");
    for character in path.chars() {
        match character {
            '"' | '`' | '$' => {
                quoted.push_str("\\\\");
                quoted.push(character);
            }
            '\\' => quoted.push_str("\\\\\\\\"),
            _ => quoted.push(character),
        }
    }
    quoted.push('"');
    quoted
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::files::tests::Scratch;

    #[test]
    fn the_entry_is_written_kept_and_removed_with_the_setting() {
        let scratch = Scratch::new("login-autostart");
        let folder = scratch.0.join("autostart");
        let skald = Path::new("/usr/bin/skald");

        assert_eq!(apply_in(&folder, skald, true), Some(Line::LoginItemOn));
        let entry = fs::read_to_string(folder.join(ENTRY)).unwrap();
        assert!(entry.starts_with("[Desktop Entry]\n"), "{entry}");
        assert!(entry.contains("\nExec=/usr/bin/skald\n"), "{entry}");
        assert_eq!(
            apply_in(&folder, skald, true),
            None,
            "an entry in place stays"
        );

        let moved = Path::new("/home/a/.local/bin/skald");
        assert_eq!(apply_in(&folder, moved, true), Some(Line::LoginItemOn));
        assert!(
            fs::read_to_string(folder.join(ENTRY))
                .unwrap()
                .contains(&moved.display().to_string())
        );

        assert_eq!(apply_in(&folder, skald, false), Some(Line::LoginItemOff));
        assert!(!folder.join(ENTRY).exists());
        assert_eq!(apply_in(&folder, skald, false), None);
    }

    #[test]
    fn an_entry_the_user_wrote_is_left_alone() {
        let scratch = Scratch::new("login-theirs");
        let folder = scratch.0.join("autostart");
        fs::create_dir_all(&folder).unwrap();
        fs::write(
            folder.join(ENTRY),
            "[Desktop Entry]\nExec=skald --verbose\n",
        )
        .unwrap();
        assert_eq!(apply_in(&folder, Path::new("/usr/bin/skald"), false), None);
        assert!(folder.join(ENTRY).exists());
    }

    #[test]
    fn a_path_with_reserved_characters_is_quoted_as_the_specification_says() {
        assert_eq!(exec_argument("/usr/bin/skald"), "/usr/bin/skald");
        assert_eq!(exec_argument("/opt/50%/skald"), "/opt/50%%/skald");
        assert_eq!(
            exec_argument("/home/a/My Apps/skald"),
            "\"/home/a/My Apps/skald\""
        );
        assert_eq!(exec_argument("/a/$b/skald"), "\"/a/\\\\$b/skald\"");
        assert_eq!(exec_argument("/a\\b/skald"), "\"/a\\\\\\\\b/skald\"");
    }
}
