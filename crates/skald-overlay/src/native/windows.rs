//! The overlay on Windows. GPUI's pop-up is a topmost tool window shown without activation; this adds the
//! rest: clicks go through it to the window below and never activate it, the screen is the one with the
//! window in front, the accent colour and the system's animation setting are read, and Kjedelig's acrylic
//! is cut to the pill's shape.

use std::ptr::null_mut;

use gpui_kit::{Rgba, Window};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::Graphics::Gdi::{
    CreateRoundRectRgn, MONITOR_DEFAULTTONEAREST, MonitorFromWindow, SetWindowRgn,
};
use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW};
use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GWL_EXSTYLE, GetForegroundWindow, GetWindowLongPtrW, LWA_ALPHA, SPI_GETCLIENTAREAANIMATION,
    SetLayeredWindowAttributes, SetWindowLongPtrW, SystemParametersInfoW, WS_EX_LAYERED,
    WS_EX_NOACTIVATE, WS_EX_TRANSPARENT,
};

use super::PillFrame;

fn hwnd(window: &Window) -> Option<HWND> {
    // GPUI's own `window_handle` names the window in GPUI; this one is the platform's.
    let handle = HasWindowHandle::window_handle(window).ok()?;
    let RawWindowHandle::Win32(win32) = handle.as_raw() else {
        return None;
    };
    Some(win32.hwnd.get() as HWND)
}

/// Windows passes a click through a window only when it is layered as well as transparent to the mouse.
pub(crate) fn let_clicks_through(window: &Window) {
    let Some(hwnd) = hwnd(window) else {
        return;
    };
    // Safety: the window is GPUI's and alive while borrowed; only its extended style changes. A layered
    // window shows only once its opacity is set, here to whole, so the drawing's own alpha applies.
    unsafe {
        let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let through = (WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE) as isize;
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, style | through);
        SetLayeredWindowAttributes(hwnd, 0, 255, LWA_ALPHA);
    }
}

/// The screen with the window in front, where the text will land. GPUI names a screen on Windows by its
/// monitor handle.
pub(crate) fn active_display() -> Option<u64> {
    // Safety: takes nothing, or a window handle the system just gave, and only reads.
    unsafe {
        let front = GetForegroundWindow();
        if front.is_null() {
            return None;
        }
        let monitor = MonitorFromWindow(front, MONITOR_DEFAULTTONEAREST);
        (!monitor.is_null()).then_some(monitor as u64)
    }
}

/// The accent colour chosen in Windows' settings.
pub(crate) fn accent_color() -> Option<Rgba> {
    // Alpha, blue, green and red, a byte each from the top.
    let abgr = dword(r"Software\Microsoft\Windows\DWM", "AccentColor")?;
    let channel = |shift: u32| ((abgr >> shift) & 0xFF) as f32 / 255.0;
    Some(Rgba {
        r: channel(0),
        g: channel(8),
        b: channel(16),
        a: 1.0,
    })
}

/// «Animation effects» turned off in Windows' accessibility settings.
pub(crate) fn prefers_reduced_motion() -> bool {
    let mut animates = 1i32;
    // Safety: the call writes one BOOL into the value given.
    let read = unsafe {
        SystemParametersInfoW(
            SPI_GETCLIENTAREAANIMATION,
            0,
            (&mut animates as *mut i32).cast(),
            0,
        )
    };
    read != 0 && animates == 0
}

/// Cuts the window to the pill, so the acrylic behind it takes the capsule's shape. The window is given
/// in points; the region is in the display's pixels.
pub(crate) fn shape_material(window: &Window, pill: PillFrame) {
    let Some(hwnd) = hwnd(window) else {
        return;
    };
    // Safety: the window is GPUI's and alive while borrowed; once set, the region is the system's.
    unsafe {
        let scale = f64::from(GetDpiForWindow(hwnd).max(96)) / 96.0;
        let at = |value: f64| (value * scale).round() as i32;
        let region = CreateRoundRectRgn(
            at(pill.x),
            at(pill.y),
            at(pill.x + pill.width) + 1,
            at(pill.y + pill.height) + 1,
            at(pill.radius * 2.0),
            at(pill.radius * 2.0),
        );
        if !region.is_null() {
            SetWindowRgn(hwnd, region, 1);
        }
    }
}

fn dword(key: &str, value: &str) -> Option<u32> {
    let key: Vec<u16> = key.encode_utf16().chain([0]).collect();
    let value: Vec<u16> = value.encode_utf16().chain([0]).collect();
    let mut data = 0u32;
    let mut size = size_of::<u32>() as u32;
    // Safety: NUL-terminated names and an out buffer of the size given.
    let read = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_DWORD,
            null_mut(),
            (&mut data as *mut u32).cast(),
            &mut size,
        )
    };
    (read == 0).then_some(data)
}
