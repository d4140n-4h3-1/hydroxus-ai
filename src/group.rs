//! Walking about together: a pair, one keeping beside the other, or a group keeping about the one
//! leading it.

use crate::{forward, right_of};
use nalgebra::Vector3;

/// A pair walking about together: how far to its partner's right the one keeping beside it walks,
/// and how far behind, in meters.
pub const BESIDE: (f32, f32) = (1.1, 0.3);

/// Where each of a group keeps about the one leading it, after the first: how far to its right,
/// in meters (to its left, less than none), and how far behind. The first of them is a pair's.
pub const ABOUT_LEADER: [(f32, f32); 5] = [
    (BESIDE.0, BESIDE.1),
    (-BESIDE.0, BESIDE.1),
    (0.0, 1.5),
    (BESIDE.0, 1.9),
    (-BESIDE.0, 1.9),
];

/// Where the one keeping to `slot` of [`ABOUT_LEADER`] stands, about a leader at `feet` facing
/// `heading`.
pub fn about_leader(feet: Vector3<f32>, heading: f32, slot: usize) -> Vector3<f32> {
    let (right, behind) = ABOUT_LEADER[slot.min(ABOUT_LEADER.len() - 1)];
    feet + right_of(heading) * right - forward(heading) * behind
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn its_right_is_to_the_right() {
        // Facing +z, the right is -x; facing +x, it is +z.
        assert!((right_of(0.0) - Vector3::new(-1.0, 0.0, 0.0)).norm() < 1.0e-6);
        assert!((right_of(std::f32::consts::FRAC_PI_2) - Vector3::new(0.0, 0.0, 1.0)).norm() < 1.0e-6);
        assert!(right_of(0.7).dot(&forward(0.7)).abs() < 1.0e-6);
    }

    #[test]
    fn a_pair_walks_on_the_right_a_step_behind_and_a_group_fans_out_behind() {
        let at = |slot| about_leader(Vector3::zeros(), 0.0, slot);
        // Facing +z, the right is -x.
        assert!(at(0).x < 0.0 && at(0).z < 0.0, "{:?}", at(0));
        assert!(at(1).x > 0.0, "the next on the left");
        assert!((0..ABOUT_LEADER.len()).all(|slot| at(slot).z < 0.0), "all behind");
        assert_eq!(at(9), at(ABOUT_LEADER.len() - 1), "more keep to the last");
    }
}
