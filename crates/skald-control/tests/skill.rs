//! `skald skill install` and `uninstall`, always into a folder of the test's making, never the user's own
//! `~/.claude`.

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use common::{Folder, skald};
use serde_json::json;

/// The skill's files as the repository holds them, which the binary must install unchanged.
const SHIPPED: [&str; 2] = ["SKILL.md", "commands.md"];

fn shipped(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../packaging/skills/skald")
        .join(name);
    fs::read_to_string(path).unwrap()
}

/// A folder for the test, and the skill's folder inside it, not yet made.
fn skill_folder() -> (Folder, PathBuf) {
    let folder = Folder::new();
    let dir = folder.path.join(".claude/skills/skald");
    (folder, dir)
}

/// A file in the folder as the command line writes its path.
fn shown(dir: &Path, name: &str) -> String {
    dir.join(name).display().to_string()
}

fn names_in(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn install_writes_the_skill_and_replaces_an_older_copy() {
    let (folder, dir) = skill_folder();
    let paths = folder.paths();
    let dir_arg = dir.to_string_lossy().into_owned();

    let ran = skald(&paths, &["skill", "install", "--dir", &dir_arg, "--json"]);
    assert_eq!(ran.code, 0, "{ran:?}");
    assert_eq!(
        ran.json(),
        json!({"dir": dir_arg, "files": [
            {"name": "SKILL.md", "status": "written"},
            {"name": "commands.md", "status": "written"},
        ]})
    );
    for name in SHIPPED {
        assert_eq!(
            fs::read_to_string(dir.join(name)).unwrap(),
            shipped(name),
            "{name}"
        );
    }

    let again = skald(&paths, &["skill", "install", "--dir", &dir_arg, "--json"]).json();
    assert_eq!(again["files"][0]["status"], "unchanged");
    assert_eq!(again["files"][1]["status"], "unchanged");

    fs::write(
        dir.join("SKILL.md"),
        "---\nname: skald\n---\nAn older skill.\n",
    )
    .unwrap();
    let upgraded = skald(&paths, &["skill", "install", "--dir", &dir_arg, "--json"]).json();
    assert_eq!(upgraded["files"][0]["status"], "written");
    assert_eq!(upgraded["files"][1]["status"], "unchanged");
    assert_eq!(
        fs::read_to_string(dir.join("SKILL.md")).unwrap(),
        shipped("SKILL.md")
    );
    assert_eq!(names_in(&dir), SHIPPED, "no partial file is left behind");
}

#[test]
fn a_person_reads_where_the_skill_went_and_how_to_add_the_tools() {
    let (folder, dir) = skill_folder();
    let paths = folder.paths();
    let dir_arg = dir.to_string_lossy().into_owned();
    let ran = skald(&paths, &["skill", "install", "--dir", &dir_arg]);
    assert_eq!(ran.code, 0, "{ran:?}");
    assert_eq!(
        ran.out,
        format!(
            "Skrev {}\nSkrev {}\nClaude-skillen «skald» ligger i {dir_arg}.\nEn agent som bruker MCP, \
             får de samme kommandoene som verktøy med:\n  claude mcp add --scope user skald -- skald \
             mcp\n",
            shown(&dir, "SKILL.md"),
            shown(&dir, "commands.md"),
        )
    );
    let ran = skald(&paths, &["skill", "install", "--dir", &dir_arg]);
    assert!(
        ran.out.starts_with(&format!(
            "{} var oppdatert fra før\n",
            shown(&dir, "SKILL.md")
        )),
        "{}",
        ran.out
    );
}

#[test]
fn uninstall_removes_the_skill_and_its_folder() {
    let (folder, dir) = skill_folder();
    let paths = folder.paths();
    let dir_arg = dir.to_string_lossy().into_owned();
    assert_eq!(
        skald(&paths, &["skill", "install", "--dir", &dir_arg]).code,
        0
    );

    let ran = skald(&paths, &["skill", "uninstall", "--dir", &dir_arg, "--json"]);
    assert_eq!(ran.code, 0, "{ran:?}");
    assert_eq!(
        ran.json(),
        json!({"dir": dir_arg, "dir_removed": true, "files": [
            {"name": "SKILL.md", "status": "removed"},
            {"name": "commands.md", "status": "removed"},
        ]})
    );
    assert!(!dir.exists());
    assert!(
        dir.parent().unwrap().exists(),
        "only the skill's own folder goes"
    );

    let ran = skald(&paths, &["skill", "uninstall", "--dir", &dir_arg]);
    assert_eq!(ran.code, 0, "{ran:?}");
    assert_eq!(
        ran.out,
        format!("Fant ingen Claude-skill fra Skald i {dir_arg}. Ingenting ble fjernet.\n")
    );
}

#[test]
fn uninstall_leaves_files_that_are_not_skalds() {
    let (folder, dir) = skill_folder();
    let paths = folder.paths();
    let dir_arg = dir.to_string_lossy().into_owned();
    assert_eq!(
        skald(&paths, &["skill", "install", "--dir", &dir_arg]).code,
        0
    );
    fs::write(dir.join("notater.md"), "mine\n").unwrap();

    let ran = skald(&paths, &["skill", "uninstall", "--dir", &dir_arg]);
    assert_eq!(ran.code, 0, "{ran:?}");
    assert_eq!(
        ran.out,
        format!(
            "Fjernet {}\nFjernet {}\n{dir_arg} har andre filer også, så mappen står igjen.\n",
            shown(&dir, "SKILL.md"),
            shown(&dir, "commands.md"),
        )
    );
    assert_eq!(names_in(&dir), ["notater.md"]);
}

#[test]
fn a_folder_holding_another_skill_is_left_alone() {
    let (folder, dir) = skill_folder();
    let paths = folder.paths();
    let dir_arg = dir.to_string_lossy().into_owned();
    fs::create_dir_all(&dir).unwrap();
    let other = "---\nname: timer\ndescription: Noe annet.\n---\n# Timer\n";
    fs::write(dir.join("SKILL.md"), other).unwrap();
    fs::write(dir.join("commands.md"), "timer sine\n").unwrap();

    for verb in ["install", "uninstall"] {
        let ran = skald(&paths, &["skill", verb, "--dir", &dir_arg, "--json"]);
        assert_eq!(ran.code, 1, "{verb}: {ran:?}");
        assert_eq!(
            ran.json(),
            json!({"error": {"code": "invalid", "message": format!(
                "{} er skillen «timer», så Skald rører den ikke. Velg en annen mappe med --dir.",
                shown(&dir, "SKILL.md"))}}),
            "{verb}"
        );
        assert_eq!(fs::read_to_string(dir.join("SKILL.md")).unwrap(), other);
        assert_eq!(
            fs::read_to_string(dir.join("commands.md")).unwrap(),
            "timer sine\n"
        );
    }
}

#[test]
fn wrong_arguments_say_how_skill_and_mcp_are_used() {
    let folder = Folder::new();
    let paths = folder.paths();
    for (words, message) in [
        (
            &["skill"][..],
            "Bruk: skald skill install [--dir <mappe>]\n      skald skill uninstall [--dir <mappe>]",
        ),
        (
            &["skill", "remove"],
            "«skald skill» har ingen kommando «remove».",
        ),
        (&["skill", "install", "--dir"], "--dir trenger en mappe."),
        (
            &["skill", "install", "x", "--dir", "y"],
            "Bruk: skald skill install [--dir <mappe>]",
        ),
        (
            &["last", "--dir", "x"],
            "--dir hører til «skald skill install» og «skald skill uninstall».",
        ),
        (&["mcp", "--first"], "--first hører til «skald mic use»."),
        (&["mcp", "extra"], "Bruk: skald mcp"),
    ] {
        let ran = skald(&paths, words);
        assert_eq!(ran.code, 2, "{words:?}: {ran:?}");
        assert_eq!(
            ran.err,
            format!("{message}\nSe «skald help».\n"),
            "{words:?}"
        );
    }
}

#[test]
fn help_lists_skill_and_mcp() {
    let folder = Folder::new();
    let ran = skald(&folder.paths(), &["help"]);
    for line in [
        "  skill install [--dir <mappe>]    Installer Claude-skillen som lærer en agent disse kommandoene",
        "  skill uninstall [--dir <mappe>]  Fjern Claude-skillen",
        "  mcp                              De samme kommandoene som MCP-verktøy, over standard inn og ut",
    ] {
        assert!(
            ran.out.lines().any(|l| l == line),
            "{line:?} in:\n{}",
            ran.out
        );
    }
}
