//! The desktop's accent colour and its wish for reduced motion, from the Settings portal, which GNOME, KDE
//! Plasma and the other portals' desktops answer for. A thread of its own reads both once and then follows
//! their changes, so the overlay reads the last answer without waiting; only the first read waits a moment
//! for the portal's first answer. A desktop without the portal, or without these settings, leaves the theme
//! its own colour and full motion.

use std::sync::mpsc;
use std::sync::{Mutex, Once, PoisonError};
use std::thread;
use std::time::Duration;

use ashpd::desktop::Color;
use ashpd::desktop::settings::{ReducedMotion, Settings};
use futures_lite::StreamExt;
use gpui_kit::Rgba;

/// The longest the first read waits for the portal's first answer.
const FIRST_ANSWER_WITHIN: Duration = Duration::from_millis(150);

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Appearance {
    pub(crate) accent: Option<Rgba>,
    pub(crate) reduced_motion: bool,
}

static LAST: Mutex<Appearance> = Mutex::new(Appearance {
    accent: None,
    reduced_motion: false,
});

static WATCHING: Once = Once::new();

/// The appearance as the portal last told it.
pub(crate) fn current() -> Appearance {
    WATCHING.call_once(|| {
        let (answered, first_answer) = mpsc::channel();
        let started = thread::Builder::new()
            .name("skald-appearance".into())
            .spawn(move || {
                // A desktop without the portal has nothing to tell.
                let _ = futures_lite::future::block_on(watch(answered));
            });
        if started.is_ok() {
            let _ = first_answer.recv_timeout(FIRST_ANSWER_WITHIN);
        }
    });
    *LAST.lock().unwrap_or_else(PoisonError::into_inner)
}

fn update(change: impl FnOnce(&mut Appearance)) {
    change(&mut LAST.lock().unwrap_or_else(PoisonError::into_inner));
}

enum Change {
    Accent(Color),
    Motion(ReducedMotion),
}

async fn watch(answered: mpsc::Sender<()>) -> ashpd::Result<()> {
    let settings = Settings::new().await?;
    // Listened for before the first read, so a change in between is not lost.
    let accents = settings.receive_accent_color_changed().await?;
    let motions = settings.receive_reduced_motion_changed().await?;
    if let Ok(color) = settings.accent_color().await {
        update(|appearance| appearance.accent = accent(color));
    }
    if let Ok(motion) = settings.reduced_motion().await {
        update(|appearance| appearance.reduced_motion = reduced(motion));
    }
    let _ = answered.send(());
    let mut changes = accents.map(Change::Accent).or(motions.map(Change::Motion));
    while let Some(change) = changes.next().await {
        match change {
            Change::Accent(color) => update(|appearance| appearance.accent = accent(color)),
            Change::Motion(motion) => {
                update(|appearance| appearance.reduced_motion = reduced(motion));
            }
        }
    }
    Ok(())
}

/// The portal's colour, each part from 0 to 1; one outside that range means the user chose none.
fn accent(color: Color) -> Option<Rgba> {
    let parts = [color.red(), color.green(), color.blue()];
    parts
        .iter()
        .all(|part| (0.0..=1.0).contains(part))
        .then(|| Rgba {
            r: parts[0] as f32,
            g: parts[1] as f32,
            b: parts[2] as f32,
            a: 1.0,
        })
}

fn reduced(motion: ReducedMotion) -> bool {
    motion == ReducedMotion::ReducedMotion
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_colour_outside_the_range_is_no_choice() {
        assert_eq!(
            accent(Color::new(0.2, 0.4, 1.0)),
            Some(Rgba {
                r: 0.2,
                g: 0.4,
                b: 1.0,
                a: 1.0
            })
        );
        assert_eq!(accent(Color::new(-1.0, -1.0, -1.0)), None);
        assert_eq!(accent(Color::new(0.5, 1.5, 0.5)), None);
    }

    #[test]
    fn only_a_wish_for_reduced_motion_reduces_it() {
        assert!(reduced(ReducedMotion::ReducedMotion));
        assert!(!reduced(ReducedMotion::NoPreference));
    }
}
