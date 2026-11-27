//! The item as AppKit's `NSStatusItem`: the rune as a template image, a view over it that draws the
//! dot, and a menu built from [`TrayModel::menu`] each time it opens after a change.
//!
//! The way the item is put together, with every entry sending one action to a target object that
//! hands it on, follows the status item of Ferric's desktop app (MIT, Thomas Seljen Tvedt).

use std::cell::{Cell, RefCell};

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject, NSObjectProtocol, ProtocolObject, Sel};
use objc2::{
    AllocAnyThread, ClassType, DefinedClass, MainThreadMarker, MainThreadOnly, define_class,
    msg_send, sel,
};
use objc2_app_kit::{
    NSAppearance, NSAppearanceNameAqua, NSAppearanceNameDarkAqua, NSAutoresizingMaskOptions,
    NSBezierPath, NSBitmapImageRep, NSColor, NSControlStateValueOff, NSControlStateValueOn,
    NSImage, NSMenu, NSMenuDelegate, NSMenuItem, NSResponder, NSStatusBar, NSStatusItem,
    NSVariableStatusItemLength, NSView,
};
use objc2_foundation::{NSArray, NSData, NSInteger, NSPoint, NSRect, NSSize, NSString};

use crate::menu::Entry;
use crate::{Action, Activity, TrayError, TrayModel};

/// The rune of `assets/icon/` on an 18 pt square, at 1x and 2x.
const RUNE_1X: &[u8] = include_bytes!("../../../../assets/icon/menu-bar-icon.png");
const RUNE_2X: &[u8] = include_bytes!("../../../../assets/icon/menu-bar-icon@2x.png");
const RUNE_POINTS: f64 = 18.0;

/// The dot's centre and radius in the rune's square, measured from its top left: in the corner the
/// rune leaves empty, below its twigs and right of its stave, a point and a half clear of the lower
/// twig so the two stay apart at 1x.
const DOT_CENTRE: (f64, f64) = (15.25, 15.0);
const DOT_RADIUS: f64 = 2.75;

/// Where the item stands after the user moves it with ⌘ is kept under this name.
const AUTOSAVE_NAME: &str = "Skald";

pub(crate) struct StatusItem {
    item: Retained<NSStatusItem>,
    rune: Retained<NSImage>,
    dot: Retained<Dot>,
    target: Retained<Target>,
    shown: TrayModel,
}

impl StatusItem {
    pub(crate) fn new(
        model: &TrayModel,
        on_action: Box<dyn Fn(Action)>,
    ) -> Result<Self, TrayError> {
        let mtm = MainThreadMarker::new().ok_or(TrayError::NotMainThread)?;
        let target = Target::new(on_action, model.menu(), mtm);
        let menu = NSMenu::new(mtm);
        // Each entry says whether it can be chosen; AppKit would otherwise enable every entry whose
        // target answers its action, an unplugged microphone among them.
        menu.setAutoenablesItems(false);
        menu.setDelegate(Some(ProtocolObject::from_ref(&*target)));

        let item = NSStatusBar::systemStatusBar().statusItemWithLength(NSVariableStatusItemLength);
        item.setAutosaveName(Some(&NSString::from_str(AUTOSAVE_NAME)));
        item.setMenu(Some(&menu));
        let rune = rune();
        let dot = Dot::new(mtm);
        if let Some(button) = item.button(mtm) {
            button.setImage(Some(&rune));
            dot.setFrame(button.bounds());
            dot.setAutoresizingMask(
                NSAutoresizingMaskOptions::ViewWidthSizable
                    | NSAutoresizingMaskOptions::ViewHeightSizable,
            );
            button.addSubview(&dot);
        }
        let status = Self {
            item,
            rune,
            dot,
            target,
            shown: model.clone(),
        };
        status.show_activity(model.activity);
        Ok(status)
    }

    pub(crate) fn show(&mut self, model: &TrayModel) {
        if *model == self.shown {
            return;
        }
        if model.activity != self.shown.activity {
            self.show_activity(model.activity);
        }
        self.target.set_entries(model.menu());
        self.shown = model.clone();
    }

    fn show_activity(&self, activity: Activity) {
        self.dot.show(activity);
        self.rune
            .setAccessibilityDescription(Some(&NSString::from_str(spoken_name(activity))));
    }
}

impl Drop for StatusItem {
    fn drop(&mut self) {
        NSStatusBar::systemStatusBar().removeStatusItem(&self.item);
    }
}

/// What VoiceOver says for the item: the app's name and, during a take, the state as the overlay
/// names it (docs/design.md, Overlay).
fn spoken_name(activity: Activity) -> &'static str {
    match activity {
        Activity::Idle => "Skald",
        Activity::Recording => "Skald · Opptak",
        Activity::Transcribing => "Skald · Rister",
    }
}

/// The rune as a template image: the menu bar draws its shape in its own colour, light or dark, and
/// highlights it as it does its own.
fn rune() -> Retained<NSImage> {
    let size = NSSize::new(RUNE_POINTS, RUNE_POINTS);
    let image = NSImage::initWithSize(NSImage::alloc(), size);
    for png in [RUNE_1X, RUNE_2X] {
        let rep = NSBitmapImageRep::imageRepWithData(&NSData::with_bytes(png))
            .expect("the menu bar icon compiled in is a PNG");
        rep.setSize(size);
        image.addRepresentation(&rep);
    }
    image.setTemplate(true);
    image
}

/// The dot's colour as `0xRRGGBB`: ochre while recording and gold while transcribing (docs/design.md,
/// Identity), the darker of each pair on a light menu bar and the lighter on a dark one.
fn dot_rgb(activity: Activity, dark: bool) -> Option<u32> {
    let (on_light, on_dark) = match activity {
        Activity::Idle => return None,
        Activity::Recording => (0xc2412d, 0xe2664e),
        Activity::Transcribing => (0xd9a94e, 0xf0c674),
    };
    Some(if dark { on_dark } else { on_light })
}

fn srgb(rgb: u32) -> Retained<NSColor> {
    let channel = |shift: u32| f64::from((rgb >> shift) & 0xff) / 255.0;
    NSColor::colorWithSRGBRed_green_blue_alpha(channel(16), channel(8), channel(0), 1.0)
}

define_class!(
    /// Draws the dot over the rune. The rune stays a template image the menu bar tints, which a
    /// coloured dot inside it would undo.
    #[unsafe(super(NSView, NSResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "SkaldTrayDot"]
    #[ivars = Cell<Activity>]
    struct Dot;

    impl Dot {
        #[unsafe(method(isFlipped))]
        fn is_flipped(&self) -> bool {
            true
        }

        /// Clicks go through to the item's button.
        #[unsafe(method(hitTest:))]
        fn hit_test(&self, _point: NSPoint) -> *mut NSView {
            std::ptr::null_mut()
        }

        #[unsafe(method(drawRect:))]
        fn draw_rect(&self, _dirty: NSRect) {
            if let Some(rgb) = dot_rgb(self.ivars().get(), drawing_on_dark()) {
                srgb(rgb).setFill();
                NSBezierPath::bezierPathWithOvalInRect(self.dot_rect()).fill();
            }
        }
    }
);

impl Dot {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(Cell::new(Activity::Idle));
        // SAFETY: NSView's designated initialiser, on a view just allocated.
        unsafe { msg_send![super(this), initWithFrame: NSRect::ZERO] }
    }

    fn show(&self, activity: Activity) {
        self.ivars().set(activity);
        self.setHidden(activity == Activity::Idle);
        self.setNeedsDisplay(true);
    }

    /// The button centres the rune, so the rune's square is centred in this view, which covers the
    /// button.
    fn dot_rect(&self) -> NSRect {
        let bounds = self.bounds();
        let left = bounds.origin.x + (bounds.size.width - RUNE_POINTS) / 2.0;
        let top = bounds.origin.y + (bounds.size.height - RUNE_POINTS) / 2.0;
        let (x, y) = DOT_CENTRE;
        NSRect::new(
            NSPoint::new(left + x - DOT_RADIUS, top + y - DOT_RADIUS),
            NSSize::new(2.0 * DOT_RADIUS, 2.0 * DOT_RADIUS),
        )
    }
}

/// Whether the view being drawn sits on a dark menu bar.
fn drawing_on_dark() -> bool {
    // SAFETY: AppKit's appearance names, constant strings.
    let (aqua, dark_aqua) = unsafe { (NSAppearanceNameAqua, NSAppearanceNameDarkAqua) };
    NSAppearance::currentDrawingAppearance()
        .bestMatchFromAppearancesWithNames(&NSArray::from_slice(&[aqua, dark_aqua]))
        .is_some_and(|name| &*name == dark_aqua)
}

struct TargetIvars {
    on_action: Box<dyn Fn(Action)>,
    /// What the menu holds the next time it opens.
    entries: RefCell<Vec<Entry>>,
    /// The entries changed since the menu was last built.
    stale: Cell<bool>,
    /// The action of each entry that can be chosen, by its tag, as the menu was last built.
    actions: RefCell<Vec<Action>>,
}

define_class!(
    /// Builds the menu as it opens and hands on the action of the entry chosen.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "SkaldTrayTarget"]
    #[ivars = TargetIvars]
    struct Target;

    impl Target {
        #[unsafe(method(choose:))]
        fn choose(&self, sender: Option<&NSMenuItem>) {
            let action = sender
                .and_then(|item| usize::try_from(item.tag()).ok())
                .and_then(|tag| self.ivars().actions.borrow().get(tag).cloned());
            // No borrow is held here, so the handler may update the item.
            if let Some(action) = action {
                (self.ivars().on_action)(action);
            }
        }
    }

    unsafe impl NSObjectProtocol for Target {}

    unsafe impl NSMenuDelegate for Target {
        #[unsafe(method(menuNeedsUpdate:))]
        fn menu_needs_update(&self, menu: &NSMenu) {
            self.build_if_stale(menu, self.mtm());
        }
    }
);

impl Target {
    fn new(
        on_action: Box<dyn Fn(Action)>,
        entries: Vec<Entry>,
        mtm: MainThreadMarker,
    ) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(TargetIvars {
            on_action,
            entries: RefCell::new(entries),
            stale: Cell::new(true),
            actions: RefCell::new(Vec::new()),
        });
        // SAFETY: NSObject's init, on an object just allocated.
        unsafe { msg_send![super(this), init] }
    }

    fn set_entries(&self, entries: Vec<Entry>) {
        if *self.ivars().entries.borrow() != entries {
            self.ivars().entries.replace(entries);
            self.ivars().stale.set(true);
        }
    }

    fn build_if_stale(&self, menu: &NSMenu, mtm: MainThreadMarker) {
        if !self.ivars().stale.replace(false) {
            return;
        }
        menu.removeAllItems();
        let mut actions = Vec::new();
        for entry in self.ivars().entries.borrow().iter() {
            menu.addItem(&self.item(entry, &mut actions, mtm));
        }
        self.ivars().actions.replace(actions);
    }

    fn item(
        &self,
        entry: &Entry,
        actions: &mut Vec<Action>,
        mtm: MainThreadMarker,
    ) -> Retained<NSMenuItem> {
        match entry {
            Entry::Separator => NSMenuItem::separatorItem(mtm),
            Entry::Label {
                title,
                greyed: false,
            } if has_section_headers() => {
                NSMenuItem::sectionHeaderWithTitle(&NSString::from_str(title), mtm)
            }
            Entry::Label { title, .. } => {
                let item = titled(title, None, "", mtm);
                item.setEnabled(false);
                item
            }
            Entry::Choice(choice) => {
                let key = choice.shortcut.map(String::from).unwrap_or_default();
                let item = titled(&choice.title, Some(sel!(choose:)), &key, mtm);
                let target: &AnyObject = self;
                // SAFETY: the target answers `choose:`; the status item keeps it as long as the menu.
                unsafe { item.setTarget(Some(target)) };
                item.setTag(actions.len() as NSInteger);
                actions.push(choice.action.clone());
                item.setEnabled(choice.enabled);
                item.setState(if choice.checked {
                    NSControlStateValueOn
                } else {
                    NSControlStateValueOff
                });
                if choice.pinned {
                    item.setImage(pin().as_deref());
                }
                if let Some(tooltip) = &choice.tooltip {
                    item.setToolTip(Some(&NSString::from_str(tooltip)));
                }
                item
            }
            Entry::Submenu { title, entries } => {
                let item = titled(title, None, "", mtm);
                let submenu = NSMenu::initWithTitle(NSMenu::alloc(mtm), &NSString::from_str(title));
                submenu.setAutoenablesItems(false);
                for entry in entries {
                    submenu.addItem(&self.item(entry, actions, mtm));
                }
                item.setSubmenu(Some(&submenu));
                item.setEnabled(!entries.is_empty());
                item
            }
        }
    }
}

/// An entry with `key` as its key with ⌘.
fn titled(
    title: &str,
    action: Option<Sel>,
    key: &str,
    mtm: MainThreadMarker,
) -> Retained<NSMenuItem> {
    // SAFETY: an entry with an action is given a target that answers it.
    unsafe {
        NSMenuItem::initWithTitle_action_keyEquivalent(
            NSMenuItem::alloc(mtm),
            &NSString::from_str(title),
            action,
            &NSString::from_str(key),
        )
    }
}

/// Whether AppKit draws a heading line, which macOS has from 14 on. A disabled entry is greyed
/// whatever colour its title is given, so before 14 the heading is greyed like the hint.
fn has_section_headers() -> bool {
    NSMenuItem::class()
        .metaclass()
        .responds_to(sel!(sectionHeaderWithTitle:))
}

/// The system's pin symbol, which marks the microphone held ahead of the order.
fn pin() -> Option<Retained<NSImage>> {
    NSImage::imageWithSystemSymbolName_accessibilityDescription(
        &NSString::from_str("pin.fill"),
        Some(&NSString::from_str("Festet")),
    )
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use skald_core::config::{HoldKey, Language};

    use super::*;
    use crate::Microphone;

    #[test]
    fn the_rune_is_an_18_point_template_at_1x_and_2x() {
        let image = rune();
        assert!(image.isTemplate());
        assert_eq!(image.size(), NSSize::new(18.0, 18.0));
        let reps = image.representations();
        let widths: Vec<NSInteger> = (0..reps.count())
            .map(|i| reps.objectAtIndex(i).pixelsWide())
            .collect();
        assert_eq!(widths, [18, 36]);
    }

    #[test]
    fn the_dot_is_ochre_while_recording_and_gold_while_transcribing() {
        assert_eq!(dot_rgb(Activity::Idle, false), None);
        assert_eq!(dot_rgb(Activity::Idle, true), None);
        assert_eq!(dot_rgb(Activity::Recording, false), Some(0xc2412d));
        assert_eq!(dot_rgb(Activity::Recording, true), Some(0xe2664e));
        assert_eq!(dot_rgb(Activity::Transcribing, false), Some(0xd9a94e));
        assert_eq!(dot_rgb(Activity::Transcribing, true), Some(0xf0c674));
    }

    #[test]
    fn the_dot_stays_inside_the_runes_square() {
        let (x, y) = DOT_CENTRE;
        assert!(x - DOT_RADIUS >= 0.0 && x + DOT_RADIUS <= RUNE_POINTS);
        assert!(y - DOT_RADIUS >= 0.0 && y + DOT_RADIUS <= RUNE_POINTS);
    }

    fn sample() -> TrayModel {
        TrayModel {
            activity: Activity::Idle,
            hold_key: HoldKey::RightCommand,
            language: Language::Bokmal,
            microphones: vec![
                Microphone {
                    id: "usb".to_owned(),
                    name: "Shure MV7".to_owned(),
                    connected: true,
                },
                Microphone {
                    id: "airpods".to_owned(),
                    name: "AirPods Pro".to_owned(),
                    connected: false,
                },
            ],
            in_use: Some("usb".to_owned()),
            pinned: Some("airpods".to_owned()),
            latest_texts: vec!["Hei, dette er en prøve.".to_owned()],
        }
    }

    fn titles(menu: &NSMenu) -> Vec<String> {
        (0..menu.numberOfItems())
            .filter_map(|i| menu.itemAtIndex(i))
            .map(|item| item.title().to_string())
            .collect()
    }

    fn submenu(menu: &NSMenu, title: &str) -> Retained<NSMenu> {
        let index = menu.indexOfItemWithTitle(&NSString::from_str(title));
        menu.itemAtIndex(index)
            .and_then(|item| item.submenu())
            .unwrap_or_else(|| panic!("no submenu {title:?}"))
    }

    /// The menu AppKit gets holds the entries as the model has them, and choosing an entry, as the
    /// menu sends it, hands the app that entry's action.
    #[test]
    fn the_built_menu_holds_the_entries_and_hands_on_their_actions() {
        // SAFETY: the menu is built and messaged on this one thread and never shown; nothing reaches
        // the window server.
        let mtm = unsafe { MainThreadMarker::new_unchecked() };
        let chosen = Rc::new(RefCell::new(Vec::new()));
        let sink = chosen.clone();
        let target = Target::new(
            Box::new(move |action| sink.borrow_mut().push(action)),
            sample().menu(),
            mtm,
        );
        let menu = NSMenu::new(mtm);
        menu.setAutoenablesItems(false);
        target.build_if_stale(&menu, mtm);

        assert_eq!(
            titles(&menu),
            [
                "Skald · bokmål",
                "Hold høyre ⌘ for å snakke",
                "",
                "Mikrofon",
                "Siste tekster",
                "Skriftspråk",
                "",
                "Ordskatt …",
                "Innstillinger …",
                "Avslutt Skald",
            ]
        );
        let heading = menu.itemAtIndex(0).unwrap();
        assert!(heading.isSectionHeader());
        let hint = menu.itemAtIndex(1).unwrap();
        assert!(!hint.isEnabled() && !hint.isSectionHeader());
        let settings = menu.itemAtIndex(8).unwrap();
        assert_eq!(settings.keyEquivalent().to_string(), ",");
        let quit = menu.itemAtIndex(9).unwrap();
        assert_eq!(quit.keyEquivalent().to_string(), "q");

        let microphones = submenu(&menu, "Mikrofon");
        assert_eq!(
            titles(&microphones),
            [
                "Shure MV7",
                "AirPods Pro",
                "",
                "Bruk den første som er koblet til"
            ]
        );
        let shure = microphones.itemAtIndex(0).unwrap();
        assert!(shure.isEnabled() && shure.state() == NSControlStateValueOn);
        assert!(shure.image().is_none());
        let airpods = microphones.itemAtIndex(1).unwrap();
        assert!(!airpods.isEnabled() && airpods.state() == NSControlStateValueOff);
        assert!(airpods.image().is_some());

        let languages = submenu(&menu, "Skriftspråk");
        let nynorsk = languages.itemAtIndex(1).unwrap();
        let texts = submenu(&menu, "Siste tekster");
        let text = texts.itemAtIndex(0).unwrap();
        let first_connected = microphones.itemAtIndex(3).unwrap();
        for item in [&nynorsk, &text, &first_connected, &quit] {
            // SAFETY: the target answers choose: with one menu item.
            let _: () = unsafe { msg_send![&*target, choose: &**item] };
        }
        assert_eq!(
            *chosen.borrow(),
            [
                Action::SetLanguage(Language::Nynorsk),
                Action::CopyText("Hei, dette er en prøve.".to_owned()),
                Action::UnpinMicrophone,
                Action::Quit,
            ]
        );
    }

    #[test]
    fn the_menu_is_built_again_only_after_its_entries_change() {
        // SAFETY: as above; the menu is never shown.
        let mtm = unsafe { MainThreadMarker::new_unchecked() };
        let mut model = sample();
        let target = Target::new(Box::new(|_| {}), model.menu(), mtm);
        let menu = NSMenu::new(mtm);
        target.build_if_stale(&menu, mtm);
        let first = menu.itemAtIndex(0).unwrap();

        model.activity = Activity::Recording;
        target.set_entries(model.menu());
        target.build_if_stale(&menu, mtm);
        assert!(std::ptr::eq(&*first, &*menu.itemAtIndex(0).unwrap()));

        model.language = Language::Nynorsk;
        target.set_entries(model.menu());
        target.build_if_stale(&menu, mtm);
        let rebuilt = menu.itemAtIndex(0).unwrap();
        assert!(!std::ptr::eq(&*first, &*rebuilt));
        assert_eq!(rebuilt.title().to_string(), "Skald · nynorsk");
    }
}
