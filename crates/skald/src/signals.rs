//! Ctrl-C, and the signals a closing terminal or the system sends, end the loop the same way: the take in
//! progress is thrown away, the takes already handed over are finished, and the model is dropped before the
//! process exits. A second signal ends the process at once, as if none were caught.

use std::sync::atomic::{AtomicBool, Ordering};

static RECEIVED: AtomicBool = AtomicBool::new(false);

pub(crate) fn received() -> bool {
    RECEIVED.load(Ordering::Relaxed)
}

#[cfg(unix)]
pub(crate) fn install() -> std::io::Result<()> {
    install_for(&[libc::SIGINT, libc::SIGTERM, libc::SIGHUP])
}

#[cfg(windows)]
pub(crate) fn install() -> std::io::Result<()> {
    crate::windows::console::on_ctrl_c(&RECEIVED)
}

#[cfg(not(any(unix, windows)))]
pub(crate) fn install() -> std::io::Result<()> {
    Ok(())
}

#[cfg(unix)]
fn install_for(signals: &[libc::c_int]) -> std::io::Result<()> {
    extern "C" fn on_signal(_signal: libc::c_int) {
        // An atomic store is all a signal handler may safely do here.
        RECEIVED.store(true, Ordering::Relaxed);
    }
    for &signal in signals {
        // SAFETY: a zeroed sigaction is a valid value to fill in; the mask is then emptied by the call made
        // for that.
        let mut action: libc::sigaction = unsafe { std::mem::zeroed() };
        action.sa_sigaction = on_signal as extern "C" fn(libc::c_int) as libc::sighandler_t;
        action.sa_flags = libc::SA_RESETHAND;
        // SAFETY: both pointers are valid for the calls, and the handler only stores to an atomic.
        let failed = unsafe {
            libc::sigemptyset(&mut action.sa_mask);
            libc::sigaction(signal, &action, std::ptr::null_mut())
        } != 0;
        if failed {
            return Err(std::io::Error::last_os_error());
        }
    }
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn a_caught_signal_is_seen_by_the_loop() {
        assert!(!received());
        install_for(&[libc::SIGUSR1]).unwrap();
        // SAFETY: raising a signal whose handler was just installed.
        assert_eq!(unsafe { libc::raise(libc::SIGUSR1) }, 0);
        assert!(received());
    }
}
