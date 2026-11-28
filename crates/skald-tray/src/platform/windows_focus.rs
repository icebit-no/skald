//! Which window gets the focus back once the menu has closed. The menu needs Skald's hidden window in front to
//! close when the user clicks elsewhere, and the hidden window would keep the focus after a choice, so the
//! next take's paste would go nowhere.

use std::ptr::null_mut;

use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::Graphics::Dwm::{DWMWA_CLOAKED, DwmGetWindowAttribute};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GW_HWNDNEXT, GW_OWNER, GWL_EXSTYLE, GetClassNameW, GetForegroundWindow, GetTopWindow,
    GetWindow, GetWindowLongPtrW, IsIconic, IsWindowVisible, WS_EX_TOOLWINDOW,
};

/// The taskbar and its overflow, which a click on the icon puts in front.
const TASKBAR_CLASSES: [&str; 4] = [
    "Shell_TrayWnd",
    "Shell_SecondaryTrayWnd",
    "NotifyIconOverflowWindow",
    "TopLevelWindowForOverflowXamlIsland",
];

/// The window in front, or, when the click on the icon put the taskbar in front, the app window highest in
/// the z-order, which is the one the user was in. Null when there is none.
pub(crate) fn window_to_return_to(own: HWND) -> HWND {
    // Safety: takes nothing and returns a window handle or null.
    let front = unsafe { GetForegroundWindow() };
    if !front.is_null() && front != own && !is_taskbar(front) {
        return front;
    }
    // Safety: walks the top-level windows from the top; every handle comes from the system.
    let mut window = unsafe { GetTopWindow(null_mut()) };
    while !window.is_null() {
        if window != own && is_app_window(window) {
            return window;
        }
        window = unsafe { GetWindow(window, GW_HWNDNEXT) };
    }
    null_mut()
}

/// A window the user works in, as Alt+Tab lists it: shown, not minimised, owned by no other window, not a
/// tool window, not hidden by the system, and not the taskbar.
fn is_app_window(window: HWND) -> bool {
    // Safety: a window handle from the system; each call only reads.
    unsafe {
        IsWindowVisible(window) != 0
            && IsIconic(window) == 0
            && GetWindow(window, GW_OWNER).is_null()
            && GetWindowLongPtrW(window, GWL_EXSTYLE) & WS_EX_TOOLWINDOW as isize == 0
            && !cloaked(window)
            && !is_taskbar(window)
    }
}

/// Hidden by the desktop window manager, as a suspended Store app's window is while it still counts as shown.
fn cloaked(window: HWND) -> bool {
    let mut cloaked = 0u32;
    // Safety: asks for one 32-bit value into a variable of that size.
    let read = unsafe {
        DwmGetWindowAttribute(
            window,
            DWMWA_CLOAKED as u32,
            (&mut cloaked as *mut u32).cast(),
            size_of::<u32>() as u32,
        )
    };
    read == 0 && cloaked != 0
}

fn is_taskbar(window: HWND) -> bool {
    let mut class = [0u16; 64];
    // Safety: a buffer of the length given.
    let len = unsafe { GetClassNameW(window, class.as_mut_ptr(), class.len() as i32) };
    let Ok(len) = usize::try_from(len) else {
        return false;
    };
    let class = String::from_utf16_lossy(&class[..len]);
    TASKBAR_CLASSES.contains(&class.as_str())
}
