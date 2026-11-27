//! The item as a StatusNotifierItem, which Waybar and the other panels on Linux show: the rune in a light
//! colour, for the dark panels Hyprland desktops have, with the dot in its corner, and the menu over D-Bus
//! (dbusmenu). ksni speaks both protocols on a thread of its own, so each entry chosen reaches the app from
//! there; the item follows the model it was last given.

use std::io::Cursor;
use std::sync::Arc;

use ksni::blocking::{Handle, TrayMethods};
use ksni::menu::{CheckmarkItem, StandardItem, SubMenu};
use ksni::{Icon, MenuItem, ToolTip};

use crate::menu::{Choice, Entry};
use crate::{Action, Activity, TrayError, TrayModel};

const RUNE_1X: &[u8] = include_bytes!("../../../../assets/icon/menu-bar-icon.png");
const RUNE_2X: &[u8] = include_bytes!("../../../../assets/icon/menu-bar-icon@2x.png");

/// The rune's side at 1x, in pixels; the dot is placed in that square as on macOS.
#[cfg(test)]
const RUNE_SIDE: f32 = 18.0;
const DOT_CENTRE: (f32, f32) = (15.25, 15.0);
const DOT_RADIUS: f32 = 2.75;

/// The identity's bone, for the rune on a dark panel (docs/design.md, Identity).
const BONE: u32 = 0xede4d0;
/// The identity's lighter ochre and gold, for the dot on a dark panel.
const OCHRE: u32 = 0xe2664e;
const GOLD: u32 = 0xf0c674;

/// What the item tells a screen reader, and shows when the pointer rests on it (docs/design.md, Menu).
fn title(activity: Activity) -> &'static str {
    match activity {
        Activity::Idle => "Skald",
        Activity::Recording => "Skald · Opptak",
        Activity::Transcribing => "Skald · Rister",
    }
}

pub(crate) struct StatusItem {
    handle: Handle<Item>,
}

impl StatusItem {
    pub(crate) fn new(
        model: &TrayModel,
        on_action: Box<dyn Fn(Action) + Send + Sync>,
    ) -> Result<Self, TrayError> {
        let item = Item {
            model: model.clone(),
            on_action: Arc::from(on_action),
        };
        let handle = item
            .spawn()
            .map_err(|error| TrayError::NoService(error.to_string()))?;
        Ok(Self { handle })
    }

    pub(crate) fn show(&mut self, model: &TrayModel) {
        self.handle.update(|item| {
            if item.model != *model {
                item.model = model.clone();
            }
        });
    }
}

impl Drop for StatusItem {
    fn drop(&mut self) {
        self.handle.shutdown().wait();
    }
}

struct Item {
    model: TrayModel,
    on_action: Arc<dyn Fn(Action) + Send + Sync>,
}

impl ksni::Tray for Item {
    // A click on the rune opens the menu, as on macOS.
    const MENU_ON_ACTIVATE: bool = true;

    fn id(&self) -> String {
        "skald".into()
    }

    fn title(&self) -> String {
        title(self.model.activity).into()
    }

    fn icon_pixmap(&self) -> Vec<Icon> {
        [(RUNE_1X, 1.0), (RUNE_2X, 2.0)]
            .into_iter()
            .filter_map(|(png, scale)| pixmap(png, scale, self.model.activity))
            .collect()
    }

    fn tool_tip(&self) -> ToolTip {
        ToolTip {
            title: title(self.model.activity).into(),
            ..ToolTip::default()
        }
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        self.model.menu().into_iter().map(menu_item).collect()
    }
}

fn menu_item(entry: Entry) -> MenuItem<Item> {
    match entry {
        Entry::Label { title, greyed } => StandardItem {
            label: label(&title),
            enabled: !greyed,
            ..StandardItem::default()
        }
        .into(),
        Entry::Choice(choice) => choice_item(choice),
        Entry::Submenu { title, entries } => SubMenu {
            label: label(&title),
            enabled: !entries.is_empty(),
            submenu: entries.into_iter().map(menu_item).collect(),
            ..SubMenu::default()
        }
        .into(),
        Entry::Separator => MenuItem::Separator,
    }
}

/// A choice with a tick where the menu has a column of them (the microphones and the written standard),
/// otherwise a plain entry. dbusmenu has no pin, so the pinned microphone says so in words.
fn choice_item(choice: Choice) -> MenuItem<Item> {
    let mut title = label(&choice.title);
    if choice.pinned {
        title.push_str(" · festet");
    }
    let action = choice.action;
    let checkable = matches!(
        action,
        Action::PinMicrophone(_) | Action::UnpinMicrophone | Action::SetLanguage(_)
    );
    let activate = Box::new(move |item: &mut Item| (item.on_action)(action.clone()));
    if checkable {
        CheckmarkItem {
            label: title,
            enabled: choice.enabled,
            checked: choice.checked,
            activate,
            ..CheckmarkItem::default()
        }
        .into()
    } else {
        StandardItem {
            label: title,
            enabled: choice.enabled,
            activate,
            ..StandardItem::default()
        }
        .into()
    }
}

/// dbusmenu reads an underscore as the mark of an access key, so one written in a name is doubled.
fn label(title: &str) -> String {
    title.replace('_', "__")
}

/// The rune at `scale` in bone, with the activity's dot, as ARGB32 in network byte order.
fn pixmap(png: &[u8], scale: f32, activity: Activity) -> Option<Icon> {
    let mut decoder = png::Decoder::new(Cursor::new(png));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().ok()?;
    let mut buffer = vec![0; reader.output_buffer_size()?];
    let frame = reader.next_frame(&mut buffer).ok()?;
    let channels = frame.color_type.samples();
    let (width, height) = (frame.width as usize, frame.height as usize);
    let dot = match activity {
        Activity::Idle => None,
        Activity::Recording => Some(OCHRE),
        Activity::Transcribing => Some(GOLD),
    };
    let mut data = Vec::with_capacity(width * height * 4);
    for y in 0..height {
        for x in 0..width {
            // The template's shape is its alpha; its colour is the panel's to choose, here bone.
            let pixel = &buffer[(y * width + x) * channels..][..channels];
            let rune = pixel[channels - 1];
            let (centre_x, centre_y) = (DOT_CENTRE.0 * scale, DOT_CENTRE.1 * scale);
            let distance =
                ((x as f32 + 0.5 - centre_x).powi(2) + (y as f32 + 0.5 - centre_y).powi(2)).sqrt();
            let cover = (DOT_RADIUS * scale + 0.5 - distance).clamp(0.0, 1.0);
            let (rgb, alpha) = match dot {
                Some(rgb) if cover > 0.0 => (rgb, (cover * 255.0) as u8),
                _ => (BONE, rune),
            };
            data.extend_from_slice(&[alpha, (rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8]);
        }
    }
    Some(Icon {
        width: i32::try_from(width).ok()?,
        height: i32::try_from(height).ok()?,
        data,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Microphone;

    #[test]
    fn the_rune_comes_at_18_and_36_pixels_with_the_dot_while_a_take_runs() {
        for (png, side) in [(RUNE_1X, 18), (RUNE_2X, 36)] {
            let scale = side as f32 / RUNE_SIDE;
            let idle = pixmap(png, scale, Activity::Idle).unwrap();
            assert_eq!((idle.width, idle.height), (side, side));
            assert_eq!(idle.data.len(), (side * side * 4) as usize);
            let painted: Vec<&[u8]> = idle.data.chunks(4).filter(|p| p[0] > 0).collect();
            assert!(!painted.is_empty(), "the rune is drawn");
            assert!(painted.iter().all(|p| p[1..] == [0xed, 0xe4, 0xd0]));

            // The dot's centre takes the activity's colour.
            let centre = ((DOT_CENTRE.1 * scale) as usize * side as usize
                + (DOT_CENTRE.0 * scale) as usize)
                * 4;
            let recording = pixmap(png, scale, Activity::Recording).unwrap();
            assert_eq!(recording.data[centre..centre + 4], [255, 0xe2, 0x66, 0x4e]);
            let carving = pixmap(png, scale, Activity::Transcribing).unwrap();
            assert_eq!(carving.data[centre..centre + 4], [255, 0xf0, 0xc6, 0x74]);
        }
    }

    #[test]
    fn the_menu_keeps_its_entries_with_ticks_where_they_choose_one_of_many() {
        let model = TrayModel {
            microphones: vec![Microphone {
                id: "usb".into(),
                name: "Shure_MV7".into(),
                connected: true,
            }],
            in_use: Some("usb".into()),
            pinned: Some("usb".into()),
            ..TrayModel::default()
        };
        let menu: Vec<MenuItem<Item>> = model.menu().into_iter().map(menu_item).collect();
        assert_eq!(menu.len(), model.menu().len());
        let MenuItem::SubMenu(microphones) = &menu[3] else {
            panic!("the microphones' submenu");
        };
        let MenuItem::Checkmark(first) = &microphones.submenu[0] else {
            panic!("a microphone has a tick");
        };
        assert_eq!(first.label, "Shure__MV7 · festet");
        assert!(first.checked);
        assert!(
            matches!(menu.last(), Some(MenuItem::Standard(quit)) if quit.label == "Avslutt Skald")
        );
    }

    #[test]
    fn the_title_says_what_a_take_is_doing() {
        assert_eq!(title(Activity::Idle), "Skald");
        assert_eq!(title(Activity::Recording), "Skald · Opptak");
        assert_eq!(title(Activity::Transcribing), "Skald · Rister");
    }
}
