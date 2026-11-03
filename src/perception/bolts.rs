//! Bolts flying by. One that passes in front of an NPC within [`NOTICE_BOLT`] meters is noticed,
//! as it would see anything there; one within [`NEAR_MISS`] of its chest is felt, wherever it
//! goes by - taken as being shot at.

use crate::{
    alert::Alert,
    perception::sight::{Sight, Stance},
};
use nalgebra::Vector3;

/// How near a bolt has to fly past an NPC's chest for it to take it as being shot at, in meters.
pub const NEAR_MISS: f32 = 1.0;
/// How near a bolt has to fly past, in meters, for an NPC to notice it, if it is in front of it.
pub const NOTICE_BOLT: f32 = 4.0;

/// Whether a bolt that came `near` meters from an NPC's chest is felt: close enough to take as
/// being shot at, wherever it went by.
pub fn felt(near: f32) -> bool {
    near <= NEAR_MISS
}

/// Whether an NPC at `feet`, facing `heading`, in `alert` - or calm, with none - notices a bolt
/// that came `near` meters from its chest, nearest at `at`: felt, or seen going by in front.
pub fn notices(alert: Option<Alert>, feet: Vector3<f32>, heading: f32, near: f32, at: Vector3<f32>) -> bool {
    felt(near)
        || near <= NOTICE_BOLT && Sight::default().could_see(alert, feet, heading, at, Stance::Standing, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bolt_is_felt_close_by_anywhere_and_noticed_further_off_only_in_front() {
        let feet = Vector3::zeros();
        // Facing +z.
        let ahead = Vector3::new(0.0, 1.0, 3.0);
        let behind = Vector3::new(0.0, 1.0, -3.0);
        assert!(notices(None, feet, 0.0, 3.0, ahead));
        assert!(!notices(None, feet, 0.0, 3.0, behind));
        assert!(notices(None, feet, 0.0, 0.8, Vector3::new(0.0, 1.0, -0.8)));
        assert!(!notices(None, feet, 0.0, 5.0, ahead));
        assert!(felt(NEAR_MISS) && !felt(NEAR_MISS + 0.1));
    }
}
