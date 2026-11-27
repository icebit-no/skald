//! Whether the window in front runs with more rights than Skald. Windows keeps a process from sending
//! input to a window of a higher integrity level, as an app run as administrator has, and drops the
//! keystrokes without an error (User Interface Privilege Isolation).

use std::ffi::c_void;
use std::ptr::null_mut;

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
use windows_sys::Win32::Security::{
    GetSidSubAuthority, GetSidSubAuthorityCount, GetTokenInformation, TOKEN_MANDATORY_LABEL,
    TOKEN_QUERY, TokenIntegrityLevel,
};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

/// False when there is no window in front or its level cannot be read, so the paste is tried as usual.
pub(crate) fn foreground_window_elevated() -> bool {
    // Safety: takes nothing and returns a window handle or null.
    let window = unsafe { GetForegroundWindow() };
    if window.is_null() {
        return false;
    }
    let mut process_id = 0;
    // Safety: a window handle from the system and a valid out pointer.
    unsafe { GetWindowThreadProcessId(window, &mut process_id) };
    if process_id == 0 {
        return false;
    }
    match (process_integrity(process_id), own_integrity()) {
        (Some(theirs), Some(ours)) => theirs > ours,
        _ => false,
    }
}

/// The integrity level of this process: medium for an app the user started, high when run as
/// administrator.
fn own_integrity() -> Option<u32> {
    // Safety: the pseudo handle of this process, which needs no closing.
    integrity(unsafe { GetCurrentProcess() })
}

fn process_integrity(process_id: u32) -> Option<u32> {
    // Safety: asks for the right to read the process's limited information, which the system grants for
    // a process of any level of the same user.
    let process = Owned(unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, process_id) });
    if process.0.is_null() {
        return None;
    }
    integrity(process.0)
}

/// The last part of the token's integrity label, which is the level itself.
fn integrity(process: HANDLE) -> Option<u32> {
    let mut token = null_mut();
    // Safety: an open process handle and a valid out pointer; the token is closed on drop.
    if unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut token) } == 0 {
        return None;
    }
    let token = Owned(token);
    let mut needed = 0;
    // Safety: asks only for the size the label needs.
    unsafe { GetTokenInformation(token.0, TokenIntegrityLevel, null_mut(), 0, &mut needed) };
    if needed == 0 {
        return None;
    }
    // u64s keep the label aligned for the pointers it holds.
    let mut buffer = vec![0u64; (needed as usize).div_ceil(size_of::<u64>())];
    // Safety: the buffer holds at least `needed` bytes; the label and its SID are read only while the
    // buffer lives, and the SID's parts within the count it reports.
    unsafe {
        let filled = GetTokenInformation(
            token.0,
            TokenIntegrityLevel,
            buffer.as_mut_ptr().cast::<c_void>(),
            needed,
            &mut needed,
        );
        if filled == 0 {
            return None;
        }
        let label = &*buffer.as_ptr().cast::<TOKEN_MANDATORY_LABEL>();
        let sid = label.Label.Sid;
        let parts = *GetSidSubAuthorityCount(sid);
        let last = u32::from(parts.checked_sub(1)?);
        Some(*GetSidSubAuthority(sid, last))
    }
}

/// A handle closed when dropped.
struct Owned(HANDLE);

impl Drop for Owned {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // Safety: a handle this module opened, closed once.
            unsafe { CloseHandle(self.0) };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Untrusted, low, medium, high and system are 0x0000 to 0x4000 in steps of 0x1000.
    #[test]
    fn this_process_has_a_level_and_its_own_process_is_not_above_it() {
        let ours = own_integrity().expect("this process's level");
        assert!(ours <= 0x4000, "{ours:#x}");
        assert_eq!(process_integrity(std::process::id()), Some(ours));
    }

    #[test]
    fn a_process_that_does_not_exist_has_no_level() {
        assert_eq!(process_integrity(u32::MAX - 3), None);
    }
}
