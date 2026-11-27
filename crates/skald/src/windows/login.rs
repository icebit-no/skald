//! Start at login, the setting, on Windows: Skald registers nothing yet. The entry has to name the
//! installed program, which M4's packaging decides.

use crate::say::Line;

/// Leaves the session's start-up alone, with nothing to say.
pub(crate) fn apply(_wanted: bool) -> Option<Line> {
    None
}
