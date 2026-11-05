//! Flying free: a flier's way through the air, at any height, rather than along the floor.
//!
//! Straight there, if nothing is in the way. If something is - a wall, a building - up as little
//! as it can, over it and down on the other side: as long as there is nothing overhead, up to
//! [`CLIMB`] above the higher of the two ends. Under a roof, along a way on the floor - the walk
//! grid's - at its height over the floor, its corners cut where they can be.
//!
//! The crate never looks at a scene: the game says whether a ball of a given radius could fly from
//! one point to another without touching anything (`clear`), casting it through its own world.

use nalgebra::Vector3;

/// How far a flier keeps from anything solid on its way, in meters - a quadcopter's rotors'
/// guards reach 0.69 m out; how far up it looks for a way over what is in its way, and in steps of
/// how much; and how many cuts it tries across the corners of a way along the floor.
pub const FLIGHT_CLEARANCE: f32 = 0.75;
pub const CLIMB: f32 = 80.0;
pub const CLIMB_STEP: f32 = 1.5;
pub const SHORTCUTS: usize = 40;

/// Whether a ball `radius` across its middle could fly from the first point to the second without
/// touching anything solid in its way.
pub type Clear<'a> = dyn Fn(Vector3<f32>, Vector3<f32>, f32) -> bool + 'a;

/// How far a flier could rise from `at`, up to `most`, keeping [`FLIGHT_CLEARANCE`] from anything
/// in its way, as `clear` has it.
pub fn headroom(clear: &Clear<'_>, at: Vector3<f32>, most: f32) -> f32 {
    let (mut low, mut high) = (0.0, most);
    if clear(at, at + Vector3::new(0.0, most, 0.0), FLIGHT_CLEARANCE) {
        return most;
    }
    // Halving the way to whatever is overhead.
    for _ in 0..8 {
        let middle = (low + high) / 2.0;
        match clear(at, at + Vector3::new(0.0, middle, 0.0), FLIGHT_CLEARANCE) {
            true => low = middle,
            false => high = middle,
        }
    }
    low
}

/// The way a flier flies from `from` to `to` through the air, as `clear` has it, at any height, as
/// a route: points in the air, the last one first, keeping [`FLIGHT_CLEARANCE`] from anything
/// solid. Straight there, if nothing is in the way. If something is, up as little as it can, over
/// it and down on the other side: as long as there is nothing overhead, up to [`CLIMB`] above the
/// higher of the two. Under a roof, along its `ground` route - along the floor, the last point
/// first, as the walk grid has it - `hover` over the floor, its corners cut where they can be.
/// Empty if there is no way: none overhead, and none along the floor.
pub fn flight(
    clear: &Clear<'_>,
    from: Vector3<f32>,
    to: Vector3<f32>,
    hover: f32,
    ground: impl FnOnce() -> Vec<Vector3<f32>>,
) -> Vec<Vector3<f32>> {
    if clear(from, to, FLIGHT_CLEARANCE) {
        return vec![to];
    }
    // As high as it can rise at either end, and the lowest it can cross between at that height.
    let top = (from.y + headroom(clear, from, CLIMB)).min(to.y + headroom(clear, to, CLIMB));
    let mut height = from.y.max(to.y) + CLIMB_STEP;
    while height <= top {
        let (up, over) = (Vector3::new(from.x, height, from.z), Vector3::new(to.x, height, to.z));
        if clear(up, over, FLIGHT_CLEARANCE) {
            return vec![to, over, up];
        }
        height += CLIMB_STEP;
    }
    // Along the floor, first point first, cutting straight across to the furthest point it can.
    let along: Vec<Vector3<f32>> = ground().into_iter().rev().map(|at| at + Vector3::new(0.0, hover, 0.0)).collect();
    let mut way = Vec::with_capacity(along.len());
    let (mut at, mut next, mut tries) = (from, 0, 0);
    while next < along.len() {
        let mut furthest = next;
        while furthest + 1 < along.len() && tries < SHORTCUTS {
            tries += 1;
            if !clear(at, along[furthest + 1], FLIGHT_CLEARANCE) {
                break;
            }
            furthest += 1;
        }
        way.push(along[furthest]);
        at = along[furthest];
        next = furthest + 1;
        if tries >= SHORTCUTS {
            way.extend_from_slice(&along[next..]);
            break;
        }
    }
    way.reverse();
    way
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOVER: f32 = 1.7;

    /// Whether a ball `radius` across flies from `a` to `b` clear of every box - its middle and its
    /// half-extents - looked at every 5 cm along the way.
    fn boxes(walls: &[(Vector3<f32>, Vector3<f32>)]) -> impl Fn(Vector3<f32>, Vector3<f32>, f32) -> bool + '_ {
        move |a, b, radius| {
            let steps = ((b - a).norm() / 0.05).ceil().max(1.0) as usize;
            (0..=steps).all(|k| {
                let p = a.lerp(&b, k as f32 / steps as f32);
                walls.iter().all(|&(at, half)| {
                    let off = (p - at).abs() - half;
                    off.map(|d| d.max(0.0)).norm() > radius
                })
            })
        }
    }

    fn flown(clear: &Clear<'_>, from: Vector3<f32>, route: &[Vector3<f32>]) -> bool {
        let mut at = from;
        route.iter().rev().all(|&next| {
            let leg = clear(at, next, 0.3);
            at = next;
            leg
        })
    }

    #[test]
    fn it_flies_straight_there_over_what_is_in_its_way_or_along_the_floor_under_a_roof() {
        let floor = (Vector3::new(0.0, -0.1, 0.0), Vector3::new(30.0, 0.1, 30.0));
        let (from, to) = (Vector3::new(0.0, HOVER, -8.0), Vector3::new(0.0, HOVER, 8.0));
        let nowhere = || -> Vec<Vector3<f32>> { panic!("no need of a way along the floor") };
        // Nothing in the way: straight there.
        let walls = [floor];
        assert_eq!(flight(&boxes(&walls), from, to, HOVER, nowhere), vec![to]);
        // A wall 5 m high across the way: up, over it with room to spare, and down.
        let walls = [floor, (Vector3::new(0.0, 2.5, 0.0), Vector3::new(20.0, 2.5, 0.2))];
        let route = flight(&boxes(&walls), from, to, HOVER, nowhere);
        assert_eq!(route.len(), 3, "{route:?}");
        assert_eq!(route[0], to);
        assert!(route[1].y > 5.0 + FLIGHT_CLEARANCE && route[1].y < 5.0 + FLIGHT_CLEARANCE + CLIMB_STEP + 0.01, "{route:?}");
        assert!(flown(&boxes(&walls), from, &route));
        // Under a roof 3 m up, the wall with a doorway at one end: along the floor, through it.
        let walls = [
            floor,
            (Vector3::new(0.0, 3.1, 0.0), Vector3::new(30.0, 0.1, 30.0)),
            (Vector3::new(-2.0, 1.5, 0.0), Vector3::new(18.0, 1.5, 0.2)),
        ];
        let door = Vector3::new(18.0, 0.0, 0.0);
        let steps: Vec<Vector3<f32>> = (0..=36)
            .map(|k| k as f32 / 36.0 * 2.0)
            .map(|t| match t < 1.0 {
                true => (from - Vector3::new(0.0, HOVER, 0.0)).lerp(&door, t),
                false => door.lerp(&(to - Vector3::new(0.0, HOVER, 0.0)), t - 1.0),
            })
            .rev()
            .collect();
        let route = flight(&boxes(&walls), from, to, HOVER, || steps.clone());
        assert!(flown(&boxes(&walls), from, &route), "{route:?}");
        assert!(route.iter().any(|at| at.x > 16.0 && at.z.abs() < 1.0), "not through the doorway: {route:?}");
        assert!(route.len() <= 8 && route.len() < steps.len() / 4, "{route:?}");
        assert_eq!(route.first().map(|at| (at - to).norm() < 1.0e-3), Some(true));
    }

    #[test]
    fn its_headroom_is_up_to_whatever_is_overhead() {
        let walls = [(Vector3::new(0.0, 10.1, 0.0), Vector3::new(5.0, 0.1, 5.0))];
        let room = headroom(&boxes(&walls), Vector3::new(0.0, 2.0, 0.0), CLIMB);
        assert!((room - (8.0 - FLIGHT_CLEARANCE)).abs() < 0.5, "{room}");
        assert_eq!(headroom(&boxes(&[]), Vector3::zeros(), CLIMB), CLIMB);
    }
}
