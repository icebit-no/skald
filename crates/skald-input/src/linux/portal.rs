//! The hold key from the GlobalShortcuts portal, where the desktop offers it: Hyprland through
//! xdg-desktop-portal-hyprland, KDE Plasma, GNOME. Skald registers one shortcut, the desktop binds a key
//! to it, and the portal reports that key going down and coming up. Each reaches the listener as a key
//! binding's `skald start` and `skald stop` do. On a desktop without the portal nothing happens, and the
//! binding is the way in.

use std::sync::Once;
use std::thread;

use ashpd::desktop::CreateSessionOptions;
use ashpd::desktop::global_shortcuts::{BindShortcutsOptions, GlobalShortcuts, NewShortcut};
use futures_lite::StreamExt;

use super::listener::press_hold_key;

/// The name the desktop knows Skald by, the bundle identifier on macOS (D-34), so the shortcut reads
/// `app.icebit.skald:hold`.
const APP_ID: &str = "app.icebit.skald";

const SHORTCUT: &str = "hold";

/// What the desktop lists the shortcut as.
const DESCRIPTION: &str = "Hold inne for å snakke";

static STARTED: Once = Once::new();

/// Registers the shortcut on a thread of its own, which then listens for as long as the process runs.
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
    // Without the portal's registry of host programs, the shortcut is registered under no name.
    if let Ok(app_id) = ashpd::AppID::try_from(APP_ID) {
        let _ = ashpd::register_host_app(app_id).await;
    }
    let portal = GlobalShortcuts::new().await?;
    // Listened for before the shortcut exists, so its first press is not missed.
    let pressed = portal
        .receive_activated()
        .await?
        .filter(|activated| activated.shortcut_id() == SHORTCUT)
        .map(|_| true);
    let released = portal
        .receive_deactivated()
        .await?
        .filter(|deactivated| deactivated.shortcut_id() == SHORTCUT)
        .map(|_| false);
    let session = portal
        .create_session(CreateSessionOptions::default())
        .await?;
    portal
        .bind_shortcuts(
            &session,
            &[NewShortcut::new(SHORTCUT, DESCRIPTION)],
            None,
            BindShortcutsOptions::default(),
        )
        .await?
        .response()?;
    let mut presses = pressed.or(released);
    while let Some(down) = presses.next().await {
        // With no listener running there is no take for the key to start or end.
        let _ = press_hold_key(down);
    }
    Ok(())
}
