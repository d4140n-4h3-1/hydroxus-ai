//! Parts in a chase: of several after someone, who runs after them and who goes to cut them off,
//! so that a group closes in from more than one side rather than trailing after them in a line,
//! each a copy of the one in front.
//!
//! [`parts`] gives each its part from what each knows - its own idea of where they are and which
//! way they are going, not a picture they all share:
//!
//! - the nearest few ([`PURSUERS`]) run after them, nearness counted the more for one already
//!   doing so ([`KEEP_BY`]), so that parts don't swap back and forth as distances wobble, and for
//!   one quick to go after someone (its [`crate::tactics::priority::Traits::aggression`],
//!   [`AGGRESSION`]);
//! - one that sees them close by ([`SEES_CLOSE`]) runs after them whatever it was given to do: it
//!   doesn't let them by for a plan;
//! - one with no idea which way they are going has nothing to cut off, and runs after them;
//! - and the rest cut them off ([`cut_spot`]): ahead of them, the way they are going, as
//!   [`crate::tactics::chase::cut_off`] has it, each of them a little to one side or the other of
//!   the last ([`SPREAD`]), so that two cutting them off do not make for the same spot.
//!
//! Whether the floor carries on to there is the game's to say.

use crate::{right_of, tactics::chase};
use nalgebra::Vector3;

/// How many of those after someone run after them.
pub const PURSUERS: usize = 1;
/// How many meters nearer one already running after them counts itself.
pub const KEEP_BY: f32 = 3.0;
/// How near one that sees them runs after them whatever its part, in meters.
pub const SEES_CLOSE: f32 = 6.0;
/// How many meters nearer aggression all one way counts one, and further all the other way.
pub const AGGRESSION: f32 = 6.0;
/// How far to the side of the last one cutting them off each other makes for, in meters.
pub const SPREAD: f32 = 4.0;

tuning!(
    /// Who runs after someone and who cuts them off (see the module's constants, its defaults).
    {
        /// How many run after them.
        pursuers: usize = PURSUERS,
        /// How many meters nearer one already running after them counts itself.
        keep_by: f32 = KEEP_BY,
        /// How near one that sees them runs after them whatever its part, in meters.
        sees_close: f32 = SEES_CLOSE,
        /// How many meters nearer aggression all one way counts one.
        aggression: f32 = AGGRESSION,
        /// How far to the side of each other those cutting them off make for, in meters.
        spread: f32 = SPREAD,
    }
);

/// What one of those after someone does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Part {
    /// Runs after them.
    Pursue,
    /// Makes for ahead of them, to cut them off: the `slot`th of those doing so, 0 straight
    /// ahead of them and each after a little further to one side or the other.
    CutOff { slot: u8 },
}

/// One of those after someone, as it knows it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Chaser {
    /// Where it stands.
    pub feet: Vector3<f32>,
    /// Where it has them, and which way it has them going - none, not going anywhere it can tell.
    pub them: Vector3<f32>,
    pub going: Option<Vector3<f32>>,
    /// Whether it sees them itself just now.
    pub sees: bool,
    /// How quick it is to go after someone, from 0 to 1 (0.5, neither way).
    pub aggression: f32,
    /// Its part last time, if it had one.
    pub was: Option<Part>,
}

impl Chaser {
    fn away(&self) -> f32 {
        let away = (self.them - self.feet).xz().norm();
        if away.is_finite() {
            away
        } else {
            f32::INFINITY
        }
    }
}

/// Each of `chasers`' part, in order.
pub fn parts(tuning: &Tuning, chasers: &[Chaser]) -> Vec<Part> {
    // How near each counts itself.
    let standing: Vec<f32> = chasers
        .iter()
        .map(|c| {
            let aggression = if c.aggression.is_finite() { c.aggression.clamp(0.0, 1.0) } else { 0.5 };
            let keep = if c.was == Some(Part::Pursue) { tuning.keep_by } else { 0.0 };
            c.away() - (aggression - 0.5) * tuning.aggression - keep
        })
        .collect();
    let mut order: Vec<usize> = (0..chasers.len()).collect();
    order.sort_by(|&a, &b| standing[a].total_cmp(&standing[b]).then(a.cmp(&b)));
    let mut parts = vec![Part::Pursue; chasers.len()];
    let mut slot = 0u8;
    for (rank, &i) in order.iter().enumerate() {
        let c = &chasers[i];
        let own_call = c.sees && c.away() <= tuning.sees_close;
        let knows_where_going = c.going.is_some_and(|g| g.iter().all(|v| v.is_finite()) && g.xz().norm() > 1.0e-3);
        if rank >= tuning.pursuers && !own_call && knows_where_going {
            parts[i] = Part::CutOff { slot };
            slot = slot.saturating_add(1);
        }
    }
    parts
}

/// Where `chaser`, cutting them off as `part` says, makes for: ahead of them as
/// [`chase::cut_off`] has it, to one side or the other by its slot - none if the floor does not
/// carry on to there (`floor_at`), when it had best run after them. None, for one pursuing.
pub fn cut_spot(tuning: &Tuning, chase: &chase::Tuning, chaser: &Chaser, part: Part, floor_at: impl Fn(Vector3<f32>) -> bool) -> Option<Vector3<f32>> {
    let Part::CutOff { slot } = part else { return None };
    let ahead = chase::cut_off(chase, chaser.them, chaser.going, chaser.away(), false, &floor_at)?;
    let way = chaser.going?.try_normalize(1.0e-3)?;
    let side = right_of(way.x.atan2(way.z));
    let steps = slot.div_ceil(2) as f32;
    let sign = if slot % 2 == 1 { 1.0 } else { -1.0 };
    let spot = ahead + side * steps * sign * tuning.spread;
    if slot == 0 {
        return Some(ahead);
    }
    // Off the floor to that side: straight ahead with the first, rather than nowhere.
    Some(if floor_at(spot) { spot } else { ahead })
}

#[cfg(test)]
mod tests {
    use super::*;

    const T: Tuning = Tuning::DEFAULT;
    const C: chase::Tuning = chase::Tuning::DEFAULT;

    fn at(x: f32, z: f32) -> Vector3<f32> {
        Vector3::new(x, 0.0, z)
    }

    fn chaser(x: f32, z: f32) -> Chaser {
        Chaser { feet: at(x, z), them: at(0.0, 0.0), going: Some(at(1.0, 0.0)), sees: false, aggression: 0.5, was: None }
    }

    #[test]
    fn the_nearest_runs_after_them_and_the_rest_cut_them_off_each_to_its_own_side() {
        let chasers = [chaser(-20.0, 0.0), chaser(-5.0, 0.0), chaser(-15.0, 10.0)];
        let p = parts(&T, &chasers);
        assert_eq!(p, vec![Part::CutOff { slot: 1 }, Part::Pursue, Part::CutOff { slot: 0 }]);
        let floor = |_: Vector3<f32>| true;
        let a = cut_spot(&T, &C, &chasers[0], p[0], floor).unwrap();
        let c = cut_spot(&T, &C, &chasers[2], p[2], floor).unwrap();
        assert!(a.x > 0.0 && c.x > 0.0, "ahead of them: {a:?} {c:?}");
        assert!((a - c).norm() >= SPREAD - 1.0e-3, "not the same spot: {a:?} {c:?}");
        assert_eq!(cut_spot(&T, &C, &chasers[1], p[1], floor), None);
    }

    #[test]
    fn one_already_running_after_them_keeps_to_it_till_another_is_clearly_nearer() {
        let mut a = chaser(-10.0, 0.0);
        a.was = Some(Part::Pursue);
        let b = chaser(-8.0, 0.0);
        assert_eq!(parts(&T, &[a, b])[0], Part::Pursue, "2 m nearer is not clearly nearer");
        let b = chaser(-6.0, 0.0);
        assert_eq!(parts(&T, &[a, b])[1], Part::Pursue, "4 m is");
    }

    #[test]
    fn one_close_by_that_sees_them_runs_after_them_whatever_and_the_keen_sooner() {
        let mut close = chaser(-5.5, 0.0);
        let near = chaser(-3.0, 0.0);
        assert_ne!(parts(&T, &[close, near])[0], Part::Pursue);
        close.sees = true;
        assert_eq!(parts(&T, &[close, near]), vec![Part::Pursue, Part::Pursue]);
        // A keen one a little further off runs after them where a careful one would not.
        let mut keen = chaser(-9.0, 0.0);
        keen.aggression = 1.0;
        let mut careful = chaser(-7.0, 0.0);
        careful.aggression = 0.0;
        assert_eq!(parts(&T, &[keen, careful])[0], Part::Pursue);
    }

    #[test]
    fn with_no_idea_which_way_they_are_going_all_run_after_them() {
        let mut a = chaser(-20.0, 0.0);
        let mut b = chaser(-5.0, 0.0);
        a.going = None;
        b.going = Some(at(0.0, 0.0));
        assert_eq!(parts(&T, &[a, b]), vec![Part::Pursue, Part::Pursue]);
        a.them = at(f32::NAN, 0.0);
        assert_eq!(parts(&T, &[a]), vec![Part::Pursue]);
        assert!(parts(&T, &[]).is_empty());
    }

    #[test]
    fn where_the_floor_does_not_carry_on_there_is_no_cutting_them_off() {
        let chasers = [chaser(-5.0, 0.0), chaser(-20.0, 0.0), chaser(-21.0, 3.0)];
        let p = parts(&T, &chasers);
        assert_eq!(cut_spot(&T, &C, &chasers[1], p[1], |at: Vector3<f32>| at.x < 1.0), None);
        // Only a strip along their way: the second keeps straight ahead with the first.
        let strip = |at: Vector3<f32>| at.z.abs() < 1.0;
        assert_eq!(cut_spot(&T, &C, &chasers[2], p[2], strip), cut_spot(&T, &C, &chasers[1], p[1], strip));
    }
}
