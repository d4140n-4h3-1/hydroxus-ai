//! Racing: a rider steering a bike round a circuit of gates, as a racer does - flat out where the
//! way on is straight, easing off and braking into the turns, over the speed pads just ahead of
//! it, and pulling out round a bike in front.
//!
//! The game moves the bike - how it handles, how fast it can go, what it hits - and says where it
//! is; this says what the rider asks of it ([`Controls`]).

use nalgebra::Vector3;

/// How far ahead a rider sees a speed pad and makes for it, and how far off its line it goes to.
pub const PAD_SEEN: f32 = 45.0;
pub const PAD_DETOUR: f32 = 6.0;
/// How far ahead a rider looks for a bike in its way, and how far it pulls out to get by one.
pub const TRAFFIC_AHEAD: f32 = 12.0;
pub const TRAFFIC_SWERVE: f32 = 3.5;

/// What the rider asks of the bike this frame.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Controls {
    /// Forward (1) or brake and back (-1).
    pub throttle: f32,
    /// Left (1) or right (-1).
    pub steer: f32,
    pub boost: bool,
}

/// The circuit as a rider goes by it.
#[derive(Debug, Clone, Copy)]
pub struct Course<'a> {
    /// The gates round it in order: a lap is all of them.
    pub gates: &'a [Vector3<f32>],
    /// The speed pads: the middle of each, and the way it kicks along the ground (length 1).
    pub pads: &'a [(Vector3<f32>, Vector3<f32>)],
    /// Half a pad's length along the way it kicks, and how far above or below it still counts.
    pub pad_half: f32,
    pub pad_height: f32,
}

/// A bike as its rider has it: where it is, which way it points (a yaw, left positive from the
/// world's +z), how fast it is going the way it points, and how fast it can go at most.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bike {
    pub at: Vector3<f32>,
    pub yaw: f32,
    pub speed: f32,
    pub top_speed: f32,
}

impl Bike {
    /// Which way it points, along the ground.
    pub fn ahead(&self) -> Vector3<f32> {
        Vector3::new(self.yaw.sin(), 0.0, self.yaw.cos())
    }
}

/// The way from `from` to `to`, as a yaw: left positive from the world's +z.
pub fn yaw_towards(from: Vector3<f32>, to: Vector3<f32>) -> f32 {
    (to.x - from.x).atan2(to.z - from.z)
}

/// `angle` brought round into -π..π.
fn wrap(angle: f32) -> f32 {
    let tau = std::f32::consts::TAU;
    (angle + std::f32::consts::PI).rem_euclid(tau) - std::f32::consts::PI
}

/// Steers `bike` for the gates of `course` from the `next` on, `line` meters off their middles
/// (left positive): flat out where the way on is straight, easing off and braking into the turns
/// - the harder `pace` is over 1, the less - and over a speed pad just ahead, not far off its line
/// and pointing its way.
pub fn autopilot(bike: &Bike, course: &Course<'_>, next: usize, line: f32, pace: f32) -> Controls {
    let gates = course.gates;
    let n = gates.len();
    let near = gates[next % n];
    let far = gates[(next + 1) % n];
    // Its line: off the middle of the road, square across the way on.
    let way = (far - near).try_normalize(1e-5).unwrap_or_else(Vector3::z);
    let left = Vector3::new(way.z, 0.0, -way.x);
    let (near_on, far_on) = (near + left * line, far + left * line);
    // Through the next gate towards the one after, a little, the nearer it is.
    let to_near = Vector3::new(near_on.x - bike.at.x, 0.0, near_on.z - bike.at.z).norm();
    let blend = (1.0 - to_near / 25.0).clamp(0.0, 1.0);
    let mut aim = near_on + (far_on - near_on).try_normalize(1e-5).unwrap_or_default() * 8.0 * blend;
    // A speed pad just ahead, not far off its line and pointing its way: over it.
    let heading = bike.ahead();
    let side = Vector3::new(heading.z, 0.0, -heading.x);
    let pad = course
        .pads
        .iter()
        .filter(|&&(at, way)| {
            let to = at - bike.at;
            let front = to.dot(&heading);
            front > course.pad_half
                && front < PAD_SEEN
                && to.dot(&side).abs() < PAD_DETOUR
                && to.y.abs() < course.pad_height * 2.0
                && way.dot(&heading) > 0.8
        })
        .min_by(|a, b| (a.0 - bike.at).norm().total_cmp(&(b.0 - bike.at).norm()));
    if let Some(&(at, way)) = pad {
        // Through its middle along it, rather than across it.
        aim = at + way * course.pad_half;
    }
    let off = wrap(yaw_towards(bike.at, aim) - bike.yaw);
    // How sharp the way on turns at the next gate, and at the one after it, as the bike nears
    // each: slow enough for the sharper, in time to make it.
    let after = gates[(next + 2) % n];
    let at_near = wrap(yaw_towards(near, far) - yaw_towards(bike.at, near)).abs();
    let at_far = wrap(yaw_towards(far, after) - yaw_towards(near, far)).abs();
    let to_far = to_near + (far - near).norm();
    let soon = |bend: f32, distance: f32| bend * (1.0 - distance / 70.0).clamp(0.0, 1.0);
    let bend = soon(at_near, to_near).max(soon(at_far, to_far));
    let speed = bike.speed;
    let wanted = bike.top_speed * (1.0 - bend * 0.9 / pace.max(0.1)).clamp(0.3, 1.0);
    // Pointed well off, it slows and comes round at a crawl, rather than backing up - which
    // would steer it the other way.
    let throttle = if off.abs() > 1.2 {
        if speed > 10.0 { -1.0 } else { 0.4 }
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

/// How far the rider of bike `n` of `bikes` (where each is and which way it points) pulls out of
/// its line to get by a bike just ahead of it: away from it, the side it is on.
pub fn swerve(n: usize, bikes: &[(Vector3<f32>, Vector3<f32>)]) -> f32 {
    let (position, ahead) = bikes[n];
    let left = Vector3::new(ahead.z, 0.0, -ahead.x);
    let mut swerve = 0.0;
    for (m, &(at, _)) in bikes.iter().enumerate() {
        if m == n {
            continue;
        }
        let to = at - position;
        let (front, side) = (to.dot(&ahead), to.dot(&left));
        if front > 0.0 && front < TRAFFIC_AHEAD && side.abs() < 2.5 && to.y.abs() < 3.0 {
            swerve = if side > 0.0 { -TRAFFIC_SWERVE } else { TRAFFIC_SWERVE };
        }
    }
    swerve
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bike(x: f32, z: f32) -> Bike {
        Bike { at: Vector3::new(x, 0.0, z), yaw: 0.0, speed: 20.0, top_speed: 38.0 }
    }

    const GATES: [Vector3<f32>; 2] = [Vector3::new(0.0, 0.0, 200.0), Vector3::new(0.0, 0.0, 0.0)];

    #[test]
    fn it_makes_for_a_speed_pad_just_ahead_of_it() {
        // A pad 40 m on, a little to the left (+x), kicking straight on.
        let pads = [(Vector3::new(4.0, 0.0, 40.0), Vector3::z())];
        let course = Course { gates: &GATES, pads: &pads, pad_half: 4.0, pad_height: 2.0 };
        let plain = Course { pads: &[], ..course };
        assert!(autopilot(&bike(0.0, 10.0), &course, 0, 0.0, 1.0).steer > 0.1);
        assert!(autopilot(&bike(0.0, 10.0), &plain, 0, 0.0, 1.0).steer.abs() < 0.01);
        // Too far off its line, it goes for the gate as if there were no pad.
        let far = bike(-8.0, 10.0);
        assert_eq!(autopilot(&far, &course, 0, 0.0, 1.0), autopilot(&far, &plain, 0, 0.0, 1.0));
    }

    #[test]
    fn flat_out_on_the_straight_and_braking_for_a_hairpin() {
        let course = Course { gates: &GATES, pads: &[], pad_half: 4.0, pad_height: 2.0 };
        let straight = autopilot(&bike(0.0, 10.0), &course, 0, 0.0, 1.0);
        assert_eq!(straight.throttle, 1.0);
        // Nearly at the gate, the way on turning right round: off the throttle, onto the brake.
        let fast = Bike { speed: 38.0, ..bike(0.0, 190.0) };
        assert!(autopilot(&fast, &course, 0, 0.0, 1.0).throttle < 0.0);
    }

    #[test]
    fn it_pulls_out_round_a_bike_just_in_front_away_from_its_side() {
        let ahead = Vector3::z();
        // One just in front, a little to the left: out to the right.
        let bikes = [(Vector3::zeros(), ahead), (Vector3::new(1.0, 0.0, 6.0), ahead)];
        assert_eq!(swerve(0, &bikes), -TRAFFIC_SWERVE);
        // The one in front pays the one behind no heed.
        assert_eq!(swerve(1, &bikes), 0.0);
    }
}
