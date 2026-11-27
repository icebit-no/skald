//! The three steps of docs/design.md, First run: which is done, where the window opens, and how the user
//! moves between them.

use crate::download::Download;
use crate::model::FirstRunModel;

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

    pub fn is_done(self, model: &FirstRunModel) -> bool {
        match self {
            Step::Model => model.download == Download::Ready,
            Step::Permissions => model.permissions_granted(),
            Step::Test => matches!(model.test, crate::model::TestDictation::Heard { .. }),
        }
    }

    /// The first step not done yet; the test when the other two are.
    pub fn first_open(model: &FirstRunModel) -> Step {
        [Step::Model, Step::Permissions]
            .into_iter()
            .find(|step| !step.is_done(model))
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
/// permissions can be granted meanwhile; «Ferdig» waits until Skald can take dictation. The test is not
/// required: someone setting Skald up again knows it works.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Footer {
    pub back: bool,
    pub next: bool,
    /// `Some(enabled)` on the last step.
    pub finish: Option<bool>,
    /// The download's progress while another step shows, so it is not forgotten.
    pub download_note: Option<String>,
}

impl Footer {
    pub fn of(step: Step, model: &FirstRunModel) -> Self {
        let download_note = (step != Step::Model)
            .then(|| match &model.download {
                Download::Starting { .. } | Download::Running { .. } => Some(format!(
                    "Laster ned modellen · {}",
                    crate::format::percent(model.download.received(), model.model.size)
                )),
                Download::Failed { .. } => Some("Nedlastingen stoppet med en feil.".to_owned()),
                _ => None,
            })
            .flatten();
        Footer {
            back: step.previous().is_some(),
            next: step.next().is_some(),
            finish: (step == Step::Test).then(|| model.ready()),
            download_note,
        }
    }
}

#[cfg(test)]
mod tests {
    use skald_models::Status;

    use super::*;
    use crate::download::Throughput;
    use crate::model::{Access, ModelCard, Platform, TestDictation};

    fn model(status: Status, microphone: Access, accessibility: Access) -> FirstRunModel {
        let card = ModelCard::of(skald_models::find("nb-whisper-large").unwrap()).unwrap();
        let mut model = FirstRunModel::new(card, status);
        model.platform = Platform::MacOs;
        model.microphone = microphone;
        model.accessibility = accessibility;
        model
    }

    #[test]
    fn the_window_opens_on_the_first_step_not_done() {
        assert_eq!(
            Step::first_open(&model(Status::Missing, Access::Granted, Access::Granted)),
            Step::Model
        );
        assert_eq!(
            Step::first_open(&model(Status::Ready, Access::Granted, Access::NotAsked)),
            Step::Permissions
        );
        assert_eq!(
            Step::first_open(&model(Status::Ready, Access::Granted, Access::Granted)),
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
        let mut model = model(Status::Ready, Access::Granted, Access::Granted);
        assert!(!Step::Test.is_done(&model));
        model.test = TestDictation::HeardNothing {
            microphone: "Shure MV7".into(),
        };
        assert!(!Step::Test.is_done(&model));
        model.test = TestDictation::Heard {
            text: "Hei.".into(),
        };
        assert!(Step::Test.is_done(&model));
    }

    #[test]
    fn finishing_waits_for_the_model_and_the_permissions_not_the_test() {
        let mut model = model(
            Status::Partial { received: 5 },
            Access::Granted,
            Access::Granted,
        );
        assert_eq!(Footer::of(Step::Test, &model).finish, Some(false));
        model.download = Download::Ready;
        assert_eq!(Footer::of(Step::Test, &model).finish, Some(true));
        assert_eq!(Footer::of(Step::Permissions, &model).finish, None);
    }

    #[test]
    fn a_running_download_is_named_on_the_other_steps() {
        let mut model = model(Status::Missing, Access::NotAsked, Access::NotAsked);
        model.download = Download::Running {
            received: model.model.size / 4,
            throughput: Throughput::default(),
            retry: None,
            stopping: false,
        };
        let footer = Footer::of(Step::Permissions, &model);
        assert_eq!(
            footer.download_note.as_deref(),
            Some("Laster ned modellen · 25 %")
        );
        assert!(footer.back && footer.next);
        assert_eq!(Footer::of(Step::Model, &model).download_note, None);
    }
}
