//! A pasteboard's contents, every item with every type it holds, read and written back unchanged.

use objc2::rc::{Retained, autoreleasepool};
use objc2::runtime::{NSObjectProtocol, ProtocolObject};
use objc2::sel;
use objc2_app_kit::{
    NSPasteboard, NSPasteboardAccessBehavior, NSPasteboardItem, NSPasteboardType,
    NSPasteboardTypeString, NSPasteboardWriting,
};
use objc2_foundation::{NSArray, NSData, NSString};

use crate::insert::{ChangeStamp, Previous};

/// Tells clipboard history tools not to record the content (nspasteboard.org).
const TRANSIENT_TYPE: &str = "org.nspasteboard.TransientType";
/// Marks a password or another secret (nspasteboard.org).
const CONCEALED_TYPE: &str = "org.nspasteboard.ConcealedType";

/// Each item's types with their data, in the pasteboard's order.
pub(crate) struct Snapshot {
    items: Vec<Vec<(Retained<NSPasteboardType>, Retained<NSData>)>>,
}

/// Unreadable when the pasteboard holds types whose data cannot be read: written back, it would come out
/// empty.
pub(crate) fn read(pasteboard: &NSPasteboard) -> Previous<Snapshot> {
    autoreleasepool(|_| {
        if reading_denied(pasteboard) {
            return Previous::Unreadable;
        }
        let pasteboard_items = pasteboard
            .pasteboardItems()
            .map(|items| items.to_vec())
            .unwrap_or_default();
        let marked_private = pasteboard_items.iter().any(|item| {
            item.types().iter().any(|kind| {
                let kind = kind.to_string();
                kind == CONCEALED_TYPE || kind == TRANSIENT_TYPE
            })
        });
        if marked_private {
            return Previous::MarkedPrivate;
        }
        let mut items = Vec::new();
        let mut types_seen = false;
        for item in &pasteboard_items {
            let types = item.types();
            types_seen |= !types.is_empty();
            let readable: Vec<_> = types
                .iter()
                .filter_map(|kind| item.dataForType(&kind).map(|data| (kind, data)))
                .collect();
            if !readable.is_empty() {
                items.push(readable);
            }
        }
        if types_seen && items.is_empty() {
            Previous::Unreadable
        } else {
            Previous::Saved(Snapshot { items })
        }
    })
}

/// Whether the user has told macOS never to let this app read the pasteboard. The setting exists from
/// macOS 15.4.
fn reading_denied(pasteboard: &NSPasteboard) -> bool {
    pasteboard.respondsToSelector(sel!(accessBehavior))
        && pasteboard.accessBehavior() == NSPasteboardAccessBehavior::AlwaysDeny
}

pub(crate) fn write_text(
    pasteboard: &NSPasteboard,
    text: &str,
    transient: bool,
) -> Option<ChangeStamp> {
    autoreleasepool(|_| {
        let item = NSPasteboardItem::new();
        // Safety: a constant AppKit defines and never changes.
        let string_type = unsafe { NSPasteboardTypeString };
        if !item.setString_forType(&NSString::from_str(text), string_type) {
            return None;
        }
        if transient {
            mark_transient(&item);
        }
        write(pasteboard, vec![item])
    })
}

/// Written back exactly as read, with no marker of its own, so a later read sees the same contents.
pub(crate) fn write_snapshot(pasteboard: &NSPasteboard, snapshot: Snapshot) -> Option<ChangeStamp> {
    autoreleasepool(|_| {
        let items = snapshot
            .items
            .into_iter()
            .map(|types| {
                let item = NSPasteboardItem::new();
                for (kind, data) in types {
                    item.setData_forType(&data, &kind);
                }
                item
            })
            .collect();
        write(pasteboard, items)
    })
}

fn mark_transient(item: &NSPasteboardItem) {
    item.setData_forType(&NSData::new(), &NSString::from_str(TRANSIENT_TYPE));
}

/// Replaces the pasteboard's contents. The stamp comes from the clearing, so a write by another app in
/// between can never be taken for ours.
fn write(pasteboard: &NSPasteboard, items: Vec<Retained<NSPasteboardItem>>) -> Option<ChangeStamp> {
    let stamp = pasteboard.clearContents();
    if items.is_empty() {
        return Some(stamp);
    }
    let objects: Vec<Retained<ProtocolObject<dyn NSPasteboardWriting>>> = items
        .into_iter()
        .map(ProtocolObject::from_retained)
        .collect();
    pasteboard
        .writeObjects(&NSArray::from_retained_slice(&objects))
        .then_some(stamp)
}

/// Run against a pasteboard of their own, never the clipboard.
#[cfg(test)]
mod tests {
    use super::*;

    const RTF_TYPE: &str = "public.rtf";
    const PRIVATE_TYPE: &str = "no.icebit.skald.test-private";

    struct Scratch(Retained<NSPasteboard>);

    impl Scratch {
        fn new() -> Self {
            Self(NSPasteboard::pasteboardWithUniqueName())
        }

        fn put(&self, items: &[&[(&str, &[u8])]]) {
            let items = items
                .iter()
                .map(|types| {
                    let item = NSPasteboardItem::new();
                    for (kind, bytes) in *types {
                        item.setData_forType(&NSData::with_bytes(bytes), &NSString::from_str(kind));
                    }
                    item
                })
                .collect();
            write(&self.0, items).expect("scratch pasteboard write");
        }

        /// Every item's types with their bytes.
        fn contents(&self) -> Vec<Vec<(String, Vec<u8>)>> {
            let items = self
                .0
                .pasteboardItems()
                .map(|items| items.to_vec())
                .unwrap_or_default();
            items
                .iter()
                .map(|item| {
                    item.types()
                        .iter()
                        .map(|kind| {
                            let bytes = item
                                .dataForType(&kind)
                                .map(|d| d.to_vec())
                                .unwrap_or_default();
                            (kind.to_string(), bytes)
                        })
                        .collect()
                })
                .collect()
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            // Safety: releaseGlobally takes no arguments, returns nothing and is called once, as the
            // pasteboard goes out of use.
            unsafe { objc2::msg_send![&*self.0, releaseGlobally] }
        }
    }

    fn saved(previous: Previous<Snapshot>) -> Snapshot {
        match previous {
            Previous::Saved(snapshot) => snapshot,
            Previous::Unreadable => panic!("unreadable"),
            Previous::MarkedPrivate => panic!("marked private"),
        }
    }

    fn kind(previous: &Previous<Snapshot>) -> &'static str {
        match previous {
            Previous::Saved(_) => "saved",
            Previous::Unreadable => "unreadable",
            Previous::MarkedPrivate => "marked private",
        }
    }

    /// A type the pasteboard derives by conversion, such as UTF-16 text from RTF, is listed beside the
    /// ones written and comes back as data of its own, which a reader cannot tell apart.
    #[test]
    fn a_reader_sees_every_item_and_type_unchanged() {
        let scratch = Scratch::new();
        scratch.put(&[
            &[
                ("public.utf8-plain-text", "Hei på deg".as_bytes()),
                (RTF_TYPE, b"{\\rtf1 Hei}"),
                (PRIVATE_TYPE, &[0, 159, 255, 1]),
            ],
            &[("public.utf8-plain-text", b"second item")],
        ]);
        let before = scratch.contents();

        let snapshot = saved(read(&scratch.0));
        write_text(&scratch.0, "dictated ", true).expect("text written");
        write_snapshot(&scratch.0, snapshot).expect("snapshot written");

        assert_eq!(scratch.contents(), before);
        assert_eq!(kind(&read(&scratch.0)), "saved");
    }

    #[test]
    fn contents_marked_concealed_or_transient_are_marked_private() {
        for marker in [CONCEALED_TYPE, TRANSIENT_TYPE] {
            let scratch = Scratch::new();
            scratch.put(&[
                &[("public.utf8-plain-text", b"ordinary")],
                &[("public.utf8-plain-text", b"hunter2"), (marker, b"")],
            ]);

            assert_eq!(kind(&read(&scratch.0)), "marked private", "{marker}");
        }
    }

    #[test]
    fn the_text_for_the_paste_alone_is_marked_private_and_kept_text_is_not() {
        let scratch = Scratch::new();

        write_text(&scratch.0, "Hei ", true).expect("written");
        assert_eq!(kind(&read(&scratch.0)), "marked private");
        write_text(&scratch.0, "Hei ", false).expect("written");
        assert_eq!(kind(&read(&scratch.0)), "saved");
    }

    #[test]
    fn an_empty_pasteboard_is_restored_empty() {
        let scratch = Scratch::new();
        write(&scratch.0, Vec::new()).expect("cleared");

        let snapshot = saved(read(&scratch.0));
        assert!(snapshot.items.is_empty());
        write_text(&scratch.0, "dictated ", true).expect("text written");
        write_snapshot(&scratch.0, snapshot).expect("snapshot written");

        assert_eq!(scratch.contents(), Vec::<Vec<(String, Vec<u8>)>>::new());
    }

    #[test]
    fn the_text_is_plain_text_and_transient_only_when_asked() {
        let scratch = Scratch::new();

        write_text(&scratch.0, "Hei, dette er en prøve. ", true).expect("written");
        assert_eq!(
            scratch.contents(),
            [vec![
                (
                    "public.utf8-plain-text".to_string(),
                    "Hei, dette er en prøve. ".as_bytes().to_vec()
                ),
                (TRANSIENT_TYPE.to_string(), Vec::new()),
            ]]
        );

        write_text(&scratch.0, "Hei ", false).expect("written");
        assert_eq!(
            scratch.contents(),
            [vec![(
                "public.utf8-plain-text".to_string(),
                b"Hei ".to_vec()
            )]]
        );
    }

    #[test]
    fn each_write_has_a_stamp_of_its_own_that_the_pasteboard_reports() {
        let scratch = Scratch::new();

        let first = write_text(&scratch.0, "a", true).expect("written");
        assert_eq!(scratch.0.changeCount(), first);
        let second = write_text(&scratch.0, "b", true).expect("written");
        assert_ne!(first, second);
        assert_eq!(scratch.0.changeCount(), second);
    }
}
