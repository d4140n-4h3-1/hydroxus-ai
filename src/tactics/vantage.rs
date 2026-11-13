//! High ground: somewhere up over a spot to look down on it from - a roof, a walkway, a tower's
//! top - that an NPC on foot can get to.
//!
//! Up there it sees over what would stop its eye on the ground, and further (see
//! [`Sight::height_reach`](crate::perception::sight::Sight::height_reach)). Searching, it climbs
//! up to look out over where someone was lost; fighting, it takes it to shoot down from.

use crate::{
    flat,
    navigation::navmesh::{NavMesh, Poly},
};
use nalgebra::Vector3;

/// Where high ground is worth going to.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(default, deny_unknown_fields))]
pub struct Tuning {
    /// How far up over the spot it is to look out over, in meters, for it to be high ground at
    /// all; and how much higher counts for any more - any higher, it only looks further down.
    pub rise: f32,
    pub most_rise: f32,
    /// How far from the spot, along the ground, it may be, in meters; and how far a walk away.
    pub near: f32,
    pub reach: f32,
    /// How far apart, in meters, two looking out from high ground stand at least.
    pub apart: f32,
    /// What a meter's walk to it, and a meter off the spot, take from a meter higher.
    pub per_walk: f32,
    pub per_off: f32,
    /// Fighting, how much better cover is - in the meters of [`team::cover_rank`] - for each
    /// meter it is up over the threat, up to [`Tuning::most_rise`].
    ///
    /// [`team::cover_rank`]: crate::tactics::team::cover_rank
    pub cover_per_meter: f32,
}

impl Tuning {
    pub const DEFAULT: Self = Self {
        rise: 3.0,
        most_rise: 12.0,
        near: 25.0,
        reach: 60.0,
        apart: 6.0,
        per_walk: 0.05,
        per_off: 0.2,
        cover_per_meter: 1.5,
    };
}

impl Default for Tuning {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// The high ground - the point on it nearest the spot - that someone at `feet` can walk to to
/// look out over `over`, best first by how high it is, less how far off the spot and how long
/// a walk: within [`Tuning::near`] of the spot and [`Tuning::reach`] of a walk, at least
/// [`Tuning::rise`] up over it, and [`Tuning::apart`] from any of `taken`. None, with none.
pub fn high_ground(
    tuning: &Tuning,
    mesh: &NavMesh,
    feet: Vector3<f32>,
    over: Vector3<f32>,
    taken: &[Vector3<f32>],
) -> Option<Vector3<f32>> {
    ranked(tuning, mesh, feet, over, taken).into_iter().next().map(|(_, at)| at)
}

/// Every high ground [`high_ground`] would choose from, each with its worth, best first.
pub fn ranked(
    tuning: &Tuning,
    mesh: &NavMesh,
    feet: Vector3<f32>,
    over: Vector3<f32>,
    taken: &[Vector3<f32>],
) -> Vec<(f32, Vector3<f32>)> {
    let mut found: Vec<(f32, Vector3<f32>)> = mesh
        .costs_from(feet, tuning.reach, |_: Poly| 0.0)
        .into_iter()
        .filter_map(|(n, cost)| {
            if mesh.is_blocked(n) {
                return None;
            }
            let rise = mesh.floor_of(n) - over.y;
            if rise < tuning.rise {
                return None;
            }
            let at = mesh.closest_point(n, over);
            let off = flat(at - over).norm();
            if off > tuning.near || taken.iter().any(|t| flat(t - at).norm() < tuning.apart) {
                return None;
            }
            Some((worth(tuning, rise, off, cost), at))
        })
        .collect();
    found.sort_by(|a, b| b.0.total_cmp(&a.0));
    found
}

/// How much high ground `rise` meters up over a spot, `off` meters from it and `walk` meters'
/// walk away is worth: the higher the better, up to [`Tuning::most_rise`], less for the walk and
/// for how far off.
pub fn worth(tuning: &Tuning, rise: f32, off: f32, walk: f32) -> f32 {
    rise.min(tuning.most_rise) - tuning.per_walk * walk - tuning.per_off * off
}

/// How much better cover `rise` meters up over the threat is than on its level, to take off its
/// rank (see [`crate::tactics::team::cover_rank`]): nothing below it.
pub fn cover_lift(tuning: &Tuning, rise: f32) -> f32 {
    tuning.cover_per_meter * rise.clamp(0.0, tuning.most_rise)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn higher_is_better_up_to_a_point_and_nearer() {
        let t = Tuning::DEFAULT;
        assert!(worth(&t, 8.0, 5.0, 20.0) > worth(&t, 4.0, 5.0, 20.0), "higher");
        assert_eq!(worth(&t, 12.0, 5.0, 20.0), worth(&t, 40.0, 5.0, 20.0), "no better past most_rise");
        assert!(worth(&t, 6.0, 5.0, 10.0) > worth(&t, 6.0, 20.0, 10.0), "nearer the spot");
        assert!(worth(&t, 6.0, 5.0, 10.0) > worth(&t, 6.0, 5.0, 50.0), "a shorter walk");
    }

    #[test]
    fn cover_up_high_is_better_cover() {
        let t = Tuning::DEFAULT;
        assert_eq!(cover_lift(&t, -3.0), 0.0, "below the threat, no better");
        assert!(cover_lift(&t, 4.0) > 5.0, "four meters up beats six nearer the best range");
        assert_eq!(cover_lift(&t, 50.0), cover_lift(&t, t.most_rise));
    }
}
