//! The clipboard's contents, every format it holds, read and written back unchanged.
//!
//! Most formats are a block of memory and are kept as their bytes; an enhanced metafile is kept as its
//! bytes too. Formats that are handles to objects only their owner understands are left out, and so are
//! the ones the system makes from a format that is kept (a bitmap and its palette from the DIB, a metafile
//! picture from the enhanced metafile), which it makes again once the kept one is back.

use std::ptr::null_mut;
use std::thread::sleep;
use std::time::Duration;

use windows_sys::Win32::Foundation::{GlobalFree, HANDLE};
use windows_sys::Win32::Graphics::Gdi::{
    DeleteEnhMetaFile, GetEnhMetaFileBits, SetEnhMetaFileBits,
};
use windows_sys::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, EnumClipboardFormats, GetClipboardData,
    GetClipboardSequenceNumber, IsClipboardFormatAvailable, OpenClipboard,
    RegisterClipboardFormatW, SetClipboardData,
};
use windows_sys::Win32::System::Memory::{
    GMEM_MOVEABLE, GMEM_ZEROINIT, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock,
};
use windows_sys::Win32::System::Ole::{
    CF_BITMAP, CF_DSPBITMAP, CF_DSPENHMETAFILE, CF_DSPMETAFILEPICT, CF_ENHMETAFILE, CF_GDIOBJFIRST,
    CF_GDIOBJLAST, CF_METAFILEPICT, CF_OWNERDISPLAY, CF_PALETTE, CF_PRIVATEFIRST, CF_PRIVATELAST,
    CF_UNICODETEXT,
};

use crate::insert::{ChangeStamp, Previous};

/// Keeps every format out of clipboard history and the cloud clipboard; password managers place it beside
/// a secret. Its data means nothing.
const EXCLUDE_FROM_MONITORING: &str = "ExcludeClipboardContentFromMonitorProcessing";
/// A 32-bit value: 0 keeps the contents out of clipboard history.
const HISTORY: &str = "CanIncludeInClipboardHistory";
/// A 32-bit value: 0 keeps the contents from reaching the user's other devices.
const CLOUD: &str = "CanUploadToCloudClipboard";

/// Another program may hold the clipboard open for a moment, as clipboard history does right after each
/// change, so opening it is tried this many times.
const OPEN_TRIES: u32 = 10;
const OPEN_RETRY_AFTER: Duration = Duration::from_millis(20);

/// Each format with its data, in the clipboard's order.
pub(crate) struct Snapshot {
    formats: Vec<(u32, Data)>,
}

enum Data {
    Memory(Vec<u8>),
    EnhancedMetafile(Vec<u8>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Memory,
    EnhancedMetafile,
    LeftOut,
}

fn kind(format: u32) -> Kind {
    let Ok(format) = u16::try_from(format) else {
        return Kind::Memory;
    };
    match format {
        CF_ENHMETAFILE => Kind::EnhancedMetafile,
        CF_BITMAP | CF_PALETTE | CF_METAFILEPICT | CF_OWNERDISPLAY | CF_DSPBITMAP
        | CF_DSPMETAFILEPICT | CF_DSPENHMETAFILE => Kind::LeftOut,
        CF_PRIVATEFIRST..=CF_PRIVATELAST | CF_GDIOBJFIRST..=CF_GDIOBJLAST => Kind::LeftOut,
        _ => Kind::Memory,
    }
}

/// Unreadable when the clipboard cannot be opened, or holds formats of which none can be kept: written
/// back, it would come out empty.
pub(crate) fn read() -> Previous<Snapshot> {
    let Some(_open) = Open::new() else {
        return Previous::Unreadable;
    };
    if marked_private() {
        return Previous::MarkedPrivate;
    }
    let mut formats = Vec::new();
    let mut formats_seen = false;
    let mut format = 0;
    loop {
        // Safety: the clipboard is open; zero starts the enumeration and ends it.
        format = unsafe { EnumClipboardFormats(format) };
        if format == 0 {
            break;
        }
        formats_seen = true;
        if let Some(data) = read_format(format) {
            formats.push((format, data));
        }
    }
    if formats_seen && formats.is_empty() {
        Previous::Unreadable
    } else {
        Previous::Saved(Snapshot { formats })
    }
}

/// Whether a password manager or another program marked the contents as kept from clipboard history.
fn marked_private() -> bool {
    let excluded = registered(EXCLUDE_FROM_MONITORING);
    // Safety: the clipboard is open; any format number may be asked about.
    if excluded != 0 && unsafe { IsClipboardFormatAvailable(excluded) } != 0 {
        return true;
    }
    let history = registered(HISTORY);
    // Safety: as above.
    history != 0
        && unsafe { IsClipboardFormatAvailable(history) } != 0
        && read_memory(history).is_some_and(|bytes| bytes.starts_with(&0u32.to_ne_bytes()))
}

fn read_format(format: u32) -> Option<Data> {
    match kind(format) {
        Kind::Memory => read_memory(format).map(Data::Memory),
        Kind::EnhancedMetafile => read_metafile().map(Data::EnhancedMetafile),
        Kind::LeftOut => None,
    }
}

/// A format's block of memory. An empty or unreadable block is left out: it is not a block of memory, or
/// holds nothing to put back.
fn read_memory(format: u32) -> Option<Vec<u8>> {
    // Safety: the clipboard is open; the handle stays the clipboard's and is only read, between lock and
    // unlock, within the size the system reports for it.
    unsafe {
        let handle = GetClipboardData(format);
        if handle.is_null() {
            return None;
        }
        let size = GlobalSize(handle);
        if size == 0 {
            return None;
        }
        let data = GlobalLock(handle);
        if data.is_null() {
            return None;
        }
        let bytes = std::slice::from_raw_parts(data.cast::<u8>(), size).to_vec();
        GlobalUnlock(handle);
        Some(bytes)
    }
}

fn read_metafile() -> Option<Vec<u8>> {
    // Safety: the clipboard is open and keeps the metafile; its bits are copied into a buffer of the size
    // the first call reports.
    unsafe {
        let metafile = GetClipboardData(u32::from(CF_ENHMETAFILE));
        if metafile.is_null() {
            return None;
        }
        let size = GetEnhMetaFileBits(metafile, 0, null_mut());
        if size == 0 {
            return None;
        }
        let mut bytes = vec![0u8; size as usize];
        (GetEnhMetaFileBits(metafile, size, bytes.as_mut_ptr()) == size).then_some(bytes)
    }
}

/// `transient` keeps the text out of clipboard history and the cloud clipboard, since it is there only for
/// the paste.
pub(crate) fn write_text(text: &str, transient: bool) -> Option<ChangeStamp> {
    let open = Open::new()?;
    // Safety: the clipboard is open.
    if unsafe { EmptyClipboard() } == 0 {
        return None;
    }
    let utf16: Vec<u8> = text
        .encode_utf16()
        .chain([0])
        .flat_map(u16::to_ne_bytes)
        .collect();
    if !set_memory(u32::from(CF_UNICODETEXT), &utf16) {
        return None;
    }
    if transient {
        set_memory(registered(EXCLUDE_FROM_MONITORING), &0u32.to_ne_bytes());
        set_memory(registered(HISTORY), &0u32.to_ne_bytes());
        set_memory(registered(CLOUD), &0u32.to_ne_bytes());
    }
    drop(open);
    Some(stamp())
}

/// Written back as read, with no marker of its own, so a later read sees the same contents. A format the
/// clipboard refuses is left out.
pub(crate) fn write_snapshot(snapshot: Snapshot) -> Option<ChangeStamp> {
    let open = Open::new()?;
    // Safety: the clipboard is open.
    if unsafe { EmptyClipboard() } == 0 {
        return None;
    }
    for (format, data) in &snapshot.formats {
        match data {
            Data::Memory(bytes) => set_memory(*format, bytes),
            Data::EnhancedMetafile(bytes) => set_metafile(bytes),
        };
    }
    drop(open);
    Some(stamp())
}

/// Changes with every write to the clipboard, by any program.
pub(crate) fn stamp() -> ChangeStamp {
    // Safety: takes nothing and only reads.
    unsafe { GetClipboardSequenceNumber() as ChangeStamp }
}

/// Puts a copy of the bytes on the open clipboard, which owns it from then on.
fn set_memory(format: u32, bytes: &[u8]) -> bool {
    if format == 0 {
        return false;
    }
    // Safety: the block is allocated with room for the bytes and written between lock and unlock; once
    // the clipboard takes it, the system frees it, and until then it is freed here.
    unsafe {
        let handle = GlobalAlloc(GMEM_MOVEABLE | GMEM_ZEROINIT, bytes.len().max(1));
        if handle.is_null() {
            return false;
        }
        let data = GlobalLock(handle);
        if data.is_null() {
            GlobalFree(handle);
            return false;
        }
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), data.cast::<u8>(), bytes.len());
        GlobalUnlock(handle);
        if SetClipboardData(format, handle).is_null() {
            GlobalFree(handle);
            return false;
        }
    }
    true
}

fn set_metafile(bytes: &[u8]) -> bool {
    let Ok(size) = u32::try_from(bytes.len()) else {
        return false;
    };
    // Safety: the metafile is made from the bytes; once the clipboard takes it, the system deletes it, and
    // until then it is deleted here.
    unsafe {
        let metafile = SetEnhMetaFileBits(size, bytes.as_ptr());
        if metafile.is_null() {
            return false;
        }
        if SetClipboardData(u32::from(CF_ENHMETAFILE), metafile as HANDLE).is_null() {
            DeleteEnhMetaFile(metafile);
            return false;
        }
    }
    true
}

/// The number of a format known by name, the same in every program; 0 when the system refuses it.
fn registered(name: &str) -> u32 {
    let wide: Vec<u16> = name.encode_utf16().chain([0]).collect();
    // Safety: a valid, NUL-terminated wide string.
    unsafe { RegisterClipboardFormatW(wide.as_ptr()) }
}

/// The clipboard, open for this thread until dropped.
struct Open;

impl Open {
    fn new() -> Option<Self> {
        for attempt in 0..OPEN_TRIES {
            if attempt > 0 {
                sleep(OPEN_RETRY_AFTER);
            }
            // Safety: opens the clipboard for this thread with no window as its owner; closed on drop.
            if unsafe { OpenClipboard(null_mut()) } != 0 {
                return Some(Self);
            }
        }
        None
    }
}

impl Drop for Open {
    fn drop(&mut self) {
        // Safety: this thread opened the clipboard and closes it once.
        unsafe { CloseClipboard() };
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Mutex, MutexGuard, PoisonError};

    use super::*;

    #[test]
    fn memory_formats_are_kept_and_handles_left_out() {
        for format in [
            CF_UNICODETEXT,
            windows_sys::Win32::System::Ole::CF_DIB,
            windows_sys::Win32::System::Ole::CF_HDROP,
        ] {
            assert_eq!(kind(u32::from(format)), Kind::Memory, "{format}");
        }
        assert_eq!(kind(0xC123), Kind::Memory, "a registered format");
        assert_eq!(kind(u32::from(CF_ENHMETAFILE)), Kind::EnhancedMetafile);
        for format in [
            CF_BITMAP,
            CF_PALETTE,
            CF_METAFILEPICT,
            CF_OWNERDISPLAY,
            0x200,
            0x2FF,
            0x300,
            0x3FF,
        ] {
            assert_eq!(kind(u32::from(format)), Kind::LeftOut, "{format}");
        }
    }

    /// The tests below use the system clipboard, which has no private copy on Windows, so they run only
    /// when asked: `cargo test -p skald-input -- --ignored`. Each takes the clipboard for itself and puts
    /// back what it held.
    struct Keep {
        snapshot: Option<Snapshot>,
        _alone: MutexGuard<'static, ()>,
    }

    impl Keep {
        fn new() -> Self {
            static CLIPBOARD: Mutex<()> = Mutex::new(());
            let alone = CLIPBOARD.lock().unwrap_or_else(PoisonError::into_inner);
            let snapshot = match read() {
                Previous::Saved(snapshot) => Some(snapshot),
                _ => None,
            };
            Self {
                snapshot,
                _alone: alone,
            }
        }
    }

    impl Drop for Keep {
        fn drop(&mut self) {
            if let Some(snapshot) = self.snapshot.take() {
                write_snapshot(snapshot);
            }
        }
    }

    fn utf16(text: &str) -> Vec<u8> {
        text.encode_utf16()
            .chain([0])
            .flat_map(u16::to_ne_bytes)
            .collect()
    }

    fn put(formats: &[(u32, &[u8])]) {
        let _open = Open::new().expect("clipboard open");
        // Safety: the clipboard is open.
        assert_ne!(unsafe { EmptyClipboard() }, 0);
        for (format, bytes) in formats {
            assert!(set_memory(*format, bytes), "{format}");
        }
    }

    /// Each format with the start of its bytes as long as written, since the system may hand back a larger
    /// block than was asked for.
    fn contents(formats: &[(u32, &[u8])]) -> Vec<(u32, Vec<u8>)> {
        let _open = Open::new().expect("clipboard open");
        formats
            .iter()
            .map(|(format, bytes)| {
                let read = read_memory(*format).unwrap_or_default();
                (*format, read.get(..bytes.len()).unwrap_or(&read).to_vec())
            })
            .collect()
    }

    fn kind_of(previous: &Previous<Snapshot>) -> &'static str {
        match previous {
            Previous::Saved(_) => "saved",
            Previous::Unreadable => "unreadable",
            Previous::MarkedPrivate => "marked private",
        }
    }

    #[test]
    #[ignore = "uses the system clipboard"]
    fn every_format_comes_back_unchanged() {
        let _keep = Keep::new();
        let html = registered("HTML Format");
        let private = registered("no.icebit.skald.test-private");
        let text = utf16("Hei på deg");
        let formats: [(u32, &[u8]); 3] = [
            (u32::from(CF_UNICODETEXT), text.as_slice()),
            (html, b"Version:0.9\r\n<b>Hei</b>".as_slice()),
            (private, [0, 159, 255, 1].as_slice()),
        ];
        put(&formats);
        let before = contents(&formats);

        let Previous::Saved(snapshot) = read() else {
            panic!("not saved");
        };
        write_text("dictated ", true).expect("text written");
        write_snapshot(snapshot).expect("snapshot written");

        assert_eq!(contents(&formats), before);
        assert_eq!(kind_of(&read()), "saved");
    }

    #[test]
    #[ignore = "uses the system clipboard"]
    fn contents_kept_from_history_are_marked_private() {
        let _keep = Keep::new();
        let text = utf16("hunter2");
        let unicode = u32::from(CF_UNICODETEXT);

        put(&[
            (unicode, text.as_slice()),
            (registered(EXCLUDE_FROM_MONITORING), [].as_slice()),
        ]);
        assert_eq!(kind_of(&read()), "marked private", "excluded");

        put(&[
            (unicode, text.as_slice()),
            (registered(HISTORY), 0u32.to_ne_bytes().as_slice()),
        ]);
        assert_eq!(kind_of(&read()), "marked private", "kept from history");

        put(&[
            (unicode, text.as_slice()),
            (registered(HISTORY), 1u32.to_ne_bytes().as_slice()),
        ]);
        assert_eq!(kind_of(&read()), "saved", "allowed in history");
    }

    #[test]
    #[ignore = "uses the system clipboard"]
    fn the_text_for_the_paste_alone_is_marked_private_and_kept_text_is_not() {
        let _keep = Keep::new();

        write_text("Hei ", true).expect("written");
        assert_eq!(kind_of(&read()), "marked private");
        write_text("Hei ", false).expect("written");
        assert_eq!(kind_of(&read()), "saved");
    }

    #[test]
    #[ignore = "uses the system clipboard"]
    fn the_text_is_unicode_text_and_each_write_changes_the_stamp() {
        let _keep = Keep::new();

        let first = write_text("Hei, dette er en prøve. ", false).expect("written");
        assert_eq!(stamp(), first);
        let expected = utf16("Hei, dette er en prøve. ");
        let unicode = u32::from(CF_UNICODETEXT);
        assert_eq!(
            contents(&[(unicode, expected.as_slice())]),
            [(unicode, expected.clone())]
        );
        let second = write_text("Hei ", false).expect("written");
        assert_ne!(first, second);
    }

    #[test]
    #[ignore = "uses the system clipboard"]
    fn an_empty_clipboard_is_restored_empty() {
        let _keep = Keep::new();
        put(&[]);

        let Previous::Saved(snapshot) = read() else {
            panic!("not saved");
        };
        assert!(snapshot.formats.is_empty());
        write_text("dictated ", true).expect("text written");
        write_snapshot(snapshot).expect("snapshot written");

        let _open = Open::new().expect("clipboard open");
        // Safety: the clipboard is open.
        assert_eq!(unsafe { EnumClipboardFormats(0) }, 0);
    }
}
