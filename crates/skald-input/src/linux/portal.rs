//! The hold key and the hands-free chord from the GlobalShortcuts portal, where the desktop offers it:
//! Hyprland through xdg-desktop-portal-hyprland, KDE Plasma, GNOME. Skald registers a shortcut for each,
//! the desktop binds a key to it, and the portal reports the hold key going down and coming up and the
//! chord being pressed. Each reaches the listener as a key binding's `skald start`, `skald stop` and
//! `skald toggle` do. On a desktop without the portal nothing happens, and the bindings are the way in.

use std::sync::Once;
use std::thread;

use ashpd::desktop::CreateSessionOptions;
use ashpd::desktop::global_shortcuts::{BindShortcutsOptions, GlobalShortcuts, NewShortcut};
use futures_lite::StreamExt;

use super::listener::{press_hold_key, toggle_hands_free};

/// The name the desktop knows Skald by, the bundle identifier on macOS (D-34), so the shortcuts read
/// `app.icebit.skald:hold` and `app.icebit.skald:hands-free`.
const APP_ID: &str = "app.icebit.skald";

const HOLD: &str = "hold";
const HANDS_FREE: &str = "hands-free";

/// What the desktop lists the shortcuts as.
const HOLD_DESCRIPTION: &str = "Hold inne for å snakke";
const HANDS_FREE_DESCRIPTION: &str = "Start eller stopp et håndfritt opptak";

/// What a shortcut of Skald's did.
enum Pressed {
    Hold { down: bool },
    HandsFree,
}

static STARTED: Once = Once::new();

/// Registers the shortcuts on a thread of its own, which then listens for as long as the process runs.
/// Called again, it does nothing: one registration serves every listener the process starts.
pub(crate) fn start() {
    STARTED.call_once(|| {
        let _ = thread::Builder::new()
            .name("skald-shortcut".into())
            .spawn(|| {
                // A desktop without the portal, or without its GlobalShortcuts, has nothing to say here.
                let _ = futures_lite::future::block_on(listen());
            });
    });
}

async fn listen() -> ashpd::Result<()> {
    // Without the portal's registry of host programs, the shortcuts are registered under no name.
    if let Ok(app_id) = ashpd::AppID::try_from(APP_ID) {
        let _ = ashpd::register_host_app(app_id).await;
    }
    let portal = GlobalShortcuts::new().await?;
    // Listened for before the shortcuts exist, so a first press is not missed.
    let pressed =
        portal
            .receive_activated()
            .await?
            .filter_map(|activated| match activated.shortcut_id() {
                HOLD => Some(Pressed::Hold { down: true }),
                HANDS_FREE => Some(Pressed::HandsFree),
                _ => None,
            });
    let released = portal
        .receive_deactivated()
        .await?
        .filter(|deactivated| deactivated.shortcut_id() == HOLD)
        .map(|_| Pressed::Hold { down: false });
    let session = portal
        .create_session(CreateSessionOptions::default())
        .await?;
    portal
        .bind_shortcuts(
            &session,
            &[
                NewShortcut::new(HOLD, HOLD_DESCRIPTION),
                NewShortcut::new(HANDS_FREE, HANDS_FREE_DESCRIPTION),
            ],
            None,
            BindShortcutsOptions::default(),
        )
        .await?
        .response()?;
    let mut presses = pressed.or(released);
    while let Some(pressed) = presses.next().await {
        // With no listener running there is no take for the key to start or end.
        let _ = match pressed {
            Pressed::Hold { down } => press_hold_key(down),
            Pressed::HandsFree => toggle_hands_free(),
        };
    }
    Ok(())
}
