//! Gameplay presets for how quickly guards notice trouble and how they follow up.
//!
//! These presets use the same perception and search inputs the game already supplies.
//! Set the difficulty before building simulation state, and pass the resulting tuning
//! to both the perception code and the squad.

use crate::Tuning;

/// A gameplay difficulty for NPC perception, alertness, and searches.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum Difficulty {
    /// More time to slip past, shorter follow-up searches, and less reliance on old leads.
    Easy,
    /// The game's baseline behavior.
    #[default]
    Normal,
    /// Quicker detection, longer searches, and more persistent use of remembered leads.
    Hard,
}

impl Difficulty {
    /// Use this difficulty with the crate's default tuning.
    pub const fn tuning(self) -> Tuning {
        self.apply(Tuning::DEFAULT)
    }

    /// Apply the preset to a game's baseline tuning.
    ///
    /// Start from the baseline each time the difficulty changes. Presets scale the
    /// game's detection, alert and search tuning. The game controls health, damage,
    /// movement speed and what information the NPC can sense.
    pub const fn apply(self, mut base: Tuning) -> Tuning {
        let (notice_time, alert_time, search_interval, memory_weight) = match self {
            Self::Easy => (1.5, 0.65, 1.5, 0.5),
            Self::Normal => return base,
            Self::Hard => (0.75, 1.35, 0.75, 1.5),
        };
        base.detection.fill *= notice_time;
        base.alert.evasion *= alert_time;
        base.alert.caution *= alert_time;
        base.alert.wary_half_life *= alert_time;
        base.orchestrate.look_every *= search_interval;
        base.orchestrate.lead_half_life *= alert_time;
        base.orchestrate.sighting_half_life *= alert_time;
        base.orchestrate.memory_lean *= memory_weight;
        base
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::perception::detection::Detection;

    #[test]
    fn presets_validate_and_normal_preserves_the_baseline() {
        let mut custom = Tuning::DEFAULT;
        custom.detection.fill = 2.0;
        assert_eq!(Difficulty::Normal.apply(custom), custom);
        assert_eq!(Difficulty::Easy.apply(custom).detection.fill, 3.0);
        assert_eq!(Difficulty::Hard.apply(custom).detection.fill, 1.5);
        for difficulty in [Difficulty::Easy, Difficulty::Normal, Difficulty::Hard] {
            let tuning = difficulty.tuning();
            assert_eq!(tuning.validate(), Ok(()));
            assert_eq!(tuning.fire, Tuning::DEFAULT.fire);
        }
    }

    #[test]
    fn higher_difficulty_spots_the_same_visible_target_sooner() {
        let steps_to_spot = |difficulty: Difficulty| {
            let tuning = difficulty.tuning();
            let mut detection = Detection::default();
            (1..=240)
                .find(|_| {
                    detection.step(&tuning.detection, 0.5, 1.0, 1.0 / 60.0);
                    detection.spotted()
                })
                .expect("target is eventually spotted")
        };
        let [easy, normal, hard] =
            [Difficulty::Easy, Difficulty::Normal, Difficulty::Hard].map(steps_to_spot);
        assert!(hard < normal && normal < easy);
    }
}
