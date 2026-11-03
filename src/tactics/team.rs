//! Fighting as a team. A side shares what any of it sees of the enemy ([`Intel`]), and with it:
//!
//! - picks its targets - whoever is shooting at it, hurt, or out in the open before whoever is
//!   merely nearest - and keeps to the one it has unless another is clearly better ([`target`]);
//! - knows which ground the enemy can see ([`Exposure`]), and keeps out of it as it moves;
//! - takes cover facing the threat, from the other enemies it knows of too, and not crowded by a
//!   teammate ([`cover_rank`], [`cover_worth`]);
//! - sends some round the side ([`flank_point`]); steps aside with no cover to be had
//!   ([`sidestep_to`]);
//! - leaves cover only while a teammate is firing to cover it ([`covered`]), and fires at where
//!   an enemy was just seen to keep their head down ([`Intel::recently_seen`]), a bolt that comes
//!   near ([`SUPPRESS_NEAR`]) keeping one down a moment ([`SUPPRESSED_FOR`]);
//! - and gives up on getting somewhere it is getting nowhere towards ([`Stuck`]).
//!
//! Whether anything is in the way is the game's to say, as everywhere in this crate.

use crate::{
    flat, forward, heading_of,
    navigation::{
        navmesh::{NavMesh, Poly},
        steer,
    },
};
use nalgebra::Vector3;
use std::collections::HashMap;

/// How long what a team saw of an enemy stays worth acting on, in seconds.
pub const INTEL_TIME: f32 = 8.0;
/// How often the team works out which ground its enemies can see, in seconds; how much dearer a
/// meter of ground one enemy can see counts, for a route; and how far off an enemy sees that far,
/// in meters.
pub const EXPOSURE_EVERY: f32 = 1.0;
pub const EXPOSURE_COST: f32 = 6.0;
pub const EXPOSURE_RANGE: f32 = 45.0;
/// How far round from the rest of its team a flanker goes, in radians, and how far from the
/// enemy it makes for, in meters.
pub const FLANK_ANGLE: f32 = 1.2;
pub const FLANK_DISTANCE: f32 = 14.0;
/// How near a teammate, in meters, makes cover crowded; and how much worse it counts cover that
/// is crowded, or seen by an enemy other than the one it is taken from, in the cover's terms.
pub const CROWDED: f32 = 3.0;
pub const CROWDED_COST: f32 = 4.0;
pub const SEEN_COST: f32 = 8.0;
/// How recently an enemy has to have been seen, in seconds, to fire at where it was; and how near
/// a bolt has to land or pass, in meters, to keep one down, and for how long, in seconds.
pub const SUPPRESS_SEEN: f32 = 3.0;
pub const SUPPRESS_NEAR: f32 = 2.0;
pub const SUPPRESSED_FOR: f32 = 1.5;
/// How far one has to get in how long, in meters and seconds, not to count as stuck.
pub const STUCK: (f32, f32) = (0.5, 3.0);
/// How far one caught without cover sidesteps, in meters.
pub const SIDESTEP: f32 = 2.5;
/// How near a teammate has to be, in meters, to cover one moving with its fire; and how often one
/// with none firing to cover it goes anyway, of the times it asks.
pub const COVERING_NEAR: f32 = 30.0;
pub const GO_ANYWAY: f32 = 0.25;

/// What a team knows of where its enemies are: for each, by index, where it was last seen and
/// how long ago, in seconds.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Intel(Vec<Option<(Vector3<f32>, f32)>>);

impl Intel {
    /// Knowing nothing yet of `enemies` of them.
    pub fn new(enemies: usize) -> Self {
        Self(vec![None; enemies])
    }

    /// Enemy `j` seen at `at`, just now.
    pub fn saw(&mut self, j: usize, at: Vector3<f32>) {
        if let Some(seen) = self.0.get_mut(j) {
            *seen = Some((at, 0.0));
        }
    }

    /// Enemy `j` gone: whatever was known of it forgotten.
    pub fn forget(&mut self, j: usize) {
        if let Some(seen) = self.0.get_mut(j) {
            *seen = None;
        }
    }

    /// All it knows a moment `dt` older.
    pub fn age(&mut self, dt: f32) {
        for (_, age) in self.0.iter_mut().flatten() {
            *age += dt;
        }
    }

    /// Those of the enemies that `alive` says are, seen within `within` seconds: by index, and
    /// where.
    pub fn seen_within<'a>(
        &'a self,
        within: f32,
        alive: impl Fn(usize) -> bool + 'a,
    ) -> impl Iterator<Item = (usize, Vector3<f32>)> + 'a {
        self.0
            .iter()
            .enumerate()
            .filter(move |&(j, _)| alive(j))
            .filter_map(move |(j, seen)| seen.filter(|&(_, age)| age < within).map(|(at, _)| (j, at)))
    }

    /// Where the enemies `alive` says are, as far as it is worth acting on (see [`INTEL_TIME`]).
    pub fn known(&self, alive: impl Fn(usize) -> bool) -> Vec<Vector3<f32>> {
        self.seen_within(INTEL_TIME, alive).map(|(_, at)| at).collect()
    }

    /// Where the nearest enemy to `me` is believed to be, of those `alive` says are.
    pub fn nearest(&self, me: Vector3<f32>, alive: impl Fn(usize) -> bool) -> Option<Vector3<f32>> {
        self.seen_within(INTEL_TIME, alive).map(|(_, at)| at).min_by(|a, b| (a - me).norm().total_cmp(&(b - me).norm()))
    }

    /// The nearest enemy to `me` seen lately enough to fire at where it was (see
    /// [`SUPPRESS_SEEN`]), and near enough ([`EXPOSURE_RANGE`]): which, and where it stood.
    pub fn recently_seen(&self, me: Vector3<f32>, alive: impl Fn(usize) -> bool) -> Option<(usize, Vector3<f32>)> {
        self.seen_within(SUPPRESS_SEEN, alive)
            .filter(|(_, at)| (at - me).norm() < EXPOSURE_RANGE)
            .min_by(|a, b| (a.1 - me).norm().total_cmp(&(b.1 - me).norm()))
    }
}

/// Which ground a team's enemies can see: for every polygon of the navmesh, how many of them could
/// see one standing in the middle of it.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Exposure(HashMap<Poly, u8>);

impl Exposure {
    /// Works it out over `mesh` for enemies standing at `enemies`: each that is within
    /// [`EXPOSURE_RANGE`] of the chest - `chest` meters over the floor - of one standing in the
    /// middle of a polygon, and that `sees` it there from where it stands.
    pub fn survey(mesh: &NavMesh, enemies: &[Vector3<f32>], chest: f32, sees: impl Fn(Vector3<f32>, Vector3<f32>) -> bool) -> Self {
        let mut seen = HashMap::new();
        if enemies.is_empty() {
            return Self(seen);
        }
        for n in 0..mesh.len() as Poly {
            if mesh.area_of(n) <= 0.0 {
                continue;
            }
            let at = mesh.centre(n) + Vector3::y() * chest;
            let seeing = enemies
                .iter()
                .filter(|&&enemy| (enemy - at).norm() < EXPOSURE_RANGE)
                .filter(|&&enemy| sees(enemy, at))
                .count();
            if seeing > 0 {
                seen.insert(n, seeing.min(255) as u8);
            }
        }
        Self(seen)
    }

    /// Whether it has anything in it: any enemies known of, seeing anywhere.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// How many of the enemies can see polygon `n`.
    pub fn at(&self, n: Poly) -> u8 {
        self.0.get(&n).copied().unwrap_or(0)
    }

    /// How much dearer a meter across polygon `n` is, for a route that keeps out of their sight.
    pub fn cost(&self, n: Poly) -> f32 {
        self.at(n) as f32 * EXPOSURE_COST
    }
}

/// How much one would rather not shoot at an enemy `distance` meters off with `health` of its
/// health left, from 0 to 1, that is `shooting_back` at it or not, and `in_the_open` or not: the
/// less, the sooner.
pub fn target_worth(distance: f32, health: f32, shooting_back: bool, in_the_open: bool) -> f32 {
    distance / 10.0 + health * 1.5 - if shooting_back { 1.5 } else { 0.0 } - if in_the_open { 0.8 } else { 0.0 }
}

/// Which of the `visible` enemies to shoot at, as `worth` says (see [`target_worth`]): the one it
/// is shooting at, `current`, unless another is clearly better.
pub fn target(visible: &[usize], current: Option<usize>, worth: impl Fn(usize) -> f32) -> Option<usize> {
    let best = visible.iter().copied().min_by(|&a, &b| worth(a).total_cmp(&worth(b)));
    match (current.filter(|j| visible.contains(j)), best) {
        (Some(current), Some(best)) if worth(current) <= worth(best) + 0.5 => Some(current),
        (_, best) => best,
    }
}

/// Whether cover whose sides face `shields` (along the ground, length 1) shields one there from
/// the way `to_threat` (along the ground, length 1).
pub fn shields_from(shields: &[Vector3<f32>], to_threat: Vector3<f32>) -> bool {
    shields.iter().any(|side| side.dot(&to_threat) > 0.92)
}

/// How good cover `cost` steps off is, `distance` meters from the threat - the less the better:
/// best fought from 10 to 22 m, or `retreat`ing, the further the better. None, too near it.
pub fn cover_rank(cost: f32, distance: f32, retreat: bool) -> Option<f32> {
    if distance < 4.0 {
        return None;
    }
    let range = if retreat { -distance } else { (distance - 16.0).abs() };
    Some(cost * 0.5 + range)
}

/// How good cover of `rank` (see [`cover_rank`]) is, `seen` by so many enemies other than the one
/// it is taken from, and `crowded` by a teammate within [`CROWDED`] or not - the less the better.
pub fn cover_worth(rank: f32, seen: usize, crowded: bool) -> f32 {
    rank + seen as f32 * SEEN_COST + if crowded { CROWDED_COST } else { 0.0 }
}

/// Where one at `me`, going round the `side` (1 or -1) against a `threat`, makes for: off to that
/// side of the line from the middle of its team, standing at `mates`, to the threat,
/// [`FLANK_DISTANCE`] from it, on the floor of `mesh` nearest there. None if it is
/// close enough already to go straight at them.
pub fn flank_point(
    mesh: &NavMesh,
    me: Vector3<f32>,
    mates: &[Vector3<f32>],
    threat: Vector3<f32>,
    side: f32,
) -> Option<Vector3<f32>> {
    let middle = mates.iter().sum::<Vector3<f32>>() / mates.len().max(1) as f32;
    let from_threat = heading_of(middle - threat)?;
    let point = threat + forward(from_threat + side * FLANK_ANGLE) * FLANK_DISTANCE;
    let there = mesh.nearest(point).map(|n| mesh.closest_point(n, point))?;
    (flat(there - me).norm() > 3.0).then_some(there)
}

/// Where one at `me`, with no cover to be had from `threat`, steps aside to, across the line from
/// it, to the `side` (1 or -1): on the floor of `mesh` there, or nearest it.
pub fn sidestep_to(
    mesh: &NavMesh,
    me: Vector3<f32>,
    threat: Vector3<f32>,
    side: f32,
) -> Option<Vector3<f32>> {
    let across = flat(threat - me).try_normalize(1.0e-3).map(steer::right_of)?;
    let to = me + across * side * SIDESTEP;
    mesh.nearest(to).map(|n| mesh.closest_point(n, to))
}

/// Whether one at `me` moves on from cover: with a teammate near enough of `mates` - each where it
/// is, and whether it is firing - firing to cover it, or none near enough to; or else
/// [`GO_ANYWAY`] of the times, as `roll`, from 0 to 1, says.
pub fn covered(me: Vector3<f32>, mates: impl IntoIterator<Item = (Vector3<f32>, bool)>, roll: f32) -> bool {
    let near: Vec<bool> = mates.into_iter().filter(|(at, _)| (at - me).norm() < COVERING_NEAR).map(|(_, firing)| firing).collect();
    near.is_empty() || near.contains(&true) || roll < GO_ANYWAY
}

/// Getting somewhere, or not: where it was when it last got anywhere, and how long ago.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Stuck {
    from: Vector3<f32>,
    since: f32,
}

impl Stuck {
    /// Starting afresh from `at`.
    pub fn at(at: Vector3<f32>) -> Self {
        Self { from: at, since: 0.0 }
    }

    /// Moves it on `dt` to one now `here`: whether it is stuck, having got no more than
    /// [`STUCK`]'s distance in its time - and starts afresh if so.
    pub fn step(&mut self, here: Vector3<f32>, dt: f32) -> bool {
        if flat(here - self.from).norm() > STUCK.0 {
            *self = Self::at(here);
            false
        } else if self.since + dt > STUCK.1 {
            *self = Self::at(here);
            true
        } else {
            self.since += dt;
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open(size: usize) -> NavMesh {
        let mut grid = crate::navigation::grid::WalkGrid::new(size, size, 1.0);
        for z in 0..size {
            for x in 0..size {
                grid.set(x, z, true);
            }
        }
        NavMesh::build(&grid, Vector3::zeros(), |_, _| 0)
    }

    #[test]
    fn a_team_knows_where_it_saw_its_enemies_for_a_while() {
        let mut intel = Intel::new(3);
        intel.saw(0, Vector3::new(10.0, 0.0, 0.0));
        intel.saw(2, Vector3::new(3.0, 0.0, 0.0));
        let alive = |_| true;
        assert_eq!(intel.nearest(Vector3::zeros(), alive), Some(Vector3::new(3.0, 0.0, 0.0)));
        assert_eq!(intel.recently_seen(Vector3::zeros(), alive).map(|(j, _)| j), Some(2));
        intel.age(SUPPRESS_SEEN + 0.1);
        assert_eq!(intel.recently_seen(Vector3::zeros(), alive), None, "too long ago to fire at");
        assert_eq!(intel.known(alive).len(), 2);
        assert_eq!(intel.nearest(Vector3::zeros(), |j| j != 2), Some(Vector3::new(10.0, 0.0, 0.0)));
        intel.forget(0);
        intel.age(INTEL_TIME);
        assert!(intel.known(alive).is_empty());
    }

    #[test]
    fn it_keeps_its_target_unless_another_is_clearly_better() {
        let worth = |j: usize| [1.0, 0.6, 0.2][j];
        assert_eq!(target(&[0, 1, 2], None, worth), Some(2));
        assert_eq!(target(&[0, 1, 2], Some(1), worth), Some(1));
        assert_eq!(target(&[0, 1, 2], Some(0), worth), Some(2));
        assert_eq!(target(&[0, 2], Some(1), worth), Some(2), "its target out of sight");
        assert!(target_worth(10.0, 1.0, true, false) < target_worth(10.0, 1.0, false, false));
        assert!(target_worth(10.0, 0.2, false, true) < target_worth(10.0, 1.0, false, false));
    }

    #[test]
    fn the_ground_the_enemy_sees_is_dear_to_cross() {
        let mesh = open(40);
        let enemy = Vector3::new(2.0, 1.5, 2.0);
        // It sees only the ground with z under 20.
        let exposure = Exposure::survey(&mesh, &[enemy], 1.2, |_, at| at.z < 20.0);
        let near = mesh.poly_at(Vector3::new(4.5, 0.0, 4.5)).unwrap();
        let far = mesh.poly_at(Vector3::new(4.5, 0.0, 32.5)).unwrap();
        assert_eq!(exposure.at(near), 1);
        assert_eq!(exposure.at(far), 0);
        assert_eq!(exposure.cost(near), EXPOSURE_COST);
        assert!(Exposure::default().is_empty() && !exposure.is_empty());
    }

    #[test]
    fn cover_faces_the_threat_at_a_good_range_and_not_where_others_see_it_or_a_mate_is() {
        assert!(shields_from(&[Vector3::z()], Vector3::z()) && !shields_from(&[Vector3::x()], Vector3::z()));
        assert_eq!(cover_rank(0.0, 3.0, false), None);
        assert!(cover_rank(4.0, 16.0, false) < cover_rank(4.0, 30.0, false));
        assert!(cover_rank(4.0, 30.0, true) < cover_rank(4.0, 16.0, true));
        assert!(cover_worth(1.0, 0, false) < cover_worth(1.0, 0, true));
        assert!(cover_worth(1.0, 0, true) < cover_worth(1.0, 1, false));
    }

    #[test]
    fn a_flanker_goes_round_the_side_and_one_without_cover_steps_aside() {
        let ground = &open(80);
        let threat = Vector3::new(40.0, 0.0, 60.0);
        let mates = [Vector3::new(38.0, 0.0, 20.0), Vector3::new(42.0, 0.0, 20.0)];
        let me = Vector3::new(40.0, 0.0, 20.0);
        let left = flank_point(ground, me, &mates, threat, 1.0).unwrap();
        let right = flank_point(ground, me, &mates, threat, -1.0).unwrap();
        assert!((flat(left - threat).norm() - FLANK_DISTANCE).abs() < 1.5);
        assert!((left.x - 40.0) * (right.x - 40.0) < 0.0, "either side: {left:?} {right:?}");
        // Already there: none.
        assert_eq!(flank_point(ground, left, &mates, threat, 1.0), None);
        let step = sidestep_to(ground, me, threat, 1.0).unwrap();
        assert!((flat(step - me).norm() - SIDESTEP).abs() < 1.0 && (step.z - me.z).abs() < 1.0);
    }

    #[test]
    fn it_moves_on_under_covering_fire_or_alone() {
        let me = Vector3::zeros();
        let near = Vector3::new(5.0, 0.0, 0.0);
        assert!(covered(me, [], 0.9));
        assert!(covered(me, [(near, true)], 0.9));
        assert!(!covered(me, [(near, false)], 0.9));
        assert!(covered(me, [(near, false)], 0.1), "now and then anyway");
        assert!(covered(me, [(Vector3::new(50.0, 0.0, 0.0), false)], 0.9), "none near enough");
    }

    #[test]
    fn getting_nowhere_for_a_while_it_is_stuck() {
        let mut stuck = Stuck::at(Vector3::zeros());
        assert!(!stuck.step(Vector3::zeros(), STUCK.1 - 0.5));
        assert!(stuck.step(Vector3::zeros(), 1.0));
        assert!(!stuck.step(Vector3::zeros(), 1.0), "afresh");
        assert!(!stuck.step(Vector3::new(1.0, 0.0, 0.0), STUCK.1), "it got somewhere");
    }
}
