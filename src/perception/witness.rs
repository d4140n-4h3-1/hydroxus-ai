//! A shooting seen: one that saw it done knows who did it only if it saw them near enough to
//! tell who it was. Further off, it saw a shooting, and no more.

use crate::flat;
use nalgebra::Vector3;

/// How near the shooter, in meters, a witness has to be to see who it is that shot.
pub const IDENTIFY_REACH: f32 = 15.0;

tuning!(
    /// What a witness can tell of a shooting.
    {
        /// How near the shooter a witness has to be to see who it is, in meters.
        identify_reach: f32 = IDENTIFY_REACH,
    }
);

/// Whether a witness at `feet` knows who it was that shot from `shooter`: whether it `sees` them,
/// near enough to tell who it is (see [`IDENTIFY_REACH`]).
pub fn knows_who(tuning: &Tuning, sees: bool, feet: Vector3<f32>, shooter: Vector3<f32>) -> bool {
    sees && flat(shooter - feet).norm() <= tuning.identify_reach
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_witness_knows_who_shot_only_if_it_saw_them_near_enough() {
        let feet = Vector3::new(0.0, 0.0, 0.0);
        assert!(knows_who(&Tuning::DEFAULT, true, feet, Vector3::new(10.0, 0.0, 5.0)), "near, and seen");
        assert!(!knows_who(&Tuning::DEFAULT, true, feet, Vector3::new(40.0, 0.0, 0.0)), "a shot from far off");
        assert!(!knows_who(&Tuning::DEFAULT, false, feet, Vector3::new(3.0, 0.0, 0.0)), "near, but not seen");
        assert!(knows_who(&Tuning::DEFAULT, true, feet, Vector3::new(0.0, 6.0, IDENTIFY_REACH)), "however high up");
    }
}
