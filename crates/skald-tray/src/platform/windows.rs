//! The item in Windows' notification area: ᚨ in white on a dark taskbar and black on a light one, with
//! the dot while a take records or is transcribed, and the menu when it is clicked.
//!
//! The icon talks to a hidden window of its own, made on the thread that makes the item, whose messages
//! that thread's message loop dispatches, as GPUI's does. The menu is built from the model last shown
//! each time it opens, so an update while it is open shows the next time, as on macOS.

use std::cell::{Cell, RefCell};
use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateBitmap, CreateDIBSection, DIB_RGB_COLORS,
    DeleteObject,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW};
use windows_sys::Win32::UI::Shell::{
    NIF_ICON, NIF_MESSAGE, NIF_SHOWTIP, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY, NIM_SETVERSION,
    NIN_SELECT, NOTIFYICON_VERSION_4, NOTIFYICONDATAW, Shell_NotifyIconW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreateIconIndirect, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyIcon,
    DestroyMenu, DestroyWindow, GWLP_USERDATA, GetCursorPos, GetSystemMetrics, GetWindowLongPtrW,
    HICON, HMENU, ICONINFO, MF_CHECKED, MF_GRAYED, MF_POPUP, MF_SEPARATOR, MF_STRING, PostMessageW,
    RegisterClassW, RegisterWindowMessageW, SM_CXSMICON, SetForegroundWindow, SetWindowLongPtrW,
    TPM_RETURNCMD, TPM_RIGHTBUTTON, TrackPopupMenuEx, WM_APP, WM_CONTEXTMENU, WM_NULL,
    WM_SETTINGCHANGE, WNDCLASSW, WS_OVERLAPPED,
};

use crate::menu::Entry;
use crate::rune::{self, Rgb};
use crate::{Action, Activity, TrayError, TrayModel};

/// What the icon sends the window when it is clicked.
const ICON_MESSAGE: u32 = WM_APP + 1;
const ICON_ID: u32 = 1;
/// The press of Enter or Space on the icon, which version 4 of the icon's messages reports as this.
const NIN_KEYSELECT: u32 = NIN_SELECT | 1;

/// The identity's colours for the dot: the darker on a light taskbar, the lighter on a dark one (D-27).
const OCHRE: (Rgb, Rgb) = (Rgb(0xc2, 0x41, 0x2d), Rgb(0xe2, 0x66, 0x4e));
const GOLD: (Rgb, Rgb) = (Rgb(0xd9, 0xa9, 0x4e), Rgb(0xf0, 0xc6, 0x74));

pub(crate) struct StatusItem {
    window: HWND,
    /// Read by the window's procedure through the window's user data, so it stays at one address. The
    /// procedure runs again inside the menu's own message loop, so the state is only ever shared, and
    /// what changes sits in cells.
    state: Box<State>,
}

struct State {
    model: RefCell<TrayModel>,
    on_action: Box<dyn Fn(Action)>,
    /// What the icon on show was drawn for, so an unchanged one is not drawn again.
    drawn: Cell<Option<(Activity, bool)>>,
    icon: Cell<HICON>,
    /// Explorer sends this when the taskbar is made again, as after it restarts; the icon must then be
    /// added again.
    taskbar_created: u32,
}

impl StatusItem {
    pub(crate) fn new(
        model: &TrayModel,
        on_action: Box<dyn Fn(Action)>,
    ) -> Result<Self, TrayError> {
        let class = wide("SkaldNotificationArea");
        // Safety: a class with a valid procedure and NUL-terminated name, registered for this module; a
        // second registration fails harmlessly and the first one is used.
        let window = unsafe {
            let instance = GetModuleHandleW(null());
            let window_class = WNDCLASSW {
                lpfnWndProc: Some(procedure),
                hInstance: instance,
                lpszClassName: class.as_ptr(),
                ..std::mem::zeroed()
            };
            RegisterClassW(&window_class);
            CreateWindowExW(
                0,
                class.as_ptr(),
                wide("Skald").as_ptr(),
                WS_OVERLAPPED,
                0,
                0,
                0,
                0,
                null_mut(),
                null_mut(),
                instance,
                null(),
            )
        };
        if window.is_null() {
            return Err(TrayError::Refused);
        }
        let state = Box::new(State {
            model: RefCell::new(model.clone()),
            on_action,
            drawn: Cell::new(None),
            icon: Cell::new(null_mut()),
            // Safety: a NUL-terminated name.
            taskbar_created: unsafe { RegisterWindowMessageW(wide("TaskbarCreated").as_ptr()) },
        });
        // Safety: the state is boxed, so its address holds until the item is dropped, which takes it
        // back out of the window before freeing it.
        unsafe { SetWindowLongPtrW(window, GWLP_USERDATA, &*state as *const State as isize) };
        let item = Self { window, state };
        item.state.redraw();
        if !item.state.add(window) {
            return Err(TrayError::Refused);
        }
        Ok(item)
    }

    pub(crate) fn show(&mut self, model: &TrayModel) {
        if *self.state.model.borrow() == *model {
            return;
        }
        *self.state.model.borrow_mut() = model.clone();
        self.state.redraw();
        self.state.notify(self.window, NIM_MODIFY);
    }
}

impl Drop for StatusItem {
    fn drop(&mut self) {
        // Safety: the icon and the window are this item's; the window lets go of the state before the
        // box is freed.
        unsafe {
            self.state.notify(self.window, NIM_DELETE);
            SetWindowLongPtrW(self.window, GWLP_USERDATA, 0);
            DestroyWindow(self.window);
            let icon = self.state.icon.get();
            if !icon.is_null() {
                DestroyIcon(icon);
            }
        }
    }
}

impl State {
    fn add(&self, window: HWND) -> bool {
        if !self.notify(window, NIM_ADD) {
            return false;
        }
        let mut data = self.data(window);
        data.Anonymous.uVersion = NOTIFYICON_VERSION_4;
        // Safety: a fully initialised structure of the size it states.
        unsafe { Shell_NotifyIconW(NIM_SETVERSION, &data) != 0 }
    }

    fn notify(&self, window: HWND, message: u32) -> bool {
        let data = self.data(window);
        // Safety: a fully initialised structure of the size it states.
        unsafe { Shell_NotifyIconW(message, &data) != 0 }
    }

    fn data(&self, window: HWND) -> NOTIFYICONDATAW {
        // Safety: a plain C structure, for which all zeros is a valid value.
        let mut data: NOTIFYICONDATAW = unsafe { std::mem::zeroed() };
        data.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
        data.hWnd = window;
        data.uID = ICON_ID;
        data.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP | NIF_SHOWTIP;
        data.uCallbackMessage = ICON_MESSAGE;
        data.hIcon = self.icon.get();
        let tip = match self.model.borrow().activity {
            Activity::Idle => "Skald",
            Activity::Recording => "Skald · Opptak",
            Activity::Transcribing => "Skald · Rister",
        };
        for (slot, unit) in data.szTip.iter_mut().zip(tip.encode_utf16()) {
            *slot = unit;
        }
        data
    }

    /// Draws the icon again when the activity or the taskbar's colour has changed.
    fn redraw(&self) {
        let light = taskbar_is_light();
        let activity = self.model.borrow().activity;
        let wanted = (activity, light);
        if self.drawn.get() == Some(wanted) {
            return;
        }
        let pick = |(darker, lighter): (Rgb, Rgb)| if light { darker } else { lighter };
        let dot = match activity {
            Activity::Idle => None,
            Activity::Recording => Some(pick(OCHRE)),
            Activity::Transcribing => Some(pick(GOLD)),
        };
        let rune = if light {
            Rgb(0, 0, 0)
        } else {
            Rgb(255, 255, 255)
        };
        if let Some(icon) = icon(rune, dot) {
            let shown = self.icon.replace(icon);
            if !shown.is_null() {
                // Safety: the icon this state made before; the item shows the new one from its next
                // notice, and Windows keeps its own copy of an icon it is given.
                unsafe { DestroyIcon(shown) };
            }
            self.drawn.set(Some(wanted));
        }
    }

    /// Opens the menu at the given point and acts on the entry chosen.
    fn open_menu(&self, window: HWND, at: POINT) {
        let mut actions = Vec::new();
        let entries = self.model.borrow().menu();
        let menu = build(&entries, &mut actions);
        if menu.is_null() {
            return;
        }
        // Safety: the menu and the window are alive; the window is brought to the front first so the menu
        // closes when the user clicks elsewhere, and is sent a message after, both as the documentation
        // for TrackPopupMenu asks.
        let chosen = unsafe {
            SetForegroundWindow(window);
            let chosen = TrackPopupMenuEx(
                menu,
                TPM_RETURNCMD | TPM_RIGHTBUTTON,
                at.x,
                at.y,
                window,
                null(),
            );
            PostMessageW(window, WM_NULL, 0, 0);
            DestroyMenu(menu);
            chosen
        };
        if let Some(action) = usize::try_from(chosen)
            .ok()
            .and_then(|id| id.checked_sub(1))
            .and_then(|index| actions.get(index))
        {
            (self.on_action)(action.clone());
        }
    }
}

/// The menu for `entries`; each choice's command is its place in `actions`, counted from one.
fn build(entries: &[Entry], actions: &mut Vec<Action>) -> HMENU {
    // Safety: a new menu, filled with NUL-terminated titles and the submenus made here, which it owns.
    unsafe {
        let menu = CreatePopupMenu();
        if menu.is_null() {
            return menu;
        }
        for entry in entries {
            match entry {
                Entry::Label { title, greyed } => {
                    let flags = if *greyed { MF_GRAYED } else { MF_STRING };
                    AppendMenuW(menu, flags, 0, wide(title).as_ptr());
                }
                Entry::Choice(choice) => {
                    actions.push(choice.action.clone());
                    let mut flags = MF_STRING;
                    if !choice.enabled {
                        flags |= MF_GRAYED;
                    }
                    if choice.checked {
                        flags |= MF_CHECKED;
                    }
                    let title = if choice.pinned {
                        format!("{} 📌", choice.title)
                    } else {
                        choice.title.clone()
                    };
                    AppendMenuW(menu, flags, actions.len(), wide(&title).as_ptr());
                }
                Entry::Submenu { title, entries } => {
                    let submenu = build(entries, actions);
                    let flags = if entries.is_empty() {
                        MF_POPUP | MF_GRAYED
                    } else {
                        MF_POPUP
                    };
                    AppendMenuW(menu, flags, submenu as usize, wide(title).as_ptr());
                }
                Entry::Separator => {
                    AppendMenuW(menu, MF_SEPARATOR, 0, null());
                }
            }
        }
        menu
    }
}

unsafe extern "system" fn procedure(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // Safety: the user data is null or the item's state, which lives while the window does.
    let state = unsafe { (GetWindowLongPtrW(window, GWLP_USERDATA) as *const State).as_ref() };
    if let Some(state) = state {
        if message == ICON_MESSAGE {
            // Version 4 puts the event in the low word of lparam and the point in wparam.
            let event = (lparam as u32) & 0xFFFF;
            if matches!(event, WM_CONTEXTMENU | NIN_SELECT | NIN_KEYSELECT) {
                let mut at = POINT {
                    x: i32::from(wparam as u16 as i16),
                    y: i32::from((wparam >> 16) as u16 as i16),
                };
                if event == NIN_SELECT {
                    // A click: open where the pointer is, which wparam gives only for the menu key.
                    // Safety: a valid out pointer.
                    unsafe { GetCursorPos(&mut at) };
                }
                state.open_menu(window, at);
            }
            return 0;
        }
        if message == state.taskbar_created {
            state.add(window);
            return 0;
        }
        if message == WM_SETTINGCHANGE {
            state.redraw();
            state.notify(window, NIM_MODIFY);
        }
    }
    // Safety: every other message gets the system's own handling.
    unsafe { DefWindowProcW(window, message, wparam, lparam) }
}

/// Whether the taskbar is light, as Windows' «light» mode for the system makes it.
fn taskbar_is_light() -> bool {
    let key = wide(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize");
    let value = wide("SystemUsesLightTheme");
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
    // Without the value, as before Windows 10's light mode, the taskbar is dark.
    read == 0 && data == 1
}

/// The icon at the notification area's size, drawn from the rune.
fn icon(rune: Rgb, dot: Option<Rgb>) -> Option<HICON> {
    // Safety: takes an index and only reads.
    let size = unsafe { GetSystemMetrics(SM_CXSMICON) }.clamp(16, 64) as u32;
    let pixels = rune::pixels(size, rune, dot);
    // Safety: a top-down 32-bit bitmap whose bits are written within its size, and a mask of the same
    // size; both are handed to the icon, which copies them, and deleted after.
    unsafe {
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: size as i32,
                biHeight: -(size as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                ..std::mem::zeroed()
            },
            ..std::mem::zeroed()
        };
        let mut bits = null_mut();
        let colour = CreateDIBSection(null_mut(), &info, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
        if colour.is_null() || bits.is_null() {
            return None;
        }
        std::ptr::copy_nonoverlapping(
            pixels.as_ptr().cast::<u8>(),
            bits.cast::<u8>(),
            pixels.len() * 4,
        );
        // The colour's alpha decides what shows; the mask is all zeros, its rows two-byte aligned.
        let mask_bits = vec![0u8; size.div_ceil(16) as usize * 2 * size as usize];
        let mask = CreateBitmap(size as i32, size as i32, 1, 1, mask_bits.as_ptr().cast());
        let icon_info = ICONINFO {
            fIcon: 1,
            xHotspot: 0,
            yHotspot: 0,
            hbmMask: mask,
            hbmColor: colour,
        };
        let icon = CreateIconIndirect(&icon_info);
        DeleteObject(colour);
        DeleteObject(mask);
        (!icon.is_null()).then_some(icon)
    }
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain([0]).collect()
}
