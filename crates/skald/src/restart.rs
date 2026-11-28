//! Skald starting itself again (D-60). macOS gives keystrokes only to a process that had Accessibility
//! when it started: a process granted it later can create its event tap, yet the tap hears no key. So a
//! Skald whose first run granted Accessibility starts again once, before it listens for the hold key.
//!
//! Started from a terminal, Skald replaces itself in place, so it keeps the terminal, its arguments and
//! the program macOS asks permissions on behalf of. Skald.app is opened again by macOS once this process
//! has ended.

use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};

use crate::cli;

/// Whether this Skald must start again before it listens for the key: it started without Accessibility,
/// and has it now.
pub(crate) fn needed(accessibility_at_start: bool, accessibility_now: bool) -> bool {
    !accessibility_at_start && accessibility_now
}

/// Starts Skald again with the arguments it was given; `test` opens the new one on the first run's test
/// and runs it. From a terminal this does not return unless it failed. For Skald.app it returns once
/// macOS has been told to open the app again after this process ends, and the caller then quits.
#[cfg(target_os = "macos")]
pub(crate) fn restart(test: bool) -> io::Result<()> {
    use std::io::Write;
    use std::os::unix::process::CommandExt;
    use std::process::Command;

    let exe = std::env::current_exe()?;
    let args = arguments(std::env::args_os().skip(1), test);
    // A line printed before this must reach the terminal before the process is replaced.
    let _ = io::stdout().flush();
    let _ = io::stderr().flush();
    match app_bundle(&exe) {
        None => Err(Command::new(exe).args(args).exec()),
        Some(bundle) => {
            // Waits for this process to end, so the new one never meets it, then has macOS open the app.
            let script = r#"while kill -0 "$0" 2>/dev/null; do sleep 0.1; done
app="$1"; shift
if [ $# -gt 0 ]; then exec open "$app" --args "$@"; else exec open "$app"; fi"#;
            Command::new("/bin/sh")
                .arg("-c")
                .arg(script)
                .arg(std::process::id().to_string())
                .arg(bundle)
                .args(args)
                .process_group(0)
                .spawn()
                .map(drop)
        }
    }
}

/// Only macOS gives keystrokes by the permissions a process started with.
#[cfg(not(target_os = "macos"))]
pub(crate) fn restart(_test: bool) -> io::Result<()> {
    Err(io::ErrorKind::Unsupported.into())
}

/// The arguments for the new Skald: the ones this Skald was given, without what macOS added at launch and
/// without the test's option, which is added when `test` asks for it.
fn arguments(given: impl IntoIterator<Item = OsString>, test: bool) -> Vec<OsString> {
    let mut args: Vec<OsString> = given
        .into_iter()
        .filter(|arg| !cli::from_launch_services(arg) && arg != cli::FIRST_RUN_TEST)
        .collect();
    if test {
        args.push(cli::FIRST_RUN_TEST.into());
    }
    args
}

/// The app bundle the executable runs from, as `…/Skald.app/Contents/MacOS/skald`; none for a binary on
/// its own, as one built with cargo.
fn app_bundle(exe: &Path) -> Option<PathBuf> {
    let macos = exe.parent()?;
    let contents = macos.parent()?;
    let bundle = contents.parent()?;
    let is_app = bundle
        .extension()
        .is_some_and(|extension| extension == "app");
    (macos.file_name()? == "MacOS" && contents.file_name()? == "Contents" && is_app)
        .then(|| bundle.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skald_starts_again_only_when_accessibility_came_while_it_ran() {
        assert!(needed(false, true));
        assert!(!needed(true, true), "granted from the start");
        assert!(!needed(false, false), "still missing: the loop says so");
        assert!(!needed(true, false), "revoked: the loop says so");
    }

    fn args(given: &[&str], test: bool) -> Vec<OsString> {
        arguments(given.iter().map(OsString::from), test)
    }

    #[test]
    fn the_new_skald_gets_the_given_arguments_and_the_test_when_asked() {
        let given = ["-v", "--dir", "/tmp/skald test"];
        assert_eq!(
            args(&given, false),
            ["-v", "--dir", "/tmp/skald test"].map(OsString::from)
        );
        assert_eq!(
            args(&given, true),
            ["-v", "--dir", "/tmp/skald test", "--first-run-test"].map(OsString::from)
        );
        assert_eq!(
            args(&["-psn_0_42", "--first-run-test", "-v"], true),
            ["-v", "--first-run-test"].map(OsString::from),
            "what macOS added is left out, and the test is asked for once"
        );
        assert_eq!(args(&["--first-run-test"], false), Vec::<OsString>::new());
    }

    #[test]
    fn skald_app_is_told_from_a_binary_on_its_own() {
        assert_eq!(
            app_bundle(Path::new("/Applications/Skald.app/Contents/MacOS/skald")),
            Some(PathBuf::from("/Applications/Skald.app"))
        );
        assert_eq!(
            app_bundle(Path::new("/Users/t/skald/target/release/skald")),
            None
        );
        assert_eq!(
            app_bundle(Path::new("/Users/t/Skald.app/skald")),
            None,
            "not inside Contents/MacOS"
        );
        assert_eq!(app_bundle(Path::new("/skald")), None);
    }
}
