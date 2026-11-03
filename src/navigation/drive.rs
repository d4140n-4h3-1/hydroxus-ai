//! Driving: a rider that steers a vehicle round a course of checkpoints, flat out where the way
//! on is straight, easing off into the turns and over the speed pads on its way; pulls out round
//! one just ahead of it; and, in traffic, slows for whoever is in its way and stops short of them.
//!
//! The vehicle's handling is the game's - how it picks up speed, turns and brakes; this says what
//! the rider does with the controls.

use crate::{flat, forward};
use nalgebra::Vector3;

/// A vehicle as its rider sees it: where it is, which way it points, and how fast it is going.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vehicle {
    pub position: Vector3<f32>,
    pub yaw: f32,
    pub speed: f32,
}

impl Vehicle {
    /// Which way it points, along the ground.
    pub fn ahead(&self) -> Vector3<f32> {
        forward(self.yaw)
    }
}

/// A course to drive round: its checkpoints in order, round and round, the one to make for
/// next, and its speed pads, each where its middle is and which way along the road it points.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Course<'a> {
    pub gates: &'a [Vector3<f32>],
    pub next: usize,
    pub pads: &'a [(Vector3<f32>, Vector3<f32>)],
}

/// What the vehicle and the course are like, to drive: how fast it goes flat out; how far a pad
/// reaches from its middle along the road, and how high it reaches; and how far ahead a pad is
/// looked for, and how far off its line one is gone over for.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Handling {
    pub top_speed: f32,
    pub pad_reach: f32,
    pub pad_height: f32,
    pub pad_seen: f32,
    pub pad_detour: f32,
}

/// The controls as a rider sets them.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Controls {
    /// Forward (1) or brake and back (-1).
    pub throttle: f32,
    /// Left (1) or right (-1).
    pub steer: f32,
    pub boost: bool,
}

/// The heading from `from` to `to`, along the ground.
pub fn yaw_towards(from: Vector3<f32>, to: Vector3<f32>) -> f32 {
    (to.x - from.x).atan2(to.z - from.z)
}

/// `angle`, brought round to between -π and π.
fn wrap(angle: f32) -> f32 {
    (angle + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
}

/// Steers `vehicle` for the checkpoints of `course` ahead, `line` meters off their middles (left
/// positive): flat out where the way on is straight, easing off and braking into the turns - the
/// harder `pace` is over 1, the less - and over a speed pad just ahead, not far off its line.
pub fn autopilot(vehicle: &Vehicle, course: &Course, handling: &Handling, line: f32, pace: f32) -> Controls {
    let n = course.gates.len();
    let near = course.gates[course.next % n];
    let far = course.gates[(course.next + 1) % n];
    // Its line: off the middle of the road, square across the way on.
    let way = (far - near).try_normalize(1e-5).unwrap_or_else(Vector3::z);
    let left = Vector3::new(way.z, 0.0, -way.x);
    let (near_on, far_on) = (near + left * line, far + left * line);
    // Through the next gate towards the one after, a little, the nearer it is.
    let to_near = flat(near_on - vehicle.position).norm();
    let blend = (1.0 - to_near / 25.0).clamp(0.0, 1.0);
    let mut aim = near_on + (far_on - near_on).try_normalize(1e-5).unwrap_or_default() * 8.0 * blend;
    // A speed pad just ahead, not far off its line and pointing its way: over it.
    let heading = vehicle.ahead();
    let side = Vector3::new(heading.z, 0.0, -heading.x);
    let pad = course
        .pads
        .iter()
        .filter(|&&(at, way)| {
            let to = at - vehicle.position;
            let front = to.dot(&heading);
            front > handling.pad_reach
                && front < handling.pad_seen
                && to.dot(&side).abs() < handling.pad_detour
                && to.y.abs() < handling.pad_height * 2.0
                && way.dot(&heading) > 0.8
        })
        .min_by(|a, b| (a.0 - vehicle.position).norm().total_cmp(&(b.0 - vehicle.position).norm()));
    if let Some(&(at, way)) = pad {
        // Through its middle along it, rather than across it.
        aim = at + way * handling.pad_reach;
    }
    let off = wrap(yaw_towards(vehicle.position, aim) - vehicle.yaw);
    // How sharp the way on turns at the next gate, and at the one after it, as the vehicle nears
    // each: slow enough for the sharper, in time to make it.
    let after = course.gates[(course.next + 2) % n];
    let at_near = wrap(yaw_towards(near, far) - yaw_towards(vehicle.position, near)).abs();
    let at_far = wrap(yaw_towards(far, after) - yaw_towards(near, far)).abs();
    let to_far = to_near + (far - near).norm();
    let soon = |bend: f32, distance: f32| bend * (1.0 - distance / 70.0).clamp(0.0, 1.0);
    let bend = soon(at_near, to_near).max(soon(at_far, to_far));
    let speed = vehicle.speed;
    let wanted = handling.top_speed * (1.0 - bend * 0.9 / pace.max(0.1)).clamp(0.3, 1.0);
    // Pointed well off, it slows and comes round at a crawl, rather than backing up - which
    // would steer it the other way.
    let throttle = if off.abs() > 1.2 {
        if speed > 10.0 {
            -1.0
        } else {
            0.4
        }
    } else if speed > wanted + 3.0 {
        -1.0
    } else if speed > wanted {
        0.0
    } else {
        1.0
    };
    Controls {
        throttle,
        steer: (off * 2.5).clamp(-1.0, 1.0),
        boost: off.abs() < 0.08 && bend < 0.1 * pace,
    }
}

/// How far ahead, in meters, a racer pulls out round a vehicle in front of it, and how far out.
pub const SWERVE_AHEAD: f32 = 12.0;
pub const SWERVE: f32 = 3.5;

/// How far `vehicle` - the `n`th of `others`, where each is - pulls out of its line to get by one
/// just ahead of it: away from it, the side it is on (left positive).
pub fn swerve(vehicle: &Vehicle, n: usize, others: &[Vector3<f32>]) -> f32 {
    let ahead = vehicle.ahead();
    let left = Vector3::new(ahead.z, 0.0, -ahead.x);
    let mut out = 0.0;
    for (m, &at) in others.iter().enumerate() {
        if m == n {
            continue;
        }
        let to = at - vehicle.position;
        let (front, side) = (to.dot(&ahead), to.dot(&left));
        if front > 0.0 && front < SWERVE_AHEAD && side.abs() < 2.5 && to.y.abs() < 3.0 {
            out = if side > 0.0 { -SWERVE } else { SWERVE };
        }
    }
    out
}

/// Someone a vehicle in traffic stops for: another vehicle, or someone on foot.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Obstacle {
    Vehicle(Vector3<f32>),
    Foot(Vector3<f32>),
}

/// How far ahead a vehicle in traffic looks for someone in its way, in meters, how far to either
/// side of its line, and how far short of them it stops: a vehicle, a body's length and more;
/// anyone on foot, a little more.
pub const LOOK: f32 = 24.0;
pub const WIDE: f32 = 1.6;
pub const GAP_VEHICLE: f32 = 4.5;
pub const GAP_FOOT: f32 = 3.5;

/// How fast one at `at` going `ahead`, braking at `brake` m/s², can be going and still stop short
/// of everyone in its way of `obstacles`, and of every vehicle of them - each none if no one is. Kept waiting too long - `impatient` - it goes on round another vehicle, which would
/// otherwise wait for it for ever; never into anyone on foot.
pub fn room_ahead(
    at: Vector3<f32>,
    ahead: Vector3<f32>,
    obstacles: impl IntoIterator<Item = Obstacle>,
    brake: f32,
    impatient: bool,
) -> (Option<f32>, Option<f32>) {
    let right = Vector3::new(-ahead.z, 0.0, ahead.x);
    let (mut anyone, mut vehicles): (Option<f32>, Option<f32>) = (None, None);
    for obstacle in obstacles {
        let (there, gap, vehicle) = match obstacle {
            Obstacle::Vehicle(there) => (there, GAP_VEHICLE, true),
            Obstacle::Foot(there) => (there, GAP_FOOT, false),
        };
        if vehicle && impatient {
            continue;
        }
        let to = there - at;
        if to.y.abs() > 3.0 {
            continue;
        }
        let (front, side) = (to.dot(&ahead), to.dot(&right));
        if front > 0.0 && front < LOOK && side.abs() < WIDE {
            let slow = (2.0 * brake * (front - gap).max(0.0)).sqrt();
            anyone = Some(anyone.map_or(slow, |s| s.min(slow)));
            if vehicle {
                vehicles = Some(vehicles.map_or(slow, |s| s.min(slow)));
            }
        }
    }
    (anyone, vehicles)
}

/// How fast one can be going, easing off at `ease` m/s², to be going no faster than `then`
/// `left` meters on.
pub fn easing_for(then: f32, left: f32, ease: f32) -> f32 {
    (then * then + 2.0 * ease * left).sqrt()
}

/// Whether one going `speed`, easing off at `ease` m/s², stops for an amber light `left` meters
/// short of the line: if it can, without braking hard.
pub fn stops_on_amber(speed: f32, left: f32, ease: f32) -> bool {
    speed * speed / (2.0 * ease) < left + 0.5
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn in_traffic_it_stops_short_of_the_nearest_in_its_way_and_goes_round_a_vehicle_only_impatient() {
        let ahead = Vector3::z();
        let at = Vector3::zeros();
        let them = [Obstacle::Vehicle(Vector3::new(0.0, 0.0, 10.0)), Obstacle::Foot(Vector3::new(0.5, 0.0, 20.0))];
        let (anyone, vehicles) = room_ahead(at, ahead, them, 9.0, false);
        let short = (2.0 * 9.0 * (10.0 - GAP_VEHICLE)).sqrt();
        assert_eq!((anyone, vehicles), (Some(short), Some(short)));
        let (anyone, vehicles) = room_ahead(at, ahead, them, 9.0, true);
        assert!(anyone.is_some() && vehicles.is_none(), "impatient, it goes round the vehicle but not the one on foot");
        assert_eq!(room_ahead(at, ahead, [Obstacle::Foot(Vector3::new(5.0, 0.0, 10.0))], 9.0, false), (None, None), "off to the side");
        assert_eq!(room_ahead(at, ahead, [Obstacle::Foot(Vector3::new(0.0, 0.0, -5.0))], 9.0, false), (None, None), "behind");
    }

    #[test]
    fn a_racer_pulls_out_round_one_just_ahead_away_from_the_side_it_is_on() {
        let me = Vehicle { position: Vector3::zeros(), yaw: 0.0, speed: 30.0 };
        // Ahead and a little to the left: out to the right.
        assert_eq!(swerve(&me, 0, &[me.position, Vector3::new(1.0, 0.0, 8.0)]), -SWERVE);
        assert_eq!(swerve(&me, 0, &[me.position, Vector3::new(-1.0, 0.0, 8.0)]), SWERVE);
        assert_eq!(swerve(&me, 0, &[me.position, Vector3::new(0.0, 0.0, 30.0)]), 0.0);
    }

    #[test]
    fn on_amber_it_stops_only_if_it_can_without_braking_hard() {
        assert!(stops_on_amber(10.0, 20.0, 4.0));
        assert!(!stops_on_amber(15.0, 10.0, 4.0));
        assert_eq!(easing_for(6.0, 0.0, 4.0), 6.0);
    }
}
