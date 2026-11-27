//! Step three: the test dictation. The take is inserted into a text field in the window, so the whole way
//! from the key to the inserted text is tried; what Skald heard is shown beside it when the text did not
//! reach the field.

use skald_core::config::HoldKey;

use crate::action::Action;
use crate::button::ButtonSpec;
use crate::download::Download;
use crate::model::{FirstRunModel, Permissions, Platform, TestDictation};

pub const FIELD_PLACEHOLDER: &str = "Teksten settes inn her";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestView {
    /// «Hold høyre ⌘ og si "Hei, dette er en prøve"».
    pub instruction: String,
    /// Why the test cannot start yet.
    pub waiting_for: Option<&'static str>,
    /// The field the take is inserted into shows once the test can run.
    pub field: bool,
    pub status: Option<String>,
    /// The take's text, shown when it did not reach the field.
    pub heard: Option<String>,
    /// Shown as a warning.
    pub problem: Option<String>,
    pub button: Option<ButtonSpec>,
    pub done: bool,
}

impl TestView {
    /// `field` is what the text field holds.
    pub fn of(model: &FirstRunModel, permissions: &Permissions, field: &str) -> Self {
        let key = hold_key_name(&model.hold_key, model.platform);
        let blank = TestView {
            instruction: format!("Hold {key} og si \"Hei, dette er en prøve\""),
            waiting_for: None,
            field: false,
            status: None,
            heard: None,
            problem: None,
            button: None,
            done: false,
        };
        let model_ready = model.download == Download::Ready;
        let permissions = permissions.granted(model.platform);
        if !model_ready || !permissions {
            return TestView {
                waiting_for: Some(match (model_ready, permissions) {
                    (false, false) => {
                        "Prøven kan starte når modellen er lastet ned og tilgangene er gitt."
                    }
                    (false, true) => "Prøven kan starte når modellen er lastet ned.",
                    _ => "Prøven kan starte når tilgangene er gitt.",
                }),
                ..blank
            };
        }
        let blank = TestView {
            field: true,
            ..blank
        };
        let again = "Hold tasten igjen for å prøve en gang til.";
        match &model.test {
            TestDictation::NotStarted => TestView {
                button: Some(ButtonSpec::primary("Start prøven", Action::RunTest)),
                ..blank
            },
            TestDictation::Loading => TestView {
                status: Some("Laster modellen …".to_owned()),
                ..blank
            },
            TestDictation::Listening => TestView {
                status: Some(format!("Klar. Hold {key} når du vil snakke.")),
                ..blank
            },
            TestDictation::Recording => TestView {
                status: Some("Tar opp. Slipp tasten når du er ferdig.".to_owned()),
                ..blank
            },
            TestDictation::Transcribing => TestView {
                status: Some("Gjenkjenner talen …".to_owned()),
                ..blank
            },
            TestDictation::Heard { text } if reached(field, text) => TestView {
                status: Some(format!("Satt inn. {again}")),
                done: true,
                ..blank
            },
            TestDictation::Heard { text } => TestView {
                heard: Some(text.clone()),
                problem: Some(
                    "Teksten kom ikke inn i feltet. Klikk i feltet og prøv igjen.".to_owned(),
                ),
                done: true,
                ..blank
            },
            TestDictation::HeardNothing { microphone } => TestView {
                problem: Some(format!("Hørte ingenting · lyttet på {microphone}")),
                status: Some(again.to_owned()),
                ..blank
            },
            TestDictation::Failed { reason } => TestView {
                problem: Some(reason.clone()),
                button: Some(ButtonSpec::primary("Start prøven igjen", Action::RunTest)),
                ..blank
            },
        }
    }
}

/// Whether the field holds the take's text. Skald inserts it with a trailing space, and the user may
/// have typed around it.
fn reached(field: &str, text: &str) -> bool {
    let text = text.trim();
    !text.is_empty() && field.contains(text)
}

/// The key as the instruction names it, in the words of the settings window's choice.
pub fn hold_key_name(key: &HoldKey, platform: Platform) -> &'static str {
    match (key, platform) {
        (HoldKey::RightCommand, _) => "høyre ⌘",
        (HoldKey::RightControl, Platform::MacOs) => "høyre Control",
        (HoldKey::RightControl, _) => "høyre Ctrl",
        // A key a newer Skald wrote: the platform's own.
        (HoldKey::Unknown(_), Platform::MacOs) => "høyre ⌘",
        (HoldKey::Unknown(_), _) => "høyre Ctrl",
    }
}

#[cfg(test)]
mod tests {
    use skald_models::Status;

    use super::*;
    use crate::model::{Access, ModelCard};

    const GRANTED: Permissions = Permissions {
        microphone: Access::Granted,
        accessibility: Access::Granted,
    };

    fn ready() -> FirstRunModel {
        let card = ModelCard::of(skald_models::find("nb-whisper-large").unwrap()).unwrap();
        let mut model = FirstRunModel::new(card, Status::Ready);
        model.platform = Platform::MacOs;
        model.hold_key = HoldKey::RightCommand;
        model
    }

    fn view(model: &FirstRunModel) -> TestView {
        TestView::of(model, &GRANTED, "")
    }

    #[test]
    fn the_instruction_is_design_mds_with_the_key_in_use() {
        let mut model = ready();
        assert_eq!(
            view(&model).instruction,
            "Hold høyre ⌘ og si \"Hei, dette er en prøve\""
        );
        model.platform = Platform::Windows;
        model.hold_key = HoldKey::RightControl;
        assert_eq!(
            view(&model).instruction,
            "Hold høyre Ctrl og si \"Hei, dette er en prøve\""
        );
        model.platform = Platform::MacOs;
        assert!(view(&model).instruction.contains("høyre Control"));
    }

    #[test]
    fn the_test_waits_for_the_model_and_the_permissions() {
        let mut model = ready();
        model.download = Download::Stopped { received: 1 };
        let mut permissions = GRANTED;
        permissions.microphone = Access::Denied;
        let waiting = TestView::of(&model, &permissions, "");
        assert_eq!(
            waiting.waiting_for,
            Some("Prøven kan starte når modellen er lastet ned og tilgangene er gitt.")
        );
        assert_eq!(waiting.button, None);
        assert!(!waiting.field, "nothing can be inserted yet");

        assert_eq!(
            TestView::of(&model, &GRANTED, "").waiting_for,
            Some("Prøven kan starte når modellen er lastet ned.")
        );
        model.download = Download::Ready;
        permissions.microphone = Access::Granted;
        permissions.accessibility = Access::NotAsked;
        assert_eq!(
            TestView::of(&model, &permissions, "").waiting_for,
            Some("Prøven kan starte når tilgangene er gitt.")
        );
    }

    #[test]
    fn a_ready_test_shows_the_field_and_starts_on_the_users_word() {
        let ready = view(&ready());
        assert_eq!(ready.waiting_for, None);
        assert!(ready.field);
        assert_eq!(
            ready.button,
            Some(ButtonSpec::primary("Start prøven", Action::RunTest))
        );
    }

    #[test]
    fn each_stage_of_a_take_is_shown() {
        let mut model = ready();
        let status = |model: &FirstRunModel| view(model).status;
        model.test = TestDictation::Loading;
        assert_eq!(status(&model).as_deref(), Some("Laster modellen …"));
        model.test = TestDictation::Listening;
        assert_eq!(
            status(&model).as_deref(),
            Some("Klar. Hold høyre ⌘ når du vil snakke.")
        );
        model.test = TestDictation::Recording;
        assert_eq!(
            status(&model).as_deref(),
            Some("Tar opp. Slipp tasten når du er ferdig.")
        );
        model.test = TestDictation::Transcribing;
        assert_eq!(status(&model).as_deref(), Some("Gjenkjenner talen …"));
    }

    #[test]
    fn a_take_that_reached_the_field_completes_the_step() {
        let mut model = ready();
        model.test = TestDictation::Heard {
            text: "Hei, dette er en prøve.".into(),
        };
        let inserted = TestView::of(&model, &GRANTED, "Hei, dette er en prøve. ");
        assert!(inserted.done);
        assert_eq!(inserted.heard, None);
        assert_eq!(inserted.problem, None);
        assert_eq!(
            inserted.status.as_deref(),
            Some("Satt inn. Hold tasten igjen for å prøve en gang til.")
        );
        assert!(
            TestView::of(&model, &GRANTED, "Først: Hei, dette er en prøve. ").done,
            "the user may have typed around it"
        );
    }

    #[test]
    fn a_take_that_missed_the_field_shows_what_skald_heard() {
        let mut model = ready();
        model.test = TestDictation::Heard {
            text: "Hei, dette er en prøve.".into(),
        };
        let missed = TestView::of(&model, &GRANTED, "");
        assert_eq!(missed.heard.as_deref(), Some("Hei, dette er en prøve."));
        assert_eq!(
            missed.problem.as_deref(),
            Some("Teksten kom ikke inn i feltet. Klikk i feltet og prøv igjen.")
        );
        assert!(missed.done, "Skald heard it; only the insertion missed");
    }

    #[test]
    fn nothing_heard_and_a_failure_are_shown() {
        let mut model = ready();
        model.test = TestDictation::HeardNothing {
            microphone: "MacBook Pro-mikrofon".into(),
        };
        let nothing = view(&model);
        assert_eq!(
            nothing.problem.as_deref(),
            Some("Hørte ingenting · lyttet på MacBook Pro-mikrofon")
        );
        assert!(!nothing.done);

        model.test = TestDictation::Failed {
            reason: "Modellen kunne ikke lastes.".into(),
        };
        let failed = view(&model);
        assert_eq!(
            failed.problem.as_deref(),
            Some("Modellen kunne ikke lastes.")
        );
        assert_eq!(failed.button.unwrap().action, Action::RunTest);
    }
}
