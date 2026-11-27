//! Start at login, the setting: Skald.app registered as its own login item through SMAppService (macOS 13
//! and later), which System Settings › General › Login Items lists and can turn off. The service belongs
//! to an app bundle, so a `skald` run from anywhere else leaves login items alone and says why.

use std::path::Path;

use crate::say::Line;

/// SMAppService's status of the app's own login item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Status {
    NotRegistered,
    Enabled,
    /// Registered, and waiting for the user to allow it in System Settings.
    RequiresApproval,
    /// The system has no such service, as for an app it cannot find.
    NotFound,
}

impl Status {
    /// SMAppServiceStatus's raw value; one this version does not know reads as not found.
    fn from_raw(raw: isize) -> Self {
        match raw {
            0 => Self::NotRegistered,
            1 => Self::Enabled,
            2 => Self::RequiresApproval,
            _ => Self::NotFound,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Step {
    Register,
    Unregister,
    /// Registered, and only the user can allow it.
    AskForApproval,
    Nothing,
}

/// What brings the login item to the setting from where it stands.
pub(crate) fn step(wanted: bool, status: Status) -> Step {
    match (wanted, status) {
        (true, Status::NotRegistered | Status::NotFound) => Step::Register,
        (true, Status::RequiresApproval) => Step::AskForApproval,
        (false, Status::Enabled | Status::RequiresApproval) => Step::Unregister,
        (true, Status::Enabled) | (false, Status::NotRegistered | Status::NotFound) => {
            Step::Nothing
        }
    }
}

/// Whether the executable is an app bundle's own, the only kind SMAppService registers as an app.
pub(crate) fn in_app_bundle(executable: &Path) -> bool {
    let Some(macos) = executable.parent() else {
        return false;
    };
    let contents = macos.parent();
    let bundle = contents.and_then(Path::parent);
    macos.file_name().is_some_and(|name| name == "MacOS")
        && contents
            .and_then(Path::file_name)
            .is_some_and(|name| name == "Contents")
        && bundle
            .and_then(Path::extension)
            .is_some_and(|extension| extension == "app")
}

/// Brings the login item to the setting, and says what the user needs to know: that it needs the app,
/// that System Settings must allow it, or why it failed.
pub(crate) fn apply(wanted: bool) -> Option<Line> {
    let in_bundle = std::env::current_exe().is_ok_and(|path| in_app_bundle(&path));
    if !in_bundle {
        return wanted.then_some(Line::LoginItemNeedsApp);
    }
    let Some(service) = service::main_app() else {
        return wanted.then_some(Line::LoginItemUnsupported);
    };
    match step(wanted, Status::from_raw(service::status(&service))) {
        Step::Nothing => None,
        Step::AskForApproval => Some(Line::LoginItemNeedsApproval),
        Step::Register => match service::register(&service) {
            Ok(()) if Status::from_raw(service::status(&service)) == Status::RequiresApproval => {
                Some(Line::LoginItemNeedsApproval)
            }
            Ok(()) => Some(Line::LoginItemOn),
            Err(reason) => Some(Line::LoginItemFailed(reason)),
        },
        Step::Unregister => match service::unregister(&service) {
            Ok(()) => Some(Line::LoginItemOff),
            Err(reason) => Some(Line::LoginItemFailed(reason)),
        },
    }
}

/// SMAppService through the Objective-C runtime. The class is looked up when asked for, so a macOS older
/// than 13 says it has none rather than failing to start.
mod service {
    use objc2::msg_send;
    use objc2::rc::Retained;
    use objc2::runtime::{AnyClass, AnyObject};
    use objc2_foundation::NSError;

    #[link(name = "ServiceManagement", kind = "framework")]
    unsafe extern "C" {}

    pub(super) fn main_app() -> Option<Retained<AnyObject>> {
        let class = AnyClass::get(c"SMAppService")?;
        // SAFETY: `mainAppService` is a class method that takes nothing and returns the app's service.
        unsafe { msg_send![class, mainAppService] }
    }

    pub(super) fn status(service: &AnyObject) -> isize {
        // SAFETY: `status` takes nothing and returns an NSInteger.
        unsafe { msg_send![service, status] }
    }

    pub(super) fn register(service: &AnyObject) -> Result<(), String> {
        // SAFETY: takes an NSError out-parameter and returns BOOL, the error convention `_` stands for.
        let result: Result<(), Retained<NSError>> =
            unsafe { msg_send![service, registerAndReturnError: _] };
        result.map_err(|error| error.localizedDescription().to_string())
    }

    pub(super) fn unregister(service: &AnyObject) -> Result<(), String> {
        // SAFETY: as for `register`.
        let result: Result<(), Retained<NSError>> =
            unsafe { msg_send![service, unregisterAndReturnError: _] };
        result.map_err(|error| error.localizedDescription().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_login_item_follows_the_setting_and_leaves_approval_to_the_user() {
        assert_eq!(step(true, Status::NotRegistered), Step::Register);
        assert_eq!(step(true, Status::NotFound), Step::Register);
        assert_eq!(step(true, Status::Enabled), Step::Nothing);
        assert_eq!(step(true, Status::RequiresApproval), Step::AskForApproval);
        assert_eq!(step(false, Status::Enabled), Step::Unregister);
        assert_eq!(step(false, Status::RequiresApproval), Step::Unregister);
        assert_eq!(step(false, Status::NotRegistered), Step::Nothing);
        assert_eq!(step(false, Status::NotFound), Step::Nothing);
    }

    #[test]
    fn the_raw_status_reads_as_sm_app_service_numbers_it() {
        assert_eq!(Status::from_raw(0), Status::NotRegistered);
        assert_eq!(Status::from_raw(1), Status::Enabled);
        assert_eq!(Status::from_raw(2), Status::RequiresApproval);
        assert_eq!(Status::from_raw(3), Status::NotFound);
        assert_eq!(Status::from_raw(9), Status::NotFound);
    }

    #[test]
    fn only_an_app_bundle_s_executable_is_in_one() {
        assert!(in_app_bundle(Path::new(
            "/Applications/Skald.app/Contents/MacOS/skald"
        )));
        for path in [
            "/Users/a/skald/target/debug/skald",
            "/opt/homebrew/bin/skald",
            "/Applications/Skald.app/Contents/Resources/skald",
            "/tmp/Contents/MacOS/skald",
            "skald",
        ] {
            assert!(!in_app_bundle(Path::new(path)), "{path}");
        }
    }

    #[test]
    fn a_skald_outside_an_app_touches_no_login_item() {
        // The test runs from target/, not from an app bundle.
        assert_eq!(apply(true), Some(Line::LoginItemNeedsApp));
        assert_eq!(apply(false), None);
    }

    #[test]
    fn this_macos_has_the_service() {
        let service = service::main_app().expect("SMAppService, macOS 13 and later");
        // A test binary is no app; whatever it reads, the number is one the status knows or not found.
        let _ = Status::from_raw(service::status(&service));
    }
}
