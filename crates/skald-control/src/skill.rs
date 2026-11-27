//! The Claude skill Skald ships, which teaches an agent to fix the words Skald writes wrong with these
//! commands (D-12). Its files are held in the binary, so the skill a user installs is the one written for
//! the commands beside it.
//!
//! The install and uninstall follow Ferric's `ferric skill` (MIT, by Skald's author): Skald's files are
//! written in place and removed one by one, so a folder that holds anything else keeps it.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::protocol::{Error, ErrorCode};

/// The skill's name, which is also its folder's.
pub(crate) const NAME: &str = "skald";

pub(crate) struct SkillFile {
    pub(crate) name: &'static str,
    pub(crate) text: &'static str,
}

pub(crate) const FILES: [SkillFile; 2] = [
    SkillFile {
        name: "SKILL.md",
        text: include_str!("../../../packaging/skills/skald/SKILL.md"),
    },
    SkillFile {
        name: "commands.md",
        text: include_str!("../../../packaging/skills/skald/commands.md"),
    },
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct Report {
    #[serde(serialize_with = "lossy")]
    pub(crate) dir: PathBuf,
    pub(crate) files: Vec<FileReport>,
    /// Whether the folder is gone after an uninstall; it stays while it holds files of its own.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) dir_removed: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct FileReport {
    pub(crate) name: &'static str,
    pub(crate) status: Status,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Status {
    Written,
    /// Already as this version has it.
    Unchanged,
    Removed,
    Missing,
}

/// A folder's name that is not UTF-8 is still shown, with the bytes it cannot show replaced.
fn lossy<S: serde::Serializer>(path: &Path, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&path.to_string_lossy())
}

/// Where Claude Code looks for the user's own skills: under `CLAUDE_CONFIG_DIR` when it is set, else under
/// `.claude` in the home folder.
pub(crate) fn default_dir(home: Option<&Path>, claude_config: Option<&Path>) -> Option<PathBuf> {
    let config = match claude_config {
        Some(config) => config.to_path_buf(),
        None => home?.join(".claude"),
    };
    Some(config.join("skills").join(NAME))
}

/// The folder named, or Claude Code's skills folder for this process's environment.
pub(crate) fn dir(named: Option<&Path>) -> Result<PathBuf, Error> {
    if let Some(named) = named {
        return Ok(named.to_path_buf());
    }
    let var = |name: &str| {
        std::env::var_os(name)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
    };
    let home = var("HOME").or_else(|| var("USERPROFILE"));
    default_dir(home.as_deref(), var("CLAUDE_CONFIG_DIR").as_deref()).ok_or_else(|| {
        Error::new(
            ErrorCode::Failed,
            "Finner ingen hjemmemappe (HOME), og dermed ingen ~/.claude/skills. Gi mappen med --dir.",
        )
    })
}

/// Writes the skill's files into the folder, replacing an older copy of them.
pub(crate) fn install(dir: &Path) -> Result<Report, Error> {
    refuse_another_skill(dir)?;
    fs::create_dir_all(dir).map_err(|error| cannot_write(dir, &error))?;
    let files = FILES
        .iter()
        .map(|file| {
            let path = dir.join(file.name);
            let status = if read(&path)?.as_deref() == Some(file.text) {
                Status::Unchanged
            } else {
                write(&path, file.text).map_err(|error| cannot_write(&path, &error))?;
                Status::Written
            };
            Ok(FileReport {
                name: file.name,
                status,
            })
        })
        .collect::<Result<_, Error>>()?;
    Ok(Report {
        dir: dir.to_path_buf(),
        files,
        dir_removed: None,
    })
}

/// Removes the skill's files from the folder, and the folder once nothing else is in it.
pub(crate) fn uninstall(dir: &Path) -> Result<Report, Error> {
    refuse_another_skill(dir)?;
    let files = FILES
        .iter()
        .map(|file| {
            let path = dir.join(file.name);
            let status = match fs::remove_file(&path) {
                Ok(()) => Status::Removed,
                Err(error) if error.kind() == io::ErrorKind::NotFound => Status::Missing,
                Err(error) => {
                    return Err(Error::new(
                        ErrorCode::Io,
                        format!("Kan ikke fjerne {}: {error}", path.display()),
                    ));
                }
            };
            Ok(FileReport {
                name: file.name,
                status,
            })
        })
        .collect::<Result<_, Error>>()?;
    // Fails while anything is left in the folder, which then stays.
    let _ = fs::remove_dir(dir);
    Ok(Report {
        dir: dir.to_path_buf(),
        files,
        dir_removed: Some(!dir.exists()),
    })
}

/// A folder named by mistake may hold another skill; its files are never replaced or removed.
fn refuse_another_skill(dir: &Path) -> Result<(), Error> {
    let path = dir.join("SKILL.md");
    let Some(text) = read(&path)? else {
        return Ok(());
    };
    match skill_name(&text) {
        Some(NAME) => Ok(()),
        other => Err(Error::new(
            ErrorCode::Invalid,
            format!(
                "{} er {}, så Skald rører den ikke. Velg en annen mappe med --dir.",
                path.display(),
                match other {
                    Some(name) => format!("skillen «{name}»"),
                    None => "ikke en skill fra Skald".to_owned(),
                }
            ),
        )),
    }
}

/// The `name` in a SKILL.md's front matter.
pub(crate) fn skill_name(text: &str) -> Option<&str> {
    front_matter(text)?
        .find_map(|line| line.strip_prefix("name:"))
        .map(str::trim)
}

/// The lines between the `---` that opens a SKILL.md and the one that closes its front matter.
fn front_matter(text: &str) -> Option<impl Iterator<Item = &str>> {
    let mut lines = text.lines();
    (lines.next()? == "---").then(|| lines.take_while(|line| *line != "---"))
}

fn read(path: &Path) -> Result<Option<String>, Error> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(Error::new(
            ErrorCode::Io,
            format!("Kan ikke lese {}: {error}", path.display()),
        )),
    }
}

/// Writes beside the file and renames it into place, so an agent reading the skill never reads half of it.
fn write(path: &Path, text: &str) -> io::Result<()> {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".{}.tmp", std::process::id()));
    let partial = path.with_file_name(name);
    let written = fs::write(&partial, text).and_then(|()| fs::rename(&partial, path));
    if written.is_err() {
        let _ = fs::remove_file(&partial);
    }
    written
}

fn cannot_write(path: &Path, error: &io::Error) -> Error {
    Error::new(
        ErrorCode::Io,
        format!("Kan ikke skrive til {}: {error}", path.display()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_skill_says_its_name_and_when_to_use_it() {
        let skill = FILES[0].text;
        assert_eq!(skill_name(skill), Some(NAME));
        let description = front_matter(skill)
            .expect("SKILL.md opens with front matter")
            .find_map(|line| line.strip_prefix("description: "))
            .expect("the front matter has a description");
        assert!(
            description.chars().count() <= 1024,
            "a description of {} characters is cut short in a skill listing",
            description.chars().count()
        );
        assert!(!description.contains(['<', '>']), "{description}");
        assert!(
            skill.lines().count() < 500,
            "a skill's body stays short; reference goes in a file beside it"
        );
    }

    #[test]
    fn the_skill_credits_the_national_library_for_its_models() {
        for file in &FILES {
            let text = file.text.split_whitespace().collect::<Vec<_>>().join(" ");
            if text.contains("NB-Whisper") || text.contains("speech models") {
                assert!(
                    text.contains("National Library of Norway (Nasjonalbiblioteket)"),
                    "{} names the models without crediting the National Library",
                    file.name
                );
            }
        }
    }

    #[test]
    fn the_skill_links_only_files_it_ships() {
        for file in &FILES {
            for (index, _) in file.text.match_indices("](") {
                let target = file.text[index + 2..].split(')').next().unwrap();
                assert!(
                    target.starts_with("https://") || FILES.iter().any(|f| f.name == target),
                    "{} links {target}, which the skill does not ship",
                    file.name
                );
            }
        }
    }

    /// The command each `skald …` in the skill's text names, as its words before the first argument.
    fn commands_named(text: &str) -> Vec<Vec<&str>> {
        text.match_indices("`skald ")
            .map(|(index, _)| {
                let span = text[index + 1..].split('`').next().unwrap_or_default();
                span.split_whitespace()
                    .skip(1)
                    .take_while(|word| word.chars().all(|c| c.is_ascii_lowercase()))
                    .take(2)
                    .collect()
            })
            .collect()
    }

    #[test]
    fn the_skill_names_only_commands_that_exist() {
        let usages: Vec<Vec<&str>> = crate::cli::COMMANDS
            .iter()
            .map(|(usage, _)| {
                usage
                    .split_whitespace()
                    .take_while(|word| word.chars().all(|c| c.is_ascii_lowercase()))
                    .collect()
            })
            .collect();
        let mut named = 0;
        for file in &FILES {
            for words in commands_named(file.text) {
                named += 1;
                assert!(
                    usages.iter().any(|usage| {
                        usage.first() == words.first()
                            && (words.len() < 2 || usage.len() < 2 || usage[..2] == words[..2])
                    }),
                    "{} names «skald {}», which is no command",
                    file.name,
                    words.join(" ")
                );
            }
        }
        assert!(named > 20, "the skill names its commands: {named}");
    }

    #[test]
    fn the_skills_settings_are_those_config_takes() {
        let commands = FILES[1].text;
        for key in crate::commands::SettingKey::ALL {
            let row = format!("| `{}` | `{}` |", key.name(), key.accepted().join("`, `"));
            assert!(commands.contains(&row), "commands.md lacks the row {row}");
        }
        let rows = commands
            .lines()
            .filter(|line| line.starts_with("| `") && line.ends_with("` |"))
            .count();
        assert_eq!(rows, crate::commands::SettingKey::ALL.len());
    }

    #[test]
    fn claude_codes_folder_is_found_from_the_environment() {
        let home = Path::new("/home/a");
        assert_eq!(
            default_dir(Some(home), None),
            Some(PathBuf::from("/home/a/.claude/skills/skald"))
        );
        assert_eq!(
            default_dir(Some(home), Some(Path::new("/srv/claude"))),
            Some(PathBuf::from("/srv/claude/skills/skald"))
        );
        assert_eq!(
            default_dir(None, Some(Path::new("/srv/claude"))),
            Some(PathBuf::from("/srv/claude/skills/skald"))
        );
        assert_eq!(default_dir(None, None), None);
    }

    #[test]
    fn a_skill_name_is_read_from_the_front_matter_only() {
        assert_eq!(skill_name("---\nname: skald\n---\n# x\n"), Some("skald"));
        assert_eq!(
            skill_name("---\ndescription: y\nname:  annen \n---\n"),
            Some("annen")
        );
        assert_eq!(skill_name("# Skald\nname: skald\n"), None);
        assert_eq!(skill_name("---\ndescription: y\n---\nname: skald\n"), None);
        assert_eq!(skill_name("---\r\nname: skald\r\n---\r\n"), Some("skald"));
    }
}
