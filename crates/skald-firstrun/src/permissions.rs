//! Step two: what each permission is for, its state, and the button that gets it granted.

use crate::action::{Action, Permission};
use crate::button::ButtonSpec;
use crate::model::{Access, ListedAs, Permissions, Platform};

/// macOS may ask about the clipboard the first time Skald puts the previous one back (docs/design.md,
/// Insertion); the step says so before it happens.
pub const CLIPBOARD_NOTE: &str = "macOS kan spørre én gang om Skald får lese utklippstavlen. Skald leser \
     den bare for å legge tilbake det som lå der, etter at teksten er satt inn. Svarer du nei, blir \
     teksten liggende på utklippstavlen.";

/// What the step calls the program a program started from a terminal is listed as, when its name was not
/// found.
const UNNAMED_PROGRAM: &str = "terminalprogrammet";

/// The name the system's settings list Skald's permissions under.
fn listed_name(listed: &ListedAs) -> &str {
    match listed {
        ListedAs::Skald => "Skald",
        ListedAs::StartedFrom(Some(name)) => name,
        ListedAs::StartedFrom(None) => UNNAMED_PROGRAM,
    }
}

/// The window comes back to the front by itself once a permission is granted.
pub fn come_back_note(listed: &ListedAs) -> String {
    format!(
        "Når du har slått på {} i Systeminnstillinger, kommer dette vinduet fram igjen av seg selv.",
        listed_name(listed)
    )
}

/// What to turn on, when Skald was started from another program, as from a terminal: macOS lists that
/// program, not Skald.
pub fn started_from_note(listed: &ListedAs) -> Option<String> {
    match listed {
        ListedAs::Skald => None,
        ListedAs::StartedFrom(Some(name)) => Some(format!(
            "Skald er startet fra {name}, så macOS viser {name}, ikke Skald, under Tilgjengelighet og \
             Mikrofon. Slå på {name} der."
        )),
        ListedAs::StartedFrom(None) => Some(
            "Skald er startet fra et terminalprogram, så macOS viser terminalprogrammet, ikke Skald, \
             under Tilgjengelighet og Mikrofon. Slå på terminalprogrammet der."
                .to_owned(),
        ),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionRow {
    pub permission: Permission,
    pub name: &'static str,
    pub purpose: &'static str,
    pub granted: bool,
    pub state: &'static str,
    /// Refused or forbidden: shown as a warning.
    pub refused: bool,
    /// Where the system keeps the setting, for a permission only the user can turn on there.
    pub hint: Option<String>,
    pub button: Option<ButtonSpec>,
}

/// The permissions this platform asks for, in the order the step lists them.
pub fn rows(
    platform: Platform,
    permissions: &Permissions,
    listed: &ListedAs,
) -> Vec<PermissionRow> {
    let mut rows = vec![row(
        Permission::Microphone,
        permissions.microphone,
        platform,
        listed,
    )];
    if platform.asks_for_accessibility() {
        rows.push(row(
            Permission::Accessibility,
            permissions.accessibility,
            platform,
            listed,
        ));
    }
    rows
}

fn row(
    permission: Permission,
    access: Access,
    platform: Platform,
    listed: &ListedAs,
) -> PermissionRow {
    let (name, purpose) = match permission {
        Permission::Microphone => ("Mikrofon", "for å høre det du sier"),
        Permission::Accessibility => ("Tilgjengelighet", "for tasten og innsettingen"),
    };
    let settings = if platform == Platform::MacOs {
        "Åpne Systeminnstillinger"
    } else {
        "Åpne innstillingene"
    };
    let (state, button) = match access {
        Access::Granted => ("Gitt", None),
        Access::NotAsked => (
            "Ikke gitt ennå",
            Some(ButtonSpec::primary(
                "Gi tilgang",
                Action::RequestAccess(permission),
            )),
        ),
        Access::Denied => (
            "Avslått",
            Some(ButtonSpec::primary(
                settings,
                Action::OpenSettings(permission),
            )),
        ),
        Access::Restricted => ("Sperret av en regel på denne maskinen", None),
    };
    let hint = (access == Access::Denied && platform == Platform::MacOs).then(|| {
        format!(
            "Slå på {} under Personvern og sikkerhet › {name}.",
            listed_name(listed)
        )
    });
    PermissionRow {
        permission,
        name,
        purpose,
        granted: access == Access::Granted,
        state,
        refused: matches!(access, Access::Denied | Access::Restricted),
        hint,
        button,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(platform: Platform, microphone: Access, accessibility: Access) -> Vec<PermissionRow> {
        super::rows(
            platform,
            &Permissions {
                microphone,
                accessibility,
            },
            &ListedAs::Skald,
        )
    }

    #[test]
    fn macos_asks_for_the_microphone_and_accessibility() {
        let rows = rows(Platform::MacOs, Access::Granted, Access::NotAsked);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].name, "Mikrofon");
        assert!(rows[0].granted);
        assert_eq!(rows[0].state, "Gitt");
        assert_eq!(rows[0].button, None);

        assert_eq!(rows[1].name, "Tilgjengelighet");
        assert_eq!(rows[1].purpose, "for tasten og innsettingen");
        assert_eq!(
            rows[1].button,
            Some(ButtonSpec::primary(
                "Gi tilgang",
                Action::RequestAccess(Permission::Accessibility)
            ))
        );
    }

    #[test]
    fn a_refusal_opens_the_pane_where_it_is_undone() {
        let rows = rows(Platform::MacOs, Access::Denied, Access::Granted);
        assert_eq!(rows[0].state, "Avslått");
        assert!(rows[0].refused);
        assert_eq!(
            rows[0].button,
            Some(ButtonSpec::primary(
                "Åpne Systeminnstillinger",
                Action::OpenSettings(Permission::Microphone)
            ))
        );
        assert_eq!(
            rows[0].hint.as_deref(),
            Some("Slå på Skald under Personvern og sikkerhet › Mikrofon.")
        );
    }

    #[test]
    fn a_policy_leaves_nothing_to_press() {
        let rows = rows(Platform::MacOs, Access::Restricted, Access::Granted);
        assert_eq!(rows[0].state, "Sperret av en regel på denne maskinen");
        assert_eq!(rows[0].button, None);
        assert!(!rows[0].granted);
    }

    #[test]
    fn windows_and_linux_ask_for_the_microphone_only() {
        for platform in [Platform::Windows, Platform::Linux] {
            let rows = rows(platform, Access::Denied, Access::NotAsked);
            assert_eq!(rows.len(), 1);
            assert_eq!(
                rows[0].button.as_ref().unwrap().label,
                "Åpne innstillingene"
            );
            assert_eq!(rows[0].hint, None);
        }
    }

    #[test]
    fn a_skald_started_from_a_terminal_names_the_terminal_to_turn_on() {
        let herdr = ListedAs::StartedFrom(Some("Herdr Core".into()));
        let note = started_from_note(&herdr).unwrap();
        assert!(
            note.contains("viser Herdr Core, ikke Skald, under Tilgjengelighet og Mikrofon"),
            "{note}"
        );
        assert!(note.ends_with("Slå på Herdr Core der."), "{note}");
        assert!(come_back_note(&herdr).starts_with("Når du har slått på Herdr Core i"));
        let denied = super::rows(
            Platform::MacOs,
            &Permissions {
                microphone: Access::Denied,
                accessibility: Access::Denied,
            },
            &herdr,
        );
        assert_eq!(
            denied[1].hint.as_deref(),
            Some("Slå på Herdr Core under Personvern og sikkerhet › Tilgjengelighet.")
        );

        let unnamed = ListedAs::StartedFrom(None);
        let note = started_from_note(&unnamed).unwrap();
        assert!(note.contains("ikke Skald"), "{note}");
        assert!(note.ends_with("Slå på terminalprogrammet der."), "{note}");

        assert_eq!(started_from_note(&ListedAs::Skald), None);
        assert!(come_back_note(&ListedAs::Skald).starts_with("Når du har slått på Skald i"));
    }

    #[test]
    fn the_clipboard_note_uses_the_apps_own_words() {
        assert!(CLIPBOARD_NOTE.contains("satt inn"));
        assert!(!CLIPBOARD_NOTE.contains("innliming"));
    }
}
