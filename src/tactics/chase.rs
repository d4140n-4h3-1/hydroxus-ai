//! Chasing and being chased. One of several after someone that is not the nearest of them makes
//! for a little way ahead of them, the way they are going, to cut them off, rather than running
//! after them - unless it is near enough to go straight at them. One running from a pursuer makes
//! for somewhere a good way off, straight away from it.

use crate::{
    navigation::grid::{Rng, WalkGrid},
    navigation::route::spot_near,
};
use nalgebra::Vector3;

/// How far ahead of whoever it is after, at most, one that is not the nearest makes for, to cut
/// them off, in meters; and how near it has to be to go straight at them instead.
pub const CUT_OFF: f32 = 10.0;
pub const CLOSE_IN: f32 = 4.0;
/// How far ahead one running makes for, in meters, each time it looks again where to go.
pub const FLEE_AHEAD: f32 = 25.0;

/// Where one `away` meters from `them`, who are `going` that way, makes for to cut them off -
/// unless it is the `nearest` of those after them, or near enough to go straight at them, or the
/// floor does not carry on that far (`floor_at`): none, then.
pub fn cut_off(
    them: Vector3<f32>,
    going: Option<Vector3<f32>>,
    away: f32,
    nearest: bool,
    floor_at: impl Fn(Vector3<f32>) -> bool,
) -> Option<Vector3<f32>> {
    going
        .filter(|_| !nearest && away > CLOSE_IN)
        .and_then(|going| going.try_normalize(1.0e-3))
        .map(|going| them + going * (away * 0.6).min(CUT_OFF))
        .filter(|&ahead| (1..=4).all(|i| floor_at(them + (ahead - them) * (i as f32 / 4.0))))
}

/// Where one at `feet` running `away` - straight away from whoever is after it, along the
/// ground - makes for, over `grid` whose corner is at `origin`: on the floor near
/// [`FLEE_AHEAD`] meters on, or that far on if there is none.
pub fn flee_to(
    (grid, origin): (&WalkGrid, Vector3<f32>),
    feet: Vector3<f32>,
    away: Vector3<f32>,
    rng: &mut Rng,
) -> Vector3<f32> {
    let ahead = feet + away * FLEE_AHEAD;
    spot_near((grid, origin), ahead, FLEE_AHEAD * 0.4, rng).unwrap_or(ahead)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_others_cut_them_off_ahead_but_the_nearest_and_those_close_go_straight_at_them() {
        let them = Vector3::new(0.0, 0.0, 0.0);
        let going = Some(Vector3::new(0.0, 0.0, 3.0));
        let floor = |_: Vector3<f32>| true;
        assert_eq!(cut_off(them, going, 10.0, false, floor), Some(Vector3::new(0.0, 0.0, 6.0)));
        assert_eq!(cut_off(them, going, 40.0, false, floor), Some(Vector3::new(0.0, 0.0, CUT_OFF)));
        assert_eq!(cut_off(them, going, 10.0, true, floor), None);
        assert_eq!(cut_off(them, going, CLOSE_IN - 1.0, false, floor), None);
        assert_eq!(cut_off(them, None, 10.0, false, floor), None);
        assert_eq!(cut_off(them, going, 10.0, false, |at: Vector3<f32>| at.z < 3.0), None);
    }
}
