//! Start at login, the setting, on Linux: Skald registers nothing. A Hyprland session starts programs from
//! `exec-once` in its own configuration, which the user writes.

use crate::say::Line;

/// Leaves the session's start-up alone, with nothing to say.
pub(crate) fn apply(_wanted: bool) -> Option<Line> {
    None
}
