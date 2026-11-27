//! The console: the one a command was run from, which a windowed program has to attach to before it can
//! print there, and its Ctrl-C, Ctrl-Break and close, which end the loop as the signals do on Unix: the
//! first sets the flag the loop watches, and a second one ends the process at once.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use windows_sys::Win32::System::Console::{
    ATTACH_PARENT_PROCESS, AttachConsole, CTRL_BREAK_EVENT, CTRL_C_EVENT, SetConsoleCtrlHandler,
};
use windows_sys::core::BOOL;

static RECEIVED: OnceLock<&'static AtomicBool> = OnceLock::new();

/// Windows ends the process when the handler of a close returns, so it waits this long first for the loop
/// to drop the model.
const CLOSE_GRACE: Duration = Duration::from_secs(4);

/// Writes standard output and error to the console of the program that ran this one, when there is one
/// and they are not already redirected, as to a pipe an agent reads. Started from the Start menu there
/// is none, and nothing is printed.
pub(crate) fn attach_to_parent() {
    // Safety: takes a process id and only joins its console; the standard handles it sets are this
    // process's own.
    unsafe { AttachConsole(ATTACH_PARENT_PROCESS) };
}

pub(crate) fn on_ctrl_c(received: &'static AtomicBool) -> std::io::Result<()> {
    let _ = RECEIVED.set(received);
    // Safety: a handler with the signature the system calls, which only touches an atomic and sleeps.
    if unsafe { SetConsoleCtrlHandler(Some(handle), 1) } == 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

unsafe extern "system" fn handle(event: u32) -> BOOL {
    let Some(received) = RECEIVED.get() else {
        return 0;
    };
    // A second event goes to the system's own handler, which ends the process.
    if received.swap(true, Ordering::Relaxed) {
        return 0;
    }
    if event != CTRL_C_EVENT && event != CTRL_BREAK_EVENT {
        std::thread::sleep(CLOSE_GRACE);
    }
    1
}
