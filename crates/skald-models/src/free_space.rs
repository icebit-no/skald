//! Room left on the disk that holds a folder, as the system would give it to a file the user asked for.

use std::io;
use std::path::Path;

/// Bytes a new file in the folder can take. The folder must exist.
#[cfg(target_os = "macos")]
pub fn available(dir: &Path) -> io::Result<u64> {
    // statvfs leaves out space APFS frees on demand (purgeable files, local snapshots), so on a nearly full
    // Mac it can refuse a download the system would make room for.
    match important_usage(dir) {
        Some(bytes) => Ok(bytes),
        None => statvfs(dir),
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
pub fn available(dir: &Path) -> io::Result<u64> {
    statvfs(dir)
}

#[cfg(target_os = "macos")]
fn important_usage(dir: &Path) -> Option<u64> {
    use objc2::rc::{Retained, autoreleasepool};
    use objc2::runtime::AnyObject;
    use objc2_foundation::{
        NSNumber, NSString, NSURL, NSURLVolumeAvailableCapacityForImportantUsageKey,
    };

    let path = dir.to_str()?;
    autoreleasepool(|_| {
        let url = NSURL::fileURLWithPath(&NSString::from_str(path));
        let mut value: Option<Retained<AnyObject>> = None;
        // SAFETY: the key is a constant Foundation defines, and the value is checked to be the NSNumber the
        // key promises before it is read.
        unsafe {
            url.getResourceValue_forKey_error(
                &mut value,
                NSURLVolumeAvailableCapacityForImportantUsageKey,
            )
        }
        .ok()?;
        let number = value?.downcast::<NSNumber>().ok()?;
        u64::try_from(number.longLongValue()).ok()
    })
}

#[cfg(unix)]
fn statvfs(dir: &Path) -> io::Result<u64> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let path = CString::new(dir.as_os_str().as_bytes())
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
    let mut stats = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    // SAFETY: `path` is a NUL-terminated string and `stats` has room for the struct statvfs writes.
    if unsafe { libc::statvfs(path.as_ptr(), stats.as_mut_ptr()) } != 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: statvfs returned 0, so it filled the struct.
    let stats = unsafe { stats.assume_init() };
    #[allow(clippy::useless_conversion)] // the field types differ between systems
    Ok(u64::from(stats.f_bavail).saturating_mul(u64::from(stats.f_frsize)))
}

#[cfg(windows)]
pub fn available(dir: &Path) -> io::Result<u64> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

    let wide: Vec<u16> = dir.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut free_to_caller = 0u64;
    // SAFETY: `wide` is NUL-terminated, and the two totals Skald does not need may be null.
    let ok = unsafe {
        GetDiskFreeSpaceExW(
            wide.as_ptr(),
            &mut free_to_caller,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(free_to_caller)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_room_on_the_disk_of_an_existing_folder() {
        let bytes = available(&std::env::temp_dir()).unwrap();
        assert!(bytes > 0);
    }

    #[cfg(unix)]
    #[test]
    fn statvfs_fails_for_a_missing_folder() {
        let missing = std::env::temp_dir().join("skald-models-no-such-folder/deeper");
        assert!(statvfs(&missing).is_err());
    }
}
