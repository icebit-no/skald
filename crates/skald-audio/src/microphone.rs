//! Which microphone a take uses: the user's order matched against what is connected.

use std::fmt;

/// A microphone's identity as the system reports it, in the form the configuration stores. It stays
/// the same across restarts and reconnections where the system allows.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MicId(String);

impl MicId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for MicId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Microphone {
    pub id: MicId,
    /// The system's name for it, which the user sees.
    pub name: String,
}

/// The microphones connected right now, in the system's own order, and the system's default input.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Connected {
    pub microphones: Vec<Microphone>,
    pub default: Option<MicId>,
}

/// The microphone a take uses: the first entry of `order` that is connected, else the system's
/// default input, else the first connected microphone.
///
/// An entry is connected when a microphone with its id is. Failing that, it matches by name a
/// connected microphone whose id no entry holds, since a device's id can change when it is plugged
/// into another port.
pub fn choose<'a>(order: &[Microphone], connected: &'a Connected) -> Option<&'a Microphone> {
    let by_id = |id: &MicId| connected.microphones.iter().find(|m| &m.id == id);
    let unclaimed_by_name = |name: &str| {
        connected
            .microphones
            .iter()
            .find(|m| m.name == name && !order.iter().any(|entry| entry.id == m.id))
    };

    order
        .iter()
        .find_map(|entry| by_id(&entry.id).or_else(|| unclaimed_by_name(&entry.name)))
        .or_else(|| connected.default.as_ref().and_then(by_id))
        .or_else(|| connected.microphones.first())
}

/// The active microphone changed: one was connected or disconnected, or the order changed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MicrophoneChange {
    pub from: Option<Microphone>,
    /// `None` when no microphone is connected.
    pub to: Option<Microphone>,
}

/// Holds the order and the last known connected microphones, and reports when the choice moves to
/// another microphone.
#[derive(Debug)]
pub(crate) struct Selector {
    order: Vec<Microphone>,
    connected: Connected,
    active: Option<Microphone>,
}

impl Selector {
    pub(crate) fn new(order: Vec<Microphone>, connected: Connected) -> Self {
        let active = choose(&order, &connected).cloned();
        Self {
            order,
            connected,
            active,
        }
    }

    pub(crate) fn active(&self) -> Option<&Microphone> {
        self.active.as_ref()
    }

    pub(crate) fn update(&mut self, connected: Connected) -> Option<MicrophoneChange> {
        self.connected = connected;
        self.choose_again()
    }

    pub(crate) fn set_order(&mut self, order: Vec<Microphone>) -> Option<MicrophoneChange> {
        self.order = order;
        self.choose_again()
    }

    fn choose_again(&mut self) -> Option<MicrophoneChange> {
        let chosen = choose(&self.order, &self.connected).cloned();
        let same = chosen.as_ref().map(|m| &m.id) == self.active.as_ref().map(|m| &m.id);
        // A renamed microphone is still the same one; only its name is refreshed.
        let from = std::mem::replace(&mut self.active, chosen);
        (!same).then(|| MicrophoneChange {
            from,
            to: self.active.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mic(id: &str, name: &str) -> Microphone {
        Microphone {
            id: MicId::new(id),
            name: name.to_owned(),
        }
    }

    fn connected(microphones: &[Microphone], default: Option<&str>) -> Connected {
        Connected {
            microphones: microphones.to_vec(),
            default: default.map(MicId::new),
        }
    }

    fn usb() -> Microphone {
        mic("usb-1", "Yeti")
    }

    fn built_in() -> Microphone {
        mic("builtin", "MacBook Pro-mikrofon")
    }

    fn headset() -> Microphone {
        mic("bt-1", "AirPods")
    }

    #[test]
    fn the_first_connected_entry_in_the_order_wins() {
        let order = [usb(), built_in()];
        let now = connected(&[built_in(), usb()], Some("builtin"));
        assert_eq!(choose(&order, &now), Some(&usb()));
    }

    #[test]
    fn falls_back_to_the_next_entry_when_the_first_is_gone() {
        let order = [usb(), built_in()];
        let now = connected(&[headset(), built_in()], Some("bt-1"));
        assert_eq!(choose(&order, &now), Some(&built_in()));
    }

    #[test]
    fn the_order_beats_the_system_default() {
        // The system moves its default to a headset that connects; the user's order still holds.
        let order = [built_in()];
        let now = connected(&[built_in(), headset()], Some("bt-1"));
        assert_eq!(choose(&order, &now), Some(&built_in()));
    }

    #[test]
    fn without_a_connected_entry_the_system_default_is_used() {
        let order = [usb()];
        let now = connected(&[built_in(), headset()], Some("bt-1"));
        assert_eq!(choose(&order, &now), Some(&headset()));
    }

    #[test]
    fn an_empty_order_uses_the_system_default() {
        let now = connected(&[built_in(), headset()], Some("bt-1"));
        assert_eq!(choose(&[], &now), Some(&headset()));
    }

    #[test]
    fn without_a_default_the_first_connected_is_used() {
        let now = connected(&[headset(), built_in()], None);
        assert_eq!(choose(&[usb()], &now), Some(&headset()));
    }

    #[test]
    fn a_default_that_is_not_listed_is_ignored() {
        let now = connected(&[built_in()], Some("gone"));
        assert_eq!(choose(&[], &now), Some(&built_in()));
    }

    #[test]
    fn nothing_connected_chooses_nothing() {
        assert_eq!(choose(&[usb()], &Connected::default()), None);
    }

    #[test]
    fn an_entry_matches_by_name_when_its_id_changed() {
        let order = [usb(), built_in()];
        let moved = mic("usb-2", "Yeti");
        let now = connected(&[built_in(), moved.clone()], Some("builtin"));
        assert_eq!(choose(&order, &now), Some(&moved));
    }

    #[test]
    fn a_name_match_never_takes_a_microphone_another_entry_holds() {
        // Two microphones of the same model: the first is gone, and the second, which the user
        // put last, must not jump ahead of the built-in one.
        let first = mic("usb-1", "Yeti");
        let second = mic("usb-2", "Yeti");
        let order = [first, built_in(), second.clone()];
        let now = connected(&[second, built_in()], None);
        assert_eq!(choose(&order, &now), Some(&built_in()));
    }

    #[test]
    fn the_selector_reports_a_microphone_that_comes_and_goes() {
        let order = vec![usb(), built_in()];
        let mut selector = Selector::new(order, connected(&[built_in()], Some("builtin")));
        assert_eq!(selector.active(), Some(&built_in()));

        let change = selector.update(connected(&[built_in(), usb()], Some("builtin")));
        assert_eq!(
            change,
            Some(MicrophoneChange {
                from: Some(built_in()),
                to: Some(usb()),
            })
        );

        let change = selector.update(connected(&[built_in()], Some("builtin")));
        assert_eq!(
            change,
            Some(MicrophoneChange {
                from: Some(usb()),
                to: Some(built_in()),
            })
        );
    }

    #[test]
    fn the_selector_is_quiet_when_the_choice_stays() {
        let order = vec![usb(), built_in()];
        let mut selector = Selector::new(order, connected(&[usb()], None));
        assert_eq!(
            selector.update(connected(&[usb(), headset()], Some("bt-1"))),
            None
        );
        assert_eq!(selector.update(connected(&[headset(), usb()], None)), None);
    }

    #[test]
    fn the_selector_keeps_a_renamed_microphone_without_a_change() {
        let mut selector = Selector::new(vec![usb()], connected(&[usb()], None));
        let renamed = mic("usb-1", "Yeti Nano");
        assert_eq!(
            selector.update(connected(std::slice::from_ref(&renamed), None)),
            None
        );
        assert_eq!(selector.active(), Some(&renamed));
    }

    #[test]
    fn the_selector_reports_when_the_last_microphone_disappears() {
        let mut selector = Selector::new(vec![], connected(&[built_in()], None));
        assert_eq!(
            selector.update(Connected::default()),
            Some(MicrophoneChange {
                from: Some(built_in()),
                to: None,
            })
        );
        assert_eq!(selector.active(), None);
    }

    #[test]
    fn a_new_order_moves_the_choice_at_once() {
        let mut selector = Selector::new(
            vec![usb(), built_in()],
            connected(&[built_in(), usb()], None),
        );
        let change = selector.set_order(vec![built_in(), usb()]);
        assert_eq!(
            change,
            Some(MicrophoneChange {
                from: Some(usb()),
                to: Some(built_in()),
            })
        );
    }
}
