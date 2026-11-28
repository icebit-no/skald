//! The hold key as the configuration names it and as the key listener takes it (D-61).

use skald_core::config::HoldKey as ConfigHoldKey;
use skald_core::hold_key::FUNCTION_KEYS;
use skald_input::HoldKey;

/// The key to listen for, or the name of one this version or this system cannot listen for: a key a newer
/// Skald wrote, or fn on Windows, which never reaches a program there.
pub(crate) fn listened(key: &ConfigHoldKey) -> Result<HoldKey, String> {
    Ok(match key {
        ConfigHoldKey::RightCommand => HoldKey::RightCommand,
        ConfigHoldKey::RightControl => HoldKey::RightControl,
        ConfigHoldKey::RightShift => HoldKey::RightShift,
        ConfigHoldKey::RightOption => HoldKey::RightOption,
        ConfigHoldKey::LeftCommand => HoldKey::LeftCommand,
        ConfigHoldKey::LeftControl => HoldKey::LeftControl,
        ConfigHoldKey::LeftShift => HoldKey::LeftShift,
        ConfigHoldKey::LeftOption => HoldKey::LeftOption,
        ConfigHoldKey::Fn if cfg!(windows) => return Err(String::from(key.clone())),
        ConfigHoldKey::Fn => HoldKey::Fn,
        ConfigHoldKey::Function(number) if FUNCTION_KEYS.contains(number) => {
            HoldKey::Function(*number)
        }
        ConfigHoldKey::Function(_) | ConfigHoldKey::Unknown(_) => {
            return Err(String::from(key.clone()));
        }
    })
}

/// The configuration's name for a key the listener takes.
pub(crate) fn configured(key: HoldKey) -> ConfigHoldKey {
    match key {
        HoldKey::RightCommand => ConfigHoldKey::RightCommand,
        HoldKey::RightControl => ConfigHoldKey::RightControl,
        HoldKey::RightShift => ConfigHoldKey::RightShift,
        HoldKey::RightOption => ConfigHoldKey::RightOption,
        HoldKey::LeftCommand => ConfigHoldKey::LeftCommand,
        HoldKey::LeftControl => ConfigHoldKey::LeftControl,
        HoldKey::LeftShift => ConfigHoldKey::LeftShift,
        HoldKey::LeftOption => ConfigHoldKey::LeftOption,
        HoldKey::Fn => ConfigHoldKey::Fn,
        HoldKey::Function(number) => ConfigHoldKey::Function(number),
    }
}

/// The key as a sentence names it: «høyre ⌘», «F13».
pub(crate) fn name(key: HoldKey) -> String {
    configured(key).name()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_hold_key_the_configuration_knows_is_listened_for_and_named_back() {
        for name in skald_core::hold_key::NAMES {
            let configuration = ConfigHoldKey::from(name.to_owned());
            if cfg!(windows) && configuration == ConfigHoldKey::Fn {
                assert_eq!(listened(&configuration), Err("fn".into()));
                continue;
            }
            let key = listened(&configuration).unwrap();
            assert_eq!(configured(key), configuration, "{name}");
        }
    }

    #[test]
    fn a_key_this_version_does_not_know_is_named_for_the_line_that_says_so() {
        assert_eq!(
            listened(&ConfigHoldKey::Unknown("right-hyper".into())),
            Err("right-hyper".into())
        );
        assert_eq!(listened(&ConfigHoldKey::Function(20)), Err("f20".into()));
    }

    #[test]
    fn the_key_is_named_as_the_configuration_names_it_to_the_user() {
        assert_eq!(name(HoldKey::Function(13)), "F13");
        assert_eq!(
            name(HoldKey::PLATFORM_DEFAULT),
            ConfigHoldKey::default().name()
        );
    }
}
