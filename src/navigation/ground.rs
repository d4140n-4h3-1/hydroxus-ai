//! Following a ground route and giving nearby walkers room on a single floor.
//!
//! The caller applies the proposed position. For a group, reserve each accepted position
//! before stepping the next walker so that their moves cannot claim the same space.

use crate::{
    flat,
    navigation::{navmesh::NavMesh, steer},
    tactics::spread,
    Tuning,
};
use nalgebra::Vector3;

/// Minimum distance between walker centers on the same floor, in meters.
pub const CLEARANCE: f32 = 0.45;

/// Result of trying to follow a route this update.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Step {
    /// Move to this position. The next waypoint is removed if reached.
    Move(Vector3<f32>),
    /// No waypoints remain.
    Arrived,
    /// Another walker is in the way. The route is still usable.
    Waiting,
    /// The direct step leaves the floor or enters blocked ground. Refresh the route.
    Replan,
}

/// Propose a step of at most `distance` meters along a same-floor route from
/// [`super::walk::route_to`]. The next waypoint is at the end of `route`.
/// Supply other walkers' accepted positions, excluding the current walker, and a stable
/// `id` to break ties if two walkers start at the same spot. Apply a returned move before
/// advancing this route again. The caller handles stairs and other changes of floor.
pub fn step(
    mesh: &NavMesh,
    tuning: &Tuning,
    feet: Vector3<f32>,
    route: &mut Vec<Vector3<f32>>,
    distance: f32,
    others: &[Vector3<f32>],
    id: u32,
) -> Step {
    let Some(&waypoint) = route.last() else {
        return Step::Arrived;
    };
    let Some(floor) = mesh.poly_at(feet).filter(|&poly| !mesh.is_blocked(poly)) else {
        return Step::Replan;
    };
    let height = mesh.floor_of(floor);
    if (waypoint.y - height).abs() > 1.0e-3 || !waypoint.iter().all(|v| v.is_finite()) {
        return Step::Replan;
    }

    let delta = flat(waypoint - feet);
    let remaining = delta.norm();
    if remaining <= 1.0e-5 {
        route.pop();
        return Step::Move(Vector3::new(feet.x, height, feet.z));
    }
    if !distance.is_finite() || distance <= 0.0 {
        return Step::Waiting;
    }
    let ahead = delta / remaining;
    let length = distance.min(remaining);
    let on_floor = |at| mesh.on_floor_segment(feet, at, height);
    let reached = length == remaining;
    let direct = if reached {
        Vector3::new(waypoint.x, height, waypoint.z)
    } else {
        Vector3::new(feet.x, height, feet.z) + ahead * length
    };
    if !on_floor(direct) {
        return Step::Replan;
    }

    let nearby = || {
        others
            .iter()
            .copied()
            .filter(|other| (other.y - height).abs() <= 1.0e-3)
    };
    let clear = |at| nearby().all(|other| clears_walker(feet, at, other));
    if clear(direct) {
        if reached {
            route.pop();
        }
        return Step::Move(direct);
    }

    let (veer, _) = steer::make_way(
        &tuning.steer,
        feet,
        ahead,
        nearby().map(|at| (at, None)),
        on_floor,
    );
    let push = spread::separation(&tuning.spread, feet, nearby(), id);
    let side = steer::right_of(ahead);
    for way in [veer + push, ahead + side, ahead - side, side, -side, push] {
        if let Some(direction) = way.try_normalize(1.0e-5) {
            let at = Vector3::new(feet.x, height, feet.z) + direction * length;
            if on_floor(at) && clear(at) {
                return Step::Move(at);
            }
        }
    }
    Step::Waiting
}

fn clears_walker(from: Vector3<f32>, to: Vector3<f32>, other: Vector3<f32>) -> bool {
    let offset = flat(from - other);
    let delta = flat(to - from);
    let minimum = CLEARANCE * CLEARANCE;
    if offset.norm_squared() < minimum {
        // When starting too close, allow movement that increases clearance throughout the step.
        return offset.dot(&delta) >= 0.0 && delta.norm_squared() > 0.0;
    }
    let fraction = (-offset.dot(&delta) / delta.norm_squared()).clamp(0.0, 1.0);
    (offset + delta * fraction).norm_squared() >= minimum
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::navigation::grid::WalkGrid;

    fn mesh(wall: bool) -> NavMesh {
        let mut grid = WalkGrid::new(5, 3, 1.0);
        for z in 0..3 {
            for x in 0..5 {
                grid.set(x, z, !(wall && x == 2));
            }
        }
        NavMesh::build(&grid, Vector3::zeros(), |_, _| 0)
    }

    #[test]
    fn a_step_over_a_gap_requests_replanning() {
        let mesh = mesh(true);
        let start = Vector3::new(1.5, 0.0, 1.5);
        let goal = Vector3::new(3.5, 0.0, 1.5);
        let mut route = vec![goal];
        assert_eq!(
            step(&mesh, &Tuning::DEFAULT, start, &mut route, 2.0, &[], 0),
            Step::Replan
        );
        assert_eq!(route, vec![goal]);
    }

    #[test]
    fn a_walker_veers_without_consuming_an_unreached_waypoint() {
        let mesh = mesh(false);
        let start = Vector3::new(1.5, 0.0, 1.5);
        let goal = Vector3::new(3.5, 0.0, 1.5);
        let other = Vector3::new(2.0, 0.0, 1.5);
        let mut route = vec![goal];
        let Step::Move(at) = step(&mesh, &Tuning::DEFAULT, start, &mut route, 0.3, &[other], 0)
        else {
            panic!("walker should be able to make way");
        };
        assert!(clears_walker(start, at, other));
        assert!((at - start).norm() <= 0.300_01);
        assert_eq!(route, vec![goal]);
        assert_eq!(
            step(&mesh, &Tuning::DEFAULT, at, &mut route, 3.0, &[], 0),
            Step::Move(goal)
        );
        assert!(route.is_empty());
    }
}
