//! Searching for someone lost from sight: where they could be by now, and who looks where.
//!
//! A [`SearchMap`] holds, for every polygon of the navmesh, the chance that the one being looked
//! for is there. It starts with all of it where they were lost - some of it a little way on, the way
//! they were going - and spreads it along the ground as fast as they could go ([`SearchMap::spread`]),
//! so that the longer they have been gone, the more ground they could be in. Whatever a searcher
//! can see, and does not see them in, is cleared ([`SearchMap::look`]): they are not there, and the
//! rest of the chance is where they are. Each searcher then makes for the most likely ground it
//! can get to soon, keeping clear of where the others are going ([`SearchMap::best`]), so that
//! several searchers spread out rather than all look in the same place.
//!
//! Compared with each searcher wandering off to somewhere at random, it finds a player who has
//! run off and hidden more often: in 12 seconds, 88% of the time against 83% - see the test at
//! the bottom.

use crate::navigation::{
    grid::LEVEL,
    navmesh::{NavMesh, Poly},
};
use nalgebra::Vector3;
use std::collections::HashMap;

/// How much of its chance a polygon keeps each step it spreads, the rest going to its neighbours
/// - all of the rest, from one no wider than a step; less, the wider it is.
const KEEP: f32 = 0.2;
/// How far one step of spreading goes, in meters.
const STEP: f32 = 1.0;
/// How much of the chance goes a little way on, the way they were going, when they are lost, and
/// how far on, in meters.
const AHEAD: f32 = 0.6;
const AHEAD_BY: f32 = 4.0;
/// How far, in meters walked, halves how much somewhere is worth searching.
const HALVED_AFTER: f32 = 20.0;
/// How far from where they were lost, in meters of walking, the map follows them at most.
const FOLLOWED: f32 = 250.0;

/// Where someone lost from sight could be: a chance for every polygon of the navmesh they could
/// have got to.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SearchMap {
    /// The chance in each polygon with any.
    chance: HashMap<Poly, f32>,
    /// How far each polygon is to walk from where they were lost - as far as they could go.
    from_lost: HashMap<Poly, f32>,
    /// How far they could have gone since, and how much of that has not been spread yet.
    gone: f32,
    owed: f32,
}

impl SearchMap {
    /// A map with nobody lost on it.
    pub fn new() -> Self {
        Self::default()
    }

    /// They have just been lost at `at`, going `going` - a way along the ground, if it is known:
    /// they are there, or a little way on that way, on the same floor and in sight of where they
    /// were.
    pub fn lose(&mut self, mesh: &NavMesh, at: Vector3<f32>, going: Option<Vector3<f32>>) {
        *self = Self::default();
        let Some(here) = mesh.nearest(at) else {
            return;
        };
        self.from_lost = mesh.costs_from(at, FOLLOWED, |_| 0.0);
        self.from_lost.insert(here, 0.0);
        let ahead = going
            .and_then(|going| Vector3::new(going.x, 0.0, going.z).try_normalize(1.0e-3))
            .map(|way| at + way * AHEAD_BY)
            .filter(|&ahead| mesh.sees_across(at, ahead))
            .and_then(|ahead| mesh.poly_at(ahead));
        match ahead {
            Some(ahead) => {
                *self.chance.entry(ahead).or_default() += AHEAD;
                *self.chance.entry(here).or_default() += 1.0 - AHEAD;
            }
            None => {
                self.chance.insert(here, 1.0);
            }
        }
    }

    /// Whether there is still anywhere left they could be.
    pub fn is_empty(&self) -> bool {
        self.total() <= 1.0e-6
    }

    /// The chance that they are in polygon `n`.
    pub fn chance(&self, n: Poly) -> f32 {
        self.chance.get(&n).copied().unwrap_or(0.0)
    }

    /// The chance that they are in the polygon `at` is in.
    pub fn chance_at(&self, mesh: &NavMesh, at: Vector3<f32>) -> f32 {
        mesh.poly_at(at).map_or(0.0, |n| self.chance(n))
    }

    /// They could have gone `meters` further along the ground since this was last asked.
    pub fn spread(&mut self, mesh: &NavMesh, meters: f32) {
        self.owed += meters.max(0.0);
        while self.owed >= STEP {
            self.owed -= STEP;
            self.gone += STEP;
            self.step(mesh);
        }
    }

    /// One step further: each polygon keeps some of its chance and gives the rest out among those
    /// it leads into that they could have got to by now, by how wide the way into each is.
    fn step(&mut self, mesh: &NavMesh) {
        let had: Vec<(Poly, f32)> = self.chance.drain().collect();
        for (n, chance) in had {
            let open: Vec<(Poly, f32)> = mesh
                .portals(n)
                .iter()
                .filter(|p| self.from_lost.get(&p.to).is_some_and(|&d| d <= self.gone))
                .map(|p| (p.to, (p.a[0] - p.b[0]).hypot(p.a[1] - p.b[1])))
                .collect();
            let width: f32 = open.iter().map(|(_, w)| w).sum();
            if open.is_empty() || width <= 0.0 {
                *self.chance.entry(n).or_default() += chance;
                continue;
            }
            let across = mesh.area_of(n).sqrt().max(STEP);
            let leaving = chance * (1.0 - KEEP) * (STEP / across).min(1.0);
            *self.chance.entry(n).or_default() += chance - leaving;
            for (to, w) in open {
                *self.chance.entry(to).or_default() += leaving * w / width;
            }
        }
    }

    /// A searcher at `from`, facing `heading`, sees as far as `reach` meters, and as far as `cone`
    /// either side of ahead - and all round, as near as `near` meters - along its own floor, and
    /// does not see them: as much of each polygon as it sees is cleared, and the rest of the
    /// chance made up to the whole again. How much of it that cleared, before it was made up.
    pub fn look(&mut self, mesh: &NavMesh, from: Vector3<f32>, heading: f32, (reach, cone, near): (f32, f32, f32)) -> f32 {
        let total = self.total();
        if total <= 0.0 {
            return 0.0;
        }
        let ahead = Vector3::new(heading.sin(), 0.0, heading.cos());
        let mut cleared = 0.0;
        for (&n, chance) in self.chance.iter_mut() {
            let floor = mesh.floor_of(n);
            if (floor - from.y).abs() > LEVEL {
                continue;
            }
            // Its middle, and in from each corner.
            let p = mesh.polygon(n);
            let cs = mesh.cell_size;
            let (x0, z0, x1, z1) = (p.x0 as f32 * cs, p.z0 as f32 * cs, p.x1 as f32 * cs, p.z1 as f32 * cs);
            let at = |fx: f32, fz: f32| mesh.origin + Vector3::new(x0 + (x1 - x0) * fx, floor, z0 + (z1 - z0) * fz);
            let points = [at(0.5, 0.5), at(0.2, 0.2), at(0.8, 0.2), at(0.2, 0.8), at(0.8, 0.8)];
            let seen = points
                .iter()
                .filter(|&&point| {
                    let to = Vector3::new(point.x - from.x, 0.0, point.z - from.z);
                    let distance = to.norm();
                    distance <= reach
                        && (distance <= near || (to.dot(&ahead) / distance.max(1.0e-3)).clamp(-1.0, 1.0).acos() <= cone)
                        && mesh.sees_across(from, point)
                })
                .count();
            let gone = *chance * seen as f32 / points.len() as f32;
            cleared += gone;
            *chance -= gone;
        }
        self.chance.retain(|_, chance| *chance > 1.0e-7);
        let left = total - cleared;
        if left > 1.0e-6 {
            let scale = total / left;
            for chance in self.chance.values_mut() {
                *chance *= scale;
            }
        }
        cleared / total
    }

    /// Where a searcher at `from` had best go to look next: the middle of the polygon with the
    /// most chance in and round it, for how far it is to walk there - no further than `within`
    /// meters - and at least `apart` meters from each of `taken`, where others are already going.
    /// None if nowhere is worth going to.
    pub fn best(&self, mesh: &NavMesh, from: Vector3<f32>, taken: &[Vector3<f32>], apart: f32, within: f32) -> Option<Vector3<f32>> {
        let costs = mesh.costs_from(from, within, |_| 0.0);
        // Where there is any chance, and next to it.
        let mut near: Vec<Poly> = self.chance.keys().flat_map(|&n| std::iter::once(n).chain(mesh.portals(n).iter().map(|p| p.to))).collect();
        near.sort_unstable();
        near.dedup();
        let mut best: Option<(f32, Vector3<f32>)> = None;
        for n in near {
            let Some(&cost) = costs.get(&n) else { continue };
            let at = mesh.centre(n);
            if !taken.iter().all(|t| (t.x - at.x).hypot(t.z - at.z) >= apart) {
                continue;
            }
            let around = self.chance(n) + mesh.portals(n).iter().map(|p| self.chance(p.to)).sum::<f32>();
            if around <= 1.0e-6 {
                continue;
            }
            let worth = around / (1.0 + cost / HALVED_AFTER);
            if best.is_none_or(|(b, _)| worth > b) {
                best = Some((worth, at));
            }
        }
        best.map(|(_, at)| at)
    }

    fn total(&self) -> f32 {
        self.chance.values().sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::navigation::{
        grid::{Rng, WalkGrid},
        route::between,
        walk::{plan, route_to},
    };

    fn mesh(grid: &WalkGrid) -> NavMesh {
        NavMesh::build(grid, Vector3::zeros(), |_, _| 0)
    }

    #[test]
    fn the_chance_spreads_up_the_stairs_and_looking_clears_only_its_own_floor() {
        // The ground along the first row; stairs along the second, up to 2 m at x = 3; and a
        // floor 2 m up over the first row from there on.
        let mut grid = WalkGrid::with_storeys(8, 2, 2, 1.0);
        for x in 0..8 {
            grid.set(x, 0, true);
        }
        for x in 0..4 {
            grid.set(x, 1, true);
            grid.set_floor(x, 1, 0.5 * (x as f32 + 1.0));
        }
        for x in 3..8 {
            let (ux, uz) = grid.on_storey((x, 0), 1);
            grid.set(ux, uz, true);
            grid.set_floor(ux, uz, 2.0);
        }
        let mesh = mesh(&grid);
        let mut map = SearchMap::new();
        map.lose(&mesh, Vector3::new(0.5, 0.0, 0.5), None);
        map.spread(&mesh, 12.0);
        let (up, under) = (Vector3::new(5.5, 2.0, 0.5), Vector3::new(5.5, 0.0, 0.5));
        assert!(map.chance_at(&mesh, up) > 0.0, "up the stairs");
        assert!(map.chance_at(&mesh, under) > 0.0, "and along the ground under it");
        // Looking along the ground clears the ground, not the floor up above it.
        map.look(&mesh, Vector3::new(0.5, 0.0, 0.5), std::f32::consts::FRAC_PI_2, (10.0, 3.0, 10.0));
        assert_eq!(map.chance_at(&mesh, under), 0.0);
        assert!(map.chance_at(&mesh, up) > 0.0);
    }

    /// A maze of `n` by `n` junctions, joined by corridors three cells wide with walls one cell
    /// thick, made by a random walk that backtracks.
    fn maze(n: usize, rng: &mut Rng) -> WalkGrid {
        let size = n * 4 + 1;
        let mut grid = WalkGrid::new(size, size, 1.0);
        let mut seen = vec![false; n * n];
        let open = |grid: &mut WalkGrid, jx: usize, jz: usize| {
            for z in 0..3 {
                for x in 0..3 {
                    grid.set(jx * 4 + 1 + x, jz * 4 + 1 + z, true);
                }
            }
        };
        let mut stack = vec![(0usize, 0usize)];
        seen[0] = true;
        open(&mut grid, 0, 0);
        while let Some(&(x, z)) = stack.last() {
            let next: Vec<(usize, usize)> = [(0isize, 1isize), (1, 0), (0, -1), (-1, 0)]
                .into_iter()
                .filter_map(|(dx, dz)| Some((x.checked_add_signed(dx)?, z.checked_add_signed(dz)?)))
                .filter(|&(nx, nz)| nx < n && nz < n && !seen[nz * n + nx])
                .collect();
            if next.is_empty() {
                stack.pop();
                continue;
            }
            let (nx, nz) = next[rng.below(next.len())];
            seen[nz * n + nx] = true;
            open(&mut grid, nx, nz);
            // The wall between them, knocked through.
            let (wx, wz) = (x * 4 + 1 + (nx * 4).saturating_sub(x * 4).min(4) , z * 4 + 1 + (nz * 4).saturating_sub(z * 4).min(4));
            let (wx, wz) = if nx < x { (x * 4, wz) } else if nx > x { (x * 4 + 4, wz) } else { (wx, wz) };
            let (wx, wz) = if nz < z { (wx, z * 4) } else if nz > z { (wx, z * 4 + 4) } else { (wx, wz) };
            for k in 0..3 {
                if nx != x {
                    grid.set(wx, z * 4 + 1 + k, true);
                } else {
                    grid.set(x * 4 + 1 + k, wz, true);
                }
            }
            stack.push((nx, nz));
        }
        grid
    }

    /// Everyone is a cell's middle: `(x, z)` as a point.
    fn at((x, z): (usize, usize)) -> Vector3<f32> {
        Vector3::new(x as f32 + 0.5, 0.0, z as f32 + 0.5)
    }


    const DT: f32 = 0.1;
    // Short enough that neither way of searching finds them every time: over the navmesh's
    // straight routes, even wandering searchers see a long way down the corridors.
    const SEARCH_TIME: f32 = 12.0;
    const RUNNER_SPEED: f32 = 5.0;
    const SEARCHER_SPEED: f32 = 3.0;
    const SIGHT: (f32, f32, f32) = (20.0, 0.96, 2.0);
    const LOOK_ABOUT: f32 = 3.0;

    struct Searcher {
        position: Vector3<f32>,
        heading: f32,
        route: Vec<Vector3<f32>>,
        looking: Option<f32>,
        look_from: f32,
        started: bool,
    }

    /// Whether a searcher at `from` facing `heading` sees someone at `them`.
    fn sees(mesh: &NavMesh, from: Vector3<f32>, heading: f32, them: Vector3<f32>) -> bool {
        let to = them - from;
        let distance = (to.x * to.x + to.z * to.z).sqrt();
        if distance > SIGHT.0 {
            return false;
        }
        let ahead = Vector3::new(heading.sin(), 0.0, heading.cos());
        let in_cone = distance <= SIGHT.2 || (to.dot(&ahead) / distance).clamp(-1.0, 1.0).acos() <= SIGHT.1;
        in_cone && mesh.sees_across(from, them)
    }

    /// How long the runner has been running when the search starts: out of sight, round a corner.
    const HEAD_START: f32 = 3.0;

    /// One chase lost and searched for, with the map or without: how long it took to find them,
    /// if they were found in time. None as a whole if the chase was never lost - a searcher could
    /// still see them when the search started.
    fn search(seed: u64, with_map: bool) -> Option<Option<f32>> {
        let mut rng = Rng::new(seed);
        let grid = maze(12, &mut rng);
        let mesh = mesh(&grid);
        let cells: Vec<(usize, usize)> = grid.walkable_cells().collect();
        // Lost at a junction's middle; the runner goes on a random way and hides.
        let lost = loop {
            let c = cells[rng.below(cells.len())];
            if c.0 % 4 == 2 && c.1 % 4 == 2 {
                break c;
            }
        };
        let running = between(&mut rng, (5.0, 15.0));
        let mut runner = at(lost);
        let mut last = lost;
        let mut runner_cell = lost;
        let mut run_left = running;
        let mut first_way: Option<(f32, f32)> = None;
        // The searchers were chasing, a few meters behind.
        let behind = cells
            .iter()
            .copied()
            .filter(|&c| {
                let d = at(c) - at(lost);
                let d = (d.x * d.x + d.z * d.z).sqrt();
                (5.0..9.0).contains(&d) && mesh.sees_across(at(c), at(lost))
            })
            .collect::<Vec<_>>();
        if behind.is_empty() {
            return None;
        }
        let mut in_sight_at_start = true;
        let mut searchers: Vec<Searcher> = (0..3)
            .map(|_| {
                let c = behind[rng.below(behind.len())];
                let to = at(lost) - at(c);
                Searcher {
                    position: at(c),
                    heading: to.x.atan2(to.z),
                    route: Vec::new(),
                    looking: None,
                    look_from: 0.0,
                    started: false,
                }
            })
            .collect();
        let mut map = SearchMap::new();
        let mut time = -HEAD_START;
        while time < SEARCH_TIME {
            time += DT;
            if time > 0.0 && std::mem::take(&mut in_sight_at_start) {
                // Lost only if nobody can see them as the search starts.
                if searchers.iter().any(|s| sees(&mesh, s.position, s.heading, runner)) {
                    return None;
                }
            }
            // The runner: along the corridors, mostly straight on, until it hides.
            if run_left > 0.0 {
                run_left -= DT;
                let mut step = RUNNER_SPEED * DT;
                while step > 0.0 {
                    let target = at(runner_cell);
                    let to = target - runner;
                    let d = to.norm();
                    if d > step {
                        runner += to / d * step;
                        break;
                    }
                    runner = target;
                    step -= d;
                    let options: Vec<(usize, usize)> = [(0isize, 1isize), (1, 0), (0, -1), (-1, 0)]
                        .into_iter()
                        .filter_map(|(dx, dz)| Some((runner_cell.0.checked_add_signed(dx)?, runner_cell.1.checked_add_signed(dz)?)))
                        .filter(|&(x, z)| x < grid.width && z < grid.depth && grid.is_walkable(x, z) && (x, z) != last)
                        .collect();
                    if options.is_empty() {
                        std::mem::swap(&mut last, &mut runner_cell);
                        continue;
                    }
                    let straight = (
                        runner_cell.0 as isize * 2 - last.0 as isize,
                        runner_cell.1 as isize * 2 - last.1 as isize,
                    );
                    let next = options
                        .iter()
                        .copied()
                        .find(|&(x, z)| (x as isize, z as isize) == straight && rng.below(10) < 8)
                        .unwrap_or(options[rng.below(options.len())]);
                    first_way.get_or_insert(((next.0 as f32 - lost.0 as f32), (next.1 as f32 - lost.1 as f32)));
                    last = runner_cell;
                    runner_cell = next;
                }
            }
            if time <= 0.0 {
                // Round the corner, gone from sight: the search is yet to start.
                continue;
            }
            if with_map && time <= DT * 1.5 {
                map.lose(&mesh, at(lost), first_way.map(|(dx, dz)| Vector3::new(dx, 0.0, dz)));
                // They have had their head start.
                map.spread(&mesh, RUNNER_SPEED * HEAD_START);
            }
            if with_map {
                map.spread(&mesh, RUNNER_SPEED * DT);
            }
            // Each searcher looks, then goes on searching.
            let goals: Vec<Option<Vector3<f32>>> = searchers.iter().map(|s| s.route.first().copied()).collect();
            for (n, s) in searchers.iter_mut().enumerate() {
                if sees(&mesh, s.position, s.heading, runner) {
                    return Some(Some(time));
                }
                if with_map {
                    map.look(&mesh, s.position, s.heading, SIGHT);
                }
                if s.route.is_empty() {
                    match s.looking {
                        Some(left) if left > 0.0 => {
                            s.looking = Some(left - DT);
                            let turn = std::f32::consts::TAU * (LOOK_ABOUT - left) / LOOK_ABOUT;
                            s.heading = s.look_from + 1.2 * turn.sin();
                            continue;
                        }
                        Some(_) => s.looking = None,
                        None if s.started => {
                            s.looking = Some(LOOK_ABOUT);
                            s.look_from = s.heading;
                            continue;
                        }
                        None => {}
                    }
                    s.route = if with_map {
                        let taken: Vec<Vector3<f32>> =
                            goals.iter().enumerate().filter(|&(m, _)| m != n).filter_map(|(_, g)| *g).collect();
                        match map.best(&mesh, s.position, &taken, 8.0, 80.0) {
                            Some(goal) => route_to(&mesh, s.position, goal, f32::INFINITY),
                            None => plan(&mesh, s.position, (10.0, 40.0), &mut rng),
                        }
                    } else if !s.started {
                        // As the game's droids do: first a little way on from where they were
                        // lost, the way they were going, then somewhere at random nearby.
                        let guess = first_way
                            .map(|(dx, dz)| {
                                let l = (dx * dx + dz * dz).sqrt().max(1.0e-3);
                                at(lost) + Vector3::new(dx / l, 0.0, dz / l) * 4.0
                            })
                            .unwrap_or(at(lost));
                        route_to(&mesh, s.position, guess, f32::INFINITY)
                    } else {
                        plan(&mesh, s.position, (10.0, 40.0), &mut rng)
                    };
                    s.started = true;
                }
                // On along the route.
                let mut step = SEARCHER_SPEED * DT;
                while let Some(&next) = s.route.last() {
                    let to = next - s.position;
                    let d = (to.x * to.x + to.z * to.z).sqrt();
                    if d > 1.0e-3 {
                        s.heading = to.x.atan2(to.z);
                    }
                    if d <= step {
                        s.position = next;
                        s.route.pop();
                        step -= d;
                    } else {
                        s.position += to / d * step;
                        break;
                    }
                }
            }
        }
        Some(None)
    }

    #[test]
    fn searching_by_the_map_finds_them_more_often_and_sooner() {
        let trials = 300;
        let run = |with_map: bool| {
            let searches: Vec<Option<f32>> = (0..trials).filter_map(|seed| search(seed * 7919 + 13, with_map)).collect();
            let found: Vec<f32> = searches.iter().flatten().copied().collect();
            println!("{} searches, of {trials} chases", searches.len());
            (found.len() as f32 / searches.len() as f32, found.iter().sum::<f32>() / found.len().max(1) as f32)
        };
        let (wandering, wandering_time) = run(false);
        let (mapped, mapped_time) = run(true);
        println!(
            "found by wandering {:.0}% in {:.1} s on average; by the map {:.0}% in {:.1} s",
            wandering * 100.0,
            wandering_time,
            mapped * 100.0,
            mapped_time
        );
        // On the navmesh: 88% against 83% (the grid's map had 98% against 83% in 30 s).
        assert!(mapped > wandering + 0.03, "{mapped} against {wandering}");
    }

    #[test]
    fn the_chance_spreads_along_the_ground_and_looking_clears_it() {
        // A corridor, 1 wide and 21 long.
        let mut grid = WalkGrid::new(21, 1, 1.0);
        for x in 0..21 {
            grid.set(x, 0, true);
        }
        // Every 2 m another area, to have polygons along it.
        let mesh = NavMesh::build(&grid, Vector3::zeros(), |x, _| (x / 2) as u8);
        let mut map = SearchMap::new();
        let x = |x: f32| Vector3::new(x, 0.0, 0.5);
        map.lose(&mesh, x(10.5), None);
        assert_eq!(map.chance_at(&mesh, x(10.5)), 1.0);
        map.spread(&mesh, 5.0);
        assert!(map.chance_at(&mesh, x(13.5)) > 0.0 && map.chance_at(&mesh, x(7.5)) > 0.0);
        assert_eq!(map.chance_at(&mesh, x(17.5)), 0.0, "no further than they could go");
        assert!((map.total() - 1.0).abs() < 1.0e-4);
        // Looking along the corridor from the left end, facing right, it sees all of it.
        let cleared = map.look(&mesh, x(0.5), std::f32::consts::FRAC_PI_2, (30.0, 0.5, 1.0));
        assert!((cleared - 1.0).abs() < 1.0e-4);
        assert!(map.is_empty());
    }
}
