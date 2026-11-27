//! Step two: what each permission is for, its state, and the button that gets it granted.

use crate::action::{Action, Permission};
use crate::button::ButtonSpec;
use crate::model::{Access, FirstRunModel, Platform};

/// macOS may ask about the clipboard the first time Skald puts the previous one back (docs/design.md,
/// Insertion); the step says so before it happens.
pub const CLIPBOARD_NOTE: &str = "macOS kan spørre én gang om Skald får lese utklippstavlen. Skald leser \
     den bare for å legge tilbake det som lå der, etter at teksten er satt inn. Svarer du nei, blir \
     teksten liggende på utklippstavlen.";

/// The window reads the permissions afresh each time it comes to the front.
pub const COME_BACK_NOTE: &str = "Når du har slått på Skald i Systeminnstillinger, kan du komme tilbake hit. Skald ser endringen selv.";

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
pub fn rows(model: &FirstRunModel) -> Vec<PermissionRow> {
    let mut rows = vec![row(
        Permission::Microphone,
        model.microphone,
        model.platform,
    )];
    if model.platform.asks_for_accessibility() {
        rows.push(row(
            Permission::Accessibility,
            model.accessibility,
            model.platform,
        ));
    }
    rows
}

fn row(permission: Permission, access: Access, platform: Platform) -> PermissionRow {
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
    let hint = (access == Access::Denied && platform == Platform::MacOs)
        .then(|| format!("Slå på Skald under Personvern og sikkerhet › {name}."));
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
    use skald_models::Status;

    use super::*;
    use crate::model::ModelCard;

    fn model(platform: Platform, microphone: Access, accessibility: Access) -> FirstRunModel {
        let card = ModelCard::of(skald_models::find("nb-whisper-large").unwrap()).unwrap();
        let mut model = FirstRunModel::new(card, Status::Missing);
        model.platform = platform;
        model.microphone = microphone;
        model.accessibility = accessibility;
        model
    }

    #[test]
    fn macos_asks_for_the_microphone_and_accessibility() {
        let rows = rows(&model(Platform::MacOs, Access::Granted, Access::NotAsked));
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
        let rows = rows(&model(Platform::MacOs, Access::Denied, Access::Granted));
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
        let rows = rows(&model(Platform::MacOs, Access::Restricted, Access::Granted));
        assert_eq!(rows[0].state, "Sperret av en regel på denne maskinen");
        assert_eq!(rows[0].button, None);
        assert!(!rows[0].granted);
    }

    #[test]
    fn windows_and_linux_ask_for_the_microphone_only() {
        for platform in [Platform::Windows, Platform::Linux] {
            let rows = rows(&model(platform, Access::Denied, Access::NotAsked));
            assert_eq!(rows.len(), 1);
            assert_eq!(
                rows[0].button.as_ref().unwrap().label,
                "Åpne innstillingene"
            );
            assert_eq!(rows[0].hint, None);
        }
    }

    #[test]
    fn the_clipboard_note_uses_the_apps_own_words() {
        assert!(CLIPBOARD_NOTE.contains("satt inn"));
        assert!(!CLIPBOARD_NOTE.contains("innliming"));
    }
}
