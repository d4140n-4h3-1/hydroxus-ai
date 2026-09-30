//! Making way in a corridor. Someone walking veers to its right round anyone ahead of it, so
//! that two meeting head on pass each other on the left - to the left instead if a wall is in the
//! way - and stops only for someone right in front of it. Someone standing about that sees
//! another coming straight at it steps out of the way.
//!
//! Others are given as where they are, and which way they are walking, if they are; where there
//! is floor, by a function the game answers from its grid.

use crate::flat;
use nalgebra::Vector3;

/// How far ahead, in meters, someone walking starts to veer round anyone in its way.
pub const AVOID_RANGE: f32 = 2.5;
/// How far to either side of its way, in meters, someone has to be not to be in it.
pub const PASSING: f32 = 1.0;
/// How hard it veers round someone just in front of it: how far to the side for every meter
/// ahead, less the further off they are.
pub const VEER: f32 = 1.2;
/// How close, in meters, someone straight ahead has to be for it to stop for them.
pub const KEEP_CLEAR: f32 = 0.8;
/// How far ahead, in meters, it makes sure there is floor before veering that way.
pub const LOOK_AHEAD: f32 = 0.6;
/// How near, in meters, someone coming straight at one standing about has to be for that one to
/// step out of its way, and how far it steps.
pub const YIELD_RANGE: f32 = 2.2;
pub const STEP_ASIDE: f32 = 0.8;

/// An NPC's right, going `ahead`.
pub fn right_of(ahead: Vector3<f32>) -> Vector3<f32> {
    Vector3::new(-ahead.z, 0.0, ahead.x)
}

/// Which way an NPC at `feet` should go to make its way `ahead` round `others` - each where
/// they are, and which way they are walking, if they are - veering to its right, or failing that
/// to its left, but only where `floor_at` says there is floor. And whether someone is right in
/// front of it, so that it has to stop.
pub fn make_way(
    feet: Vector3<f32>,
    ahead: Vector3<f32>,
    others: impl Iterator<Item = (Vector3<f32>, Option<Vector3<f32>>)>,
    floor_at: impl Fn(Vector3<f32>) -> bool,
) -> (Vector3<f32>, bool) {
    let right = right_of(ahead);
    let mut veer: f32 = 0.0;
    let mut blocked = false;
    for (there, _) in others {
        let to_them = flat(there - feet);
        let distance = to_them.norm();
        let (along, across) = (to_them.dot(&ahead), to_them.dot(&right));
        if distance > AVOID_RANGE || along <= 0.0 || across.abs() > PASSING {
            continue;
        }
        veer = veer.max(VEER * (1.0 - distance / AVOID_RANGE));
        blocked |= distance < KEEP_CLEAR && along > 0.8 * distance;
    }
    if veer == 0.0 {
        return (ahead, blocked);
    }
    // Right if there is room, then left, and less sharply before not at all.
    for side in [veer, 0.5 * veer, -veer, -0.5 * veer] {
        let way = (ahead + right * side).normalize();
        if floor_at(feet + way * LOOK_AHEAD) {
            // Veering hard, it gets past whoever is in front of it rather than waiting for them.
            return (way, blocked && side.abs() < 0.5 * VEER);
        }
    }
    (ahead, blocked)
}

/// Where an NPC standing about at `feet` should step to, to get out of the way of any of
/// `others` walking straight at it: out to the side they are not veering to, or failing that
/// the other side, wherever `floor_at` says there is floor.
pub fn step_aside(
    feet: Vector3<f32>,
    others: impl Iterator<Item = (Vector3<f32>, Option<Vector3<f32>>)>,
    floor_at: impl Fn(Vector3<f32>) -> bool,
) -> Option<Vector3<f32>> {
    for (there, walking) in others {
        let Some(going) = walking else {
            continue;
        };
        let to_me = flat(feet - there);
        let distance = to_me.norm();
        let (along, across) = (to_me.dot(&going), to_me.dot(&right_of(going)));
        if distance > YIELD_RANGE || along <= 0.0 || across.abs() > PASSING {
            continue;
        }
        // They veer to their right, so out to their left.
        let left = -right_of(going);
        return [left, -left]
            .into_iter()
            .map(|side| feet + side * STEP_ASIDE)
            .find(|&spot| floor_at(spot) && floor_at(feet + (spot - feet) * 0.5));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const AHEAD: Vector3<f32> = Vector3::new(0.0, 0.0, 1.0);
    const EVERYWHERE: fn(Vector3<f32>) -> bool = |_| true;

    fn standing(x: f32, z: f32) -> (Vector3<f32>, Option<Vector3<f32>>) {
        (Vector3::new(x, 0.0, z), None)
    }

    #[test]
    fn with_nobody_about_it_goes_straight_on() {
        let (way, blocked) = make_way(Vector3::zeros(), AHEAD, std::iter::empty(), EVERYWHERE);
        assert_eq!((way, blocked), (AHEAD, false));
        // Someone behind, or well off to one side, is not in the way either.
        let others = [standing(0.0, -1.0), standing(2.0, 1.0)];
        let (way, _) = make_way(Vector3::zeros(), AHEAD, others.into_iter(), EVERYWHERE);
        assert_eq!(way, AHEAD);
    }

    #[test]
    fn it_veers_right_round_someone_ahead_and_harder_the_nearer() {
        let right = right_of(AHEAD);
        let (far, _) = make_way(
            Vector3::zeros(),
            AHEAD,
            [standing(0.0, 2.0)].into_iter(),
            EVERYWHERE,
        );
        let (near, blocked) = make_way(
            Vector3::zeros(),
            AHEAD,
            [standing(0.0, 1.0)].into_iter(),
            EVERYWHERE,
        );
        assert!(far.dot(&right) > 0.0 && near.dot(&right) > far.dot(&right));
        assert!(!blocked);
    }

    #[test]
    fn with_a_wall_on_the_right_it_veers_left() {
        let right = right_of(AHEAD);
        let floor = |spot: Vector3<f32>| spot.dot(&right_of(AHEAD)) < 0.1;
        let (way, _) = make_way(
            Vector3::zeros(),
            AHEAD,
            [standing(0.0, 1.0)].into_iter(),
            floor,
        );
        assert!(way.dot(&right) < 0.0);
    }

    #[test]
    fn hemmed_in_with_someone_right_in_front_it_stops() {
        let floor = |spot: Vector3<f32>| spot.x.abs() < 0.05;
        let (_, blocked) = make_way(
            Vector3::zeros(),
            AHEAD,
            [standing(0.0, 0.5)].into_iter(),
            floor,
        );
        assert!(blocked);
    }

    #[test]
    fn standing_about_it_steps_out_of_the_way_of_someone_coming_at_it() {
        let coming = (Vector3::new(0.0, 0.0, -1.5), Some(AHEAD));
        let spot = step_aside(Vector3::zeros(), [coming].into_iter(), EVERYWHERE).unwrap();
        // Out to their left, since they veer to their right.
        assert!(spot.dot(&right_of(AHEAD)) < -0.5);
        // Nobody walking at it, or someone walking away, leaves it be.
        let going_away = (Vector3::new(0.0, 0.0, -1.5), Some(-AHEAD));
        assert_eq!(
            step_aside(Vector3::zeros(), [going_away].into_iter(), EVERYWHERE),
            None
        );
        let waiting = standing(0.0, -1.5);
        assert_eq!(
            step_aside(Vector3::zeros(), [waiting].into_iter(), EVERYWHERE),
            None
        );
    }
}
