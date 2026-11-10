//! Where the player keeps turning up: the places an NPC - or a faction - has seen them, and found
//! them hiding, for a search to look in first and a patrol to go by.
//!
//! [`Haunts`] keep each place the player was seen ([`Haunts::seen_at`]) or found hiding - found
//! after a search, counting the tuning's `found` times as much ([`Haunts::found_at`]) - by the
//! navmesh cell it is in, so that they hold however the navmesh is opened and shut. Each fades
//! ([`Haunts::step`]), and only the tuning's `most` are kept, the faintest let go first. Tell it of
//! the moments that matter - spotted, lost, found - not of every frame the player is in sight.
//!
//! Once the player is lost, a [`SearchMap`] has the chance of where they could be spread from
//! where they were lost. Leaning it on the haunts ([`Haunts::lean`]) weighs every polygon up for
//! the haunts within the tuning's `near` of it: by up to the tuning's `lean` again as much, half
//! that once the haunts round it count the tuning's `sure`. Searchers then go first to where the
//! player has hidden before - and only where they could have got to since, as the map has it:
//! a haunt across the level is no reason to look there now.

use crate::navigation::{grid::LEVEL, navmesh::{NavMesh, Poly}};
use crate::tactics::search::SearchMap;
use nalgebra::Vector3;

/// How many times as much being found hiding somewhere counts as being seen there: three.
pub const FOUND: f32 = 3.0;
/// How long, in seconds, a haunt takes to fade halfway: half an hour.
pub const HALF_LIFE: f32 = 1800.0;
/// How near a haunt, in meters along the ground, counts for somewhere, the nearer the more.
pub const NEAR: f32 = 4.0;
/// How far the haunts round somewhere weigh it up in a search: by up to as much again.
pub const LEAN: f32 = 1.0;
/// How much the haunts round somewhere have to count for it to be weighed up halfway.
pub const SURE: f32 = 2.0;
/// How much what an NPC is told counts for, against what it saw itself: a half.
pub const TOLD: f32 = 0.5;
/// How many haunts are kept at most.
pub const MOST: usize = 64;

tuning!(
    /// How an NPC learns where the player turns up (see the module's constants, its defaults).
    {
        /// How many times as much being found hiding somewhere counts as being seen there.
        found: f32 = FOUND,
        /// How long, in seconds, a haunt takes to fade halfway.
        half_life: f32 = HALF_LIFE,
        /// How near a haunt, in meters, counts for somewhere.
        near: f32 = NEAR,
        /// How far the haunts round somewhere weigh it up in a search.
        lean: f32 = LEAN,
        /// How much the haunts round somewhere count for it to be weighed up halfway.
        sure: f32 = SURE,
        /// How much what it is told counts for.
        told: f32 = TOLD,
        /// How many haunts are kept at most.
        most: usize = MOST,
    }
);

/// The places the player has turned up, each a navmesh cell - `(x, row)`, as
/// [`NavMesh::cell_at`] has them - and how much it counts for.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Haunts {
    haunts: Vec<((usize, usize), f32)>,
}

impl Haunts {
    /// Nowhere yet.
    pub fn new() -> Self {
        Self::default()
    }

    /// The player was seen at `at`: spotted there, or last seen there before being lost.
    pub fn seen_at(&mut self, tuning: &Tuning, mesh: &NavMesh, at: Vector3<f32>) {
        if let Some(cell) = mesh.cell_at(at) {
            self.count(tuning, cell, 1.0);
        }
    }

    /// The player was found hiding at `at`.
    pub fn found_at(&mut self, tuning: &Tuning, mesh: &NavMesh, at: Vector3<f32>) {
        if let Some(cell) = mesh.cell_at(at) {
            self.count(tuning, cell, tuning.found);
        }
    }

    /// It tells `to` where the player turns up: each haunt counted the tuning's `told` share as
    /// much.
    pub fn tell(&self, tuning: &Tuning, to: &mut Haunts) {
        for &(cell, weight) in &self.haunts {
            to.count(tuning, cell, weight * tuning.told.max(0.0));
        }
    }

    /// Moves it on `dt` seconds: each haunt fading, halfway in the tuning's half-life, and
    /// forgotten once next to nothing is left.
    pub fn step(&mut self, tuning: &Tuning, dt: f32) {
        for (_, weight) in &mut self.haunts {
            *weight = super::fade(*weight, tuning.half_life, dt);
        }
        self.haunts.retain(|(_, weight)| *weight > 1.0e-2);
    }

    /// Every haunt, where it is on the navmesh's floor and how much it counts for.
    pub fn haunts<'a>(&'a self, mesh: &'a NavMesh) -> impl Iterator<Item = (Vector3<f32>, f32)> + 'a {
        self.haunts.iter().map(|&(cell, weight)| (mesh.on_floor(cell), weight))
    }

    /// Where the player turns up most - for a patrol to go by - none if nowhere yet.
    pub fn likeliest(&self, mesh: &NavMesh) -> Option<Vector3<f32>> {
        self.haunts.iter().max_by(|a, b| a.1.total_cmp(&b.1)).map(|&(cell, _)| mesh.on_floor(cell))
    }

    /// How much the haunts round polygon `n` count for: each within the tuning's `near` of it,
    /// on its floor, the nearer the more.
    pub fn around(&self, tuning: &Tuning, mesh: &NavMesh, n: Poly) -> f32 {
        let near = tuning.near.max(1.0e-3);
        self.haunts
            .iter()
            .map(|&(cell, weight)| {
                let at = mesh.on_floor(cell);
                let to = mesh.closest_point(n, at);
                if (to.y - at.y).abs() > LEVEL {
                    return 0.0;
                }
                weight * (1.0 - (to.x - at.x).hypot(to.z - at.z) / near).max(0.0)
            })
            .sum()
    }

    /// How many times as worth searching polygon `n` is for the haunts round it: one, with none,
    /// up to one and the tuning's `lean`.
    pub fn weight(&self, tuning: &Tuning, mesh: &NavMesh, n: Poly) -> f32 {
        let around = self.around(tuning, mesh, n);
        1.0 + tuning.lean.max(0.0) * around / (around + tuning.sure.max(1.0e-3))
    }

    /// How much more a searcher should make of polygon `n`, for the haunts about it: from 1, none,
    /// up to one and the tuning's `lean`. Give it to [`SearchMap::best_by`], so that a searcher
    /// goes first where the player has turned up before, wherever the search has spread to.
    pub fn weigh<'a>(&'a self, tuning: &'a Tuning, mesh: &'a NavMesh) -> impl Fn(crate::navigation::navmesh::Poly) -> f32 + 'a {
        move |n| if self.haunts.is_empty() { 1.0 } else { self.weight(tuning, mesh, n) }
    }

    /// Leans `map` on the haunts: where the player could be, weighed up where they have turned up
    /// before (see [`SearchMap::lean`]). Only what is in `map` already is weighed: just after
    /// [`SearchMap::lose`], that is where they were lost and nothing else, so a haunt further off
    /// counts for nothing - use [`Haunts::weigh`] with [`SearchMap::best_by`] for that.
    pub fn lean(&self, tuning: &Tuning, mesh: &NavMesh, map: &mut SearchMap) {
        if !self.haunts.is_empty() {
            map.lean(|n| self.weight(tuning, mesh, n));
        }
    }

    fn count(&mut self, tuning: &Tuning, cell: (usize, usize), weight: f32) {
        if weight.is_nan() || weight <= 0.0 {
            return;
        }
        match self.haunts.iter_mut().find(|(c, _)| *c == cell) {
            Some((_, had)) => *had += weight,
            None => self.haunts.push((cell, weight)),
        }
        if self.haunts.len() > tuning.most {
            self.haunts.sort_by(|a, b| b.1.total_cmp(&a.1));
            self.haunts.truncate(tuning.most);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::navigation::grid::WalkGrid;

    const T: Tuning = Tuning::DEFAULT;

    /// A corridor 21 cells long, each pair of cells its own polygon.
    fn corridor() -> NavMesh {
        let mut grid = WalkGrid::new(21, 1, 1.0);
        for x in 0..21 {
            grid.set(x, 0, true);
        }
        NavMesh::build(&grid, Vector3::zeros(), |x, _| (x / 2) as u8)
    }

    fn at(x: f32) -> Vector3<f32> {
        Vector3::new(x + 0.5, 0.0, 0.5)
    }

    /// How much of the chance is in the cells from `from` to `to`.
    fn chance(map: &SearchMap, mesh: &NavMesh, from: usize, to: usize) -> f32 {
        let mut polys: Vec<Poly> = (from..=to).filter_map(|x| mesh.poly_at(at(x as f32))).collect();
        polys.dedup();
        polys.iter().map(|&n| map.chance(n)).sum()
    }

    #[test]
    fn a_search_looks_first_where_the_player_has_hidden_before() {
        let mesh = corridor();
        let lost = |haunts: &Haunts| {
            let mut map = SearchMap::new();
            map.lose(&mesh, at(10.0), None);
            map.spread(&mesh, 30.0);
            haunts.lean(&T, &mesh, &mut map);
            map
        };
        let blind = lost(&Haunts::new());
        let was = chance(&blind, &mesh, 0, 8) / chance(&blind, &mesh, 12, 20);
        // They were found hiding down the left end, twice.
        let mut haunts = Haunts::new();
        haunts.found_at(&T, &mesh, at(2.0));
        haunts.found_at(&T, &mesh, at(2.0));
        assert_eq!(haunts.likeliest(&mesh).map(|p| p.x), Some(2.5));
        let leaned = lost(&haunts);
        let (left, right) = (chance(&leaned, &mesh, 0, 8), chance(&leaned, &mesh, 12, 20));
        assert!(left / right > was * 1.2, "{left} {right}, against {was} not leaned");
        let total: f32 = (0..=20).filter_map(|x| mesh.poly_at(at(x as f32))).collect::<std::collections::BTreeSet<_>>().iter().map(|&n| leaned.chance(n)).sum();
        assert!((total - 1.0).abs() < 1.0e-4, "still the whole: {total}");
    }

    #[test]
    fn a_haunt_is_no_reason_to_look_where_they_could_not_have_got_to() {
        let mesh = corridor();
        let mut haunts = Haunts::new();
        haunts.found_at(&T, &mesh, at(0.0));
        let mut map = SearchMap::new();
        map.lose(&mesh, at(18.0), None);
        map.spread(&mesh, 3.0);
        let before = chance(&map, &mesh, 0, 4);
        haunts.lean(&T, &mesh, &mut map);
        assert_eq!(before, 0.0);
        assert_eq!(chance(&map, &mesh, 0, 4), 0.0);
        assert!((chance(&map, &mesh, 0, 20) - 1.0).abs() < 1.0e-4);
    }

    #[test]
    fn haunts_fade_are_told_on_and_only_the_strongest_are_kept() {
        let mesh = corridor();
        let mut haunts = Haunts::new();
        haunts.seen_at(&T, &mesh, at(5.0));
        let mut other = Haunts::new();
        haunts.tell(&T, &mut other);
        assert_eq!(other.haunts(&mesh).map(|(_, w)| w).collect::<Vec<_>>(), vec![TOLD]);
        haunts.step(&T, HALF_LIFE * 20.0);
        assert_eq!(haunts.likeliest(&mesh), None);
        let few = Tuning { most: 2, ..T };
        let mut haunts = Haunts::new();
        haunts.found_at(&few, &mesh, at(1.0));
        haunts.seen_at(&few, &mesh, at(9.0));
        haunts.found_at(&few, &mesh, at(15.0));
        let kept: Vec<f32> = haunts.haunts(&mesh).map(|(p, _)| p.x).collect();
        assert_eq!(kept, vec![1.5, 15.5], "the faintest let go");
    }
}
