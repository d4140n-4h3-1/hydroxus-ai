//! Metal Gear's alert phases, which a hostile NPC goes through as it hunts the player:
//!
//! - **Alert**: it can see them, and is after them.
//! - **Evasion**: it has lost them, and searches, for [`EVASION`] seconds.
//! - **Caution**: it has given up searching, and goes about as before, but watching for them, for
//!   [`CAUTION`] seconds; then it is calm again.
//!
//! Seeing them again in any phase, it is back on Alert.

/// How long an NPC searches for the player once it has lost them, in seconds, and how long it
/// stays wary after that.
pub const EVASION: f32 = 30.0;
pub const CAUTION: f32 = 60.0;

/// How a hostile NPC is going about the player, least urgent first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Alert {
    /// It has given up searching for them, and goes about, watching for them.
    Caution,
    /// It has lost them, and is searching.
    Evasion,
    /// It can see them, and is after them.
    Alert,
}

impl Alert {
    /// The phase that follows this one, seeing the player or not, with `left` seconds left of
    /// searching or of being wary: none, once it is calm again.
    pub fn next(self, sees: bool, left: f32) -> Option<Alert> {
        Some(match self {
            _ if sees => Alert::Alert,
            Alert::Alert => Alert::Evasion,
            Alert::Evasion if left <= 0.0 => Alert::Caution,
            Alert::Caution if left <= 0.0 => return None,
            other => other,
        })
    }

    /// How long this phase lasts, in seconds, once the player is out of sight: none for Alert,
    /// which lasts as long as they are in sight.
    pub fn lasts(self) -> Option<f32> {
        match self {
            Alert::Alert => None,
            Alert::Evasion => Some(EVASION),
            Alert::Caution => Some(CAUTION),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_searches_once_it_loses_them_gives_up_and_calms_down_in_the_end() {
        assert_eq!(Alert::Alert.next(true, 0.0), Some(Alert::Alert));
        assert_eq!(Alert::Alert.next(false, 0.0), Some(Alert::Evasion));
        assert_eq!(Alert::Evasion.next(false, 5.0), Some(Alert::Evasion));
        assert_eq!(Alert::Evasion.next(false, 0.0), Some(Alert::Caution));
        assert_eq!(Alert::Evasion.next(true, 5.0), Some(Alert::Alert));
        assert_eq!(Alert::Caution.next(false, 5.0), Some(Alert::Caution));
        assert_eq!(Alert::Caution.next(true, 5.0), Some(Alert::Alert));
        assert_eq!(Alert::Caution.next(false, 0.0), None, "calm again");
    }

    #[test]
    fn the_most_urgent_phase_is_the_greatest() {
        assert!(Alert::Alert > Alert::Evasion && Alert::Evasion > Alert::Caution);
    }
}
