//! Step three: the test dictation, what the user is asked to say and what Skald heard.

use skald_core::config::HoldKey;

use crate::action::Action;
use crate::button::ButtonSpec;
use crate::download::Download;
use crate::model::{FirstRunModel, Platform, TestDictation};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestView {
    /// «Hold høyre ⌘ og si "Hei, dette er en prøve"».
    pub instruction: String,
    /// Why the test cannot start yet.
    pub waiting_for: Option<&'static str>,
    pub status: Option<String>,
    /// The text of the latest take.
    pub heard: Option<String>,
    /// Shown as a warning.
    pub problem: Option<String>,
    pub button: Option<ButtonSpec>,
    pub done: bool,
}

impl TestView {
    pub fn of(model: &FirstRunModel) -> Self {
        let key = hold_key_name(&model.hold_key, model.platform);
        let blank = TestView {
            instruction: format!("Hold {key} og si \"Hei, dette er en prøve\""),
            waiting_for: None,
            status: None,
            heard: None,
            problem: None,
            button: None,
            done: false,
        };
        let model_ready = model.download == Download::Ready;
        let permissions = model.permissions_granted();
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
            TestDictation::Heard { text } => TestView {
                heard: Some(text.clone()),
                status: Some(again.to_owned()),
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

    fn ready() -> FirstRunModel {
        let card = ModelCard::of(skald_models::find("nb-whisper-large").unwrap()).unwrap();
        let mut model = FirstRunModel::new(card, Status::Ready);
        model.platform = Platform::MacOs;
        model.hold_key = HoldKey::RightCommand;
        model.microphone = Access::Granted;
        model.accessibility = Access::Granted;
        model
    }

    #[test]
    fn the_instruction_is_design_mds_with_the_key_in_use() {
        let mut model = ready();
        assert_eq!(
            TestView::of(&model).instruction,
            "Hold høyre ⌘ og si \"Hei, dette er en prøve\""
        );
        model.platform = Platform::Windows;
        model.hold_key = HoldKey::RightControl;
        assert_eq!(
            TestView::of(&model).instruction,
            "Hold høyre Ctrl og si \"Hei, dette er en prøve\""
        );
        model.platform = Platform::MacOs;
        assert!(TestView::of(&model).instruction.contains("høyre Control"));
    }

    #[test]
    fn the_test_waits_for_the_model_and_the_permissions() {
        let mut model = ready();
        model.download = Download::Stopped { received: 1 };
        model.microphone = Access::Denied;
        let view = TestView::of(&model);
        assert_eq!(
            view.waiting_for,
            Some("Prøven kan starte når modellen er lastet ned og tilgangene er gitt.")
        );
        assert_eq!(view.button, None);

        model.microphone = Access::Granted;
        assert_eq!(
            TestView::of(&model).waiting_for,
            Some("Prøven kan starte når modellen er lastet ned.")
        );
        model.download = Download::Ready;
        model.accessibility = Access::NotAsked;
        assert_eq!(
            TestView::of(&model).waiting_for,
            Some("Prøven kan starte når tilgangene er gitt.")
        );
    }

    #[test]
    fn a_ready_test_starts_on_the_users_word() {
        let view = TestView::of(&ready());
        assert_eq!(view.waiting_for, None);
        assert_eq!(
            view.button,
            Some(ButtonSpec::primary("Start prøven", Action::RunTest))
        );
    }

    #[test]
    fn each_stage_of_a_take_is_shown() {
        let mut model = ready();
        let status = |model: &FirstRunModel| TestView::of(model).status;
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
    fn what_skald_heard_is_shown_and_completes_the_step() {
        let mut model = ready();
        model.test = TestDictation::Heard {
            text: "Hei, dette er en prøve.".into(),
        };
        let view = TestView::of(&model);
        assert_eq!(view.heard.as_deref(), Some("Hei, dette er en prøve."));
        assert!(view.done);

        model.test = TestDictation::HeardNothing {
            microphone: "MacBook Pro-mikrofon".into(),
        };
        let view = TestView::of(&model);
        assert_eq!(
            view.problem.as_deref(),
            Some("Hørte ingenting · lyttet på MacBook Pro-mikrofon")
        );
        assert!(!view.done);

        model.test = TestDictation::Failed {
            reason: "Modellen kunne ikke lastes.".into(),
        };
        let view = TestView::of(&model);
        assert_eq!(view.problem.as_deref(), Some("Modellen kunne ikke lastes."));
        assert_eq!(view.button.unwrap().action, Action::RunTest);
    }
}
