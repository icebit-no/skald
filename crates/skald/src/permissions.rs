//! The microphone and Accessibility as the first-run window needs them: read without a prompt, asked for
//! with the system's prompt, the System Settings pane where a refusal is undone, and whom System Settings
//! lists them under.

use std::ffi::{OsStr, c_void};
use std::os::unix::ffi::OsStrExt;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

use skald_audio::MicrophoneAccess;
use skald_firstrun::{Access, ListedAs, Permission, PermissionChecks};

const MICROPHONE_PANE: &str =
    "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone";

/// The system's own checks, which never prompt.
pub(crate) fn checks() -> PermissionChecks {
    PermissionChecks {
        microphone: Box::new(|| access(skald_audio::microphone_access())),
        accessibility: Box::new(skald_input::accessibility_granted),
    }
}

pub(crate) fn access(microphone: MicrophoneAccess) -> Access {
    match microphone {
        MicrophoneAccess::Granted => Access::Granted,
        MicrophoneAccess::NotAsked => Access::NotAsked,
        MicrophoneAccess::Denied => Access::Denied,
        MicrophoneAccess::Restricted => Access::Restricted,
    }
}

/// Shows the system's prompt for the permission. `answered` is called, on any thread, once the user has
/// answered the microphone's prompt; Accessibility's prompt sends the user to System Settings, and the
/// window reads it again when it comes back to the front.
pub(crate) fn request(permission: Permission, answered: impl Fn() + Send + Sync + 'static) {
    match permission {
        Permission::Microphone => microphone::request(answered),
        Permission::Accessibility => {
            skald_input::request_accessibility();
        }
    }
}

/// Opens the permission's pane in System Settings.
pub(crate) fn open_settings(permission: Permission) {
    match permission {
        Permission::Microphone => {
            let _ = Command::new("open").arg(MICROPHONE_PANE).status();
        }
        Permission::Accessibility => {
            let _ = skald_input::open_accessibility_settings();
        }
    }
}

/// Whom System Settings lists Skald's permissions under. macOS asks on behalf of the process responsible
/// for Skald: Skald itself when it runs as Skald.app, or the terminal it was started from.
pub(crate) fn listed_as() -> ListedAs {
    let me = std::process::id() as libc::pid_t;
    match responsible_for(me) {
        None => ListedAs::Skald,
        Some(pid) if pid == me => ListedAs::Skald,
        Some(pid) => ListedAs::StartedFrom(executable(pid).and_then(|path| shown_name(&path))),
    }
}

/// The process macOS holds responsible for `pid`, which its privacy permissions are asked on behalf of.
/// The lookup is not in Apple's headers, so it is looked up as the program runs, and a system without it
/// answers nothing.
fn responsible_for(pid: libc::pid_t) -> Option<libc::pid_t> {
    type Lookup = unsafe extern "C" fn(libc::pid_t) -> libc::pid_t;
    // SAFETY: a symbol looked up by a valid C string among the libraries already loaded.
    let symbol = unsafe {
        libc::dlsym(
            libc::RTLD_DEFAULT,
            c"responsibility_get_pid_responsible_for_pid".as_ptr(),
        )
    };
    if symbol.is_null() {
        return None;
    }
    // SAFETY: libSystem's function of this name takes a pid and returns one.
    let lookup: Lookup = unsafe { std::mem::transmute::<*mut c_void, Lookup>(symbol) };
    // SAFETY: the function only reads the process table.
    let responsible = unsafe { lookup(pid) };
    (responsible > 0).then_some(responsible)
}

fn executable(pid: libc::pid_t) -> Option<PathBuf> {
    let mut buffer = vec![0u8; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
    // SAFETY: the buffer holds as many bytes as the call is told.
    let length =
        unsafe { libc::proc_pidpath(pid, buffer.as_mut_ptr().cast(), buffer.len() as u32) };
    let length = usize::try_from(length).ok().filter(|length| *length > 0)?;
    Some(PathBuf::from(OsStr::from_bytes(&buffer[..length])))
}

/// The name System Settings shows for a program: the app bundle it runs from, as Finder names it, or
/// the executable's own name for a program outside a bundle.
fn shown_name(executable: &Path) -> Option<String> {
    let bundle = executable
        .components()
        .find_map(|component| match component {
            Component::Normal(name) => Path::new(name)
                .extension()
                .is_some_and(|extension| extension == "app")
                .then_some(name),
            _ => None,
        });
    let name = match bundle {
        Some(bundle) => Path::new(bundle).file_stem()?,
        None => executable.file_name()?,
    };
    Some(name.to_string_lossy().into_owned())
}

/// AVFoundation's request for the microphone, through the Objective-C runtime.
mod microphone {
    use block2::RcBlock;
    use objc2::msg_send;
    use objc2::runtime::{AnyClass, Bool};
    use objc2_foundation::NSString;

    #[link(name = "AVFoundation", kind = "framework")]
    unsafe extern "C" {
        static AVMediaTypeAudio: Option<&'static NSString>;
    }

    pub(super) fn request(answered: impl Fn() + Send + Sync + 'static) {
        let Some(class) = AnyClass::get(c"AVCaptureDevice") else {
            return;
        };
        // SAFETY: a constant AVFoundation exports, read once the class lookup has shown it is loaded.
        let Some(media_type) = (unsafe { AVMediaTypeAudio }) else {
            return;
        };
        let handler = RcBlock::new(move |_granted: Bool| answered());
        // SAFETY: the class method takes a media type and a block taking BOOL, which AVFoundation copies
        // and calls once on a queue of its own.
        unsafe {
            let _: () = msg_send![
                class,
                requestAccessForMediaType: media_type,
                completionHandler: &*handler
            ];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_microphone_reads_as_the_window_names_it() {
        assert_eq!(access(MicrophoneAccess::Granted), Access::Granted);
        assert_eq!(access(MicrophoneAccess::NotAsked), Access::NotAsked);
        assert_eq!(access(MicrophoneAccess::Denied), Access::Denied);
        assert_eq!(access(MicrophoneAccess::Restricted), Access::Restricted);
    }

    #[test]
    fn a_program_is_named_by_its_bundle_or_else_its_executable() {
        let named = |path: &str| shown_name(Path::new(path));
        assert_eq!(
            named(
                "/Users/t/.local/share/herdr-core/active/Herdr Core.app/Contents/MacOS/herdr-core"
            )
            .as_deref(),
            Some("Herdr Core")
        );
        assert_eq!(
            named("/System/Applications/Utilities/Terminal.app/Contents/MacOS/Terminal").as_deref(),
            Some("Terminal")
        );
        assert_eq!(
            named("/Applications/Visual Studio Code.app/Contents/Frameworks/Code Helper.app/Contents/MacOS/Code Helper")
                .as_deref(),
            Some("Visual Studio Code"),
            "the outermost bundle"
        );
        assert_eq!(
            named("/opt/homebrew/bin/alacritty").as_deref(),
            Some("alacritty")
        );
    }

    #[test]
    fn skald_is_listed_as_itself_or_as_the_program_responsible_for_it() {
        // Run by cargo, the tests are the responsibility of the terminal or CI's runner, not their own.
        let me = std::process::id() as libc::pid_t;
        match responsible_for(me) {
            Some(pid) if pid != me => {
                assert!(matches!(listed_as(), ListedAs::StartedFrom(_)));
                assert!(executable(pid).is_some());
            }
            _ => assert_eq!(listed_as(), ListedAs::Skald),
        }
    }

    #[test]
    fn the_checks_ask_the_system_without_a_prompt() {
        let checks = checks();
        assert_eq!(
            (checks.microphone)(),
            access(skald_audio::microphone_access())
        );
        assert_eq!(
            (checks.accessibility)(),
            skald_input::accessibility_granted()
        );
    }
}
