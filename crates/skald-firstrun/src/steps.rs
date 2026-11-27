//! The three steps of docs/design.md, First run: which is done, where the window opens, and how the user
//! moves between them.

use crate::download::Download;
use crate::format;
use crate::model::{FirstRunModel, Permissions, TestDictation};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Model,
    Permissions,
    Test,
}

impl Step {
    pub const ALL: [Step; 3] = [Step::Model, Step::Permissions, Step::Test];

    pub fn label(self) -> &'static str {
        match self {
            Step::Model => "Talemodell",
            Step::Permissions => "Tilganger",
            Step::Test => "Prøv",
        }
    }

    pub fn number(self) -> usize {
        match self {
            Step::Model => 1,
            Step::Permissions => 2,
            Step::Test => 3,
        }
    }

    pub fn is_done(self, model: &FirstRunModel, permissions: &Permissions) -> bool {
        match self {
            Step::Model => model.download == Download::Ready,
            Step::Permissions => permissions.granted(model.platform),
            Step::Test => matches!(model.test, TestDictation::Heard { .. }),
        }
    }

    /// The first step not done yet; the test when the other two are.
    pub fn first_open(model: &FirstRunModel, permissions: &Permissions) -> Step {
        [Step::Model, Step::Permissions]
            .into_iter()
            .find(|step| !step.is_done(model, permissions))
            .unwrap_or(Step::Test)
    }

    pub fn next(self) -> Option<Step> {
        match self {
            Step::Model => Some(Step::Permissions),
            Step::Permissions => Some(Step::Test),
            Step::Test => None,
        }
    }

    pub fn previous(self) -> Option<Step> {
        match self {
            Step::Model => None,
            Step::Permissions => Some(Step::Model),
            Step::Test => Some(Step::Permissions),
        }
    }
}

/// The buttons along the bottom of a step. The user may go on while the model downloads, so the
/// permissions can be granted meanwhile. The last step ends with «Ferdig» once the test heard something
/// and «Hopp over» before; either waits until Skald can take dictation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Footer {
    pub back: bool,
    pub next: bool,
    /// On the last step only.
    pub finish: Option<Finish>,
    /// The download's progress while another step shows, so it is not forgotten.
    pub download_note: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Finish {
    pub label: &'static str,
    pub primary: bool,
    pub enabled: bool,
}

impl Footer {
    pub fn of(step: Step, model: &FirstRunModel, permissions: &Permissions) -> Self {
        let download_note = match (&model.download, step) {
            (_, Step::Model) => None,
            (Download::Starting { .. } | Download::Running { .. }, _) => Some(format!(
                "Laster ned modellen · {}",
                format::percent(model.download.received(), model.model.size)
            )),
            (Download::Failed { .. }, _) => Some("Nedlastingen stoppet med en feil.".to_owned()),
            _ => None,
        };
        let tested = Step::Test.is_done(model, permissions);
        Footer {
            back: step.previous().is_some(),
            next: step.next().is_some(),
            finish: (step == Step::Test).then(|| Finish {
                label: if tested { "Ferdig" } else { "Hopp over" },
                primary: tested,
                enabled: model.ready(permissions),
            }),
            download_note,
        }
    }
}

#[cfg(test)]
mod tests {
    use skald_models::Status;

    use super::*;
    use crate::download::Throughput;
    use crate::model::{Access, ModelCard, Platform};

    const GRANTED: Permissions = Permissions {
        microphone: Access::Granted,
        accessibility: Access::Granted,
    };
    const ACCESSIBILITY_MISSING: Permissions = Permissions {
        microphone: Access::Granted,
        accessibility: Access::NotAsked,
    };

    fn model(status: Status) -> FirstRunModel {
        let card = ModelCard::of(skald_models::find("nb-whisper-large").unwrap()).unwrap();
        let mut model = FirstRunModel::new(card, status);
        model.platform = Platform::MacOs;
        model
    }

    #[test]
    fn the_window_opens_on_the_first_step_not_done() {
        assert_eq!(
            Step::first_open(&model(Status::Missing), &GRANTED),
            Step::Model
        );
        assert_eq!(
            Step::first_open(&model(Status::Ready), &ACCESSIBILITY_MISSING),
            Step::Permissions
        );
        assert_eq!(
            Step::first_open(&model(Status::Ready), &GRANTED),
            Step::Test
        );
    }

    #[test]
    fn the_steps_are_numbered_and_named_in_order() {
        let labels: Vec<_> = Step::ALL
            .iter()
            .map(|step| (step.number(), step.label()))
            .collect();
        assert_eq!(labels, [(1, "Talemodell"), (2, "Tilganger"), (3, "Prøv")]);
        assert_eq!(Step::Model.next(), Some(Step::Permissions));
        assert_eq!(Step::Test.next(), None);
        assert_eq!(Step::Model.previous(), None);
    }

    #[test]
    fn the_test_step_is_done_once_skald_heard_something() {
        let mut model = model(Status::Ready);
        assert!(!Step::Test.is_done(&model, &GRANTED));
        model.test = TestDictation::HeardNothing {
            microphone: "Shure MV7".into(),
        };
        assert!(!Step::Test.is_done(&model, &GRANTED));
        model.test = TestDictation::Heard {
            text: "Hei.".into(),
        };
        assert!(Step::Test.is_done(&model, &GRANTED));
    }

    #[test]
    fn the_test_can_be_skipped_once_skald_can_take_dictation() {
        let mut model = model(Status::Partial { received: 5 });
        let finish = |model: &FirstRunModel, permissions| {
            Footer::of(Step::Test, model, permissions).finish.unwrap()
        };
        assert_eq!(
            finish(&model, &GRANTED),
            Finish {
                label: "Hopp over",
                primary: false,
                enabled: false
            }
        );
        model.download = Download::Ready;
        assert!(finish(&model, &GRANTED).enabled);
        assert!(!finish(&model, &ACCESSIBILITY_MISSING).enabled);

        model.test = TestDictation::Heard {
            text: "Hei.".into(),
        };
        assert_eq!(
            finish(&model, &GRANTED),
            Finish {
                label: "Ferdig",
                primary: true,
                enabled: true
            }
        );
        assert_eq!(Footer::of(Step::Permissions, &model, &GRANTED).finish, None);
    }

    #[test]
    fn a_running_download_is_named_on_the_other_steps() {
        let mut model = model(Status::Missing);
        model.download = Download::Running {
            received: model.model.size / 4,
            throughput: Throughput::default(),
            retry: None,
            stopping: false,
        };
        let footer = Footer::of(Step::Permissions, &model, &ACCESSIBILITY_MISSING);
        assert_eq!(
            footer.download_note.as_deref(),
            Some("Laster ned modellen · 25 %")
        );
        assert!(footer.back && footer.next);
        assert_eq!(
            Footer::of(Step::Model, &model, &ACCESSIBILITY_MISSING).download_note,
            None
        );
    }
}
