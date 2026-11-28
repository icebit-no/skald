//! Start at login, the setting, on Windows: a value named Skald under the user's Run key, which Windows
//! starts at login and Task Manager lists among the start-up apps. The value names the exe that wrote it, so
//! one left by an exe that has since moved, as an unpacked zip moved elsewhere, is written again at the next
//! start. Scoop starts Skald from its `current` folder and winget from one folder per package, so an update
//! keeps the path.

use std::ptr::null_mut;

use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
use windows_sys::Win32::System::Registry::{
    HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ, RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW,
};

use crate::say::Line;

const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE: &str = "Skald";

#[derive(Debug, Clone, PartialEq, Eq)]
enum Step {
    Write(String),
    Remove,
    Nothing,
}

/// What brings the Run key to the setting from what it holds now.
fn step(wanted: bool, present: Option<&str>, command: &str) -> Step {
    match (wanted, present) {
        (true, Some(present)) if present == command => Step::Nothing,
        (true, _) => Step::Write(command.to_owned()),
        (false, Some(_)) => Step::Remove,
        (false, None) => Step::Nothing,
    }
}

/// The command Windows runs at login: the exe's path in quotes, since it may hold spaces.
fn command_for(executable: &std::path::Path) -> String {
    format!("\"{}\"", executable.display())
}

/// Brings the Run key to the setting; says what changed, or why it could not be.
pub(crate) fn apply(wanted: bool) -> Option<Line> {
    let executable = match std::env::current_exe() {
        Ok(executable) => executable,
        Err(error) => return wanted.then(|| Line::LoginItemFailed(error.to_string())),
    };
    let present = match read() {
        Ok(present) => present,
        Err(error) => return Some(Line::LoginItemFailed(error.to_string())),
    };
    let result = match step(wanted, present.as_deref(), &command_for(&executable)) {
        Step::Write(command) => write(&command).map(|()| Some(Line::LoginItemOn)),
        Step::Remove => remove().map(|()| Some(Line::LoginItemOff)),
        Step::Nothing => Ok(None),
    };
    result.unwrap_or_else(|error| Some(Line::LoginItemFailed(error.to_string())))
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain([0]).collect()
}

/// The Run value's text, or `None` when there is no value named Skald.
fn read() -> std::io::Result<Option<String>> {
    let (key, value) = (wide(RUN), wide(VALUE));
    let mut size = 0u32;
    // Safety: NUL-terminated names; the first call asks only for the size.
    let asked = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_SZ,
            null_mut(),
            null_mut(),
            &mut size,
        )
    };
    if asked == ERROR_FILE_NOT_FOUND {
        return Ok(None);
    }
    if asked != ERROR_SUCCESS {
        return Err(std::io::Error::from_raw_os_error(asked as i32));
    }
    let mut buffer = vec![0u16; (size as usize).div_ceil(2)];
    // Safety: as above, with a buffer of the size the first call gave.
    let read = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_SZ,
            null_mut(),
            buffer.as_mut_ptr().cast(),
            &mut size,
        )
    };
    if read != ERROR_SUCCESS {
        return Err(std::io::Error::from_raw_os_error(read as i32));
    }
    let text = String::from_utf16_lossy(&buffer);
    Ok(Some(text.trim_end_matches('\0').to_owned()))
}

fn write(command: &str) -> std::io::Result<()> {
    let (key, value, data) = (wide(RUN), wide(VALUE), wide(command));
    // Safety: NUL-terminated names and a NUL-terminated string of the size given, in bytes.
    let written = unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            value.as_ptr(),
            REG_SZ,
            data.as_ptr().cast(),
            (data.len() * size_of::<u16>()) as u32,
        )
    };
    if written != ERROR_SUCCESS {
        return Err(std::io::Error::from_raw_os_error(written as i32));
    }
    Ok(())
}

fn remove() -> std::io::Result<()> {
    let (key, value) = (wide(RUN), wide(VALUE));
    // Safety: NUL-terminated names.
    let removed = unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), value.as_ptr()) };
    if removed != ERROR_SUCCESS && removed != ERROR_FILE_NOT_FOUND {
        return Err(std::io::Error::from_raw_os_error(removed as i32));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn the_value_is_written_when_missing_or_naming_another_exe_and_removed_when_not_wanted() {
        let command = command_for(Path::new(r"C:\Users\A\scoop\apps\skald\current\skald.exe"));
        assert_eq!(
            command,
            r#""C:\Users\A\scoop\apps\skald\current\skald.exe""#
        );

        assert_eq!(step(true, None, &command), Step::Write(command.clone()));
        assert_eq!(
            step(true, Some(r#""D:\old\skald.exe""#), &command),
            Step::Write(command.clone())
        );
        assert_eq!(step(true, Some(&command), &command), Step::Nothing);
        assert_eq!(step(false, Some(&command), &command), Step::Remove);
        assert_eq!(step(false, None, &command), Step::Nothing);
    }
}
