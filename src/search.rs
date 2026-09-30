//! Searching for someone lost from sight: where they could be by now, and who looks where.
//!
//! A [`SearchMap`] holds, for every cell of the ground, the chance that the one being looked for
//! is there. It starts with all of it where they were lost - some of it a little way on, the way
//! they were going - and spreads it along the ground as fast as they could go ([`SearchMap::spread`]),
//! so that the longer they have been gone, the more ground they could be in. Whatever a searcher
//! can see, and does not see them in, is cleared ([`SearchMap::look`]): they are not there, and the
//! rest of the chance is where they are. Each searcher then makes for the most likely ground it
//! can get to soon, keeping clear of where the others are going ([`SearchMap::best`]), so that
//! several searchers spread out rather than all look in the same place.
//!
//! Compared with each searcher wandering off to somewhere at random, it finds a player who has
//! run off and hidden far more often: see the test at the bottom.

use crate::grid::WalkGrid;

/// How much of its chance a cell keeps each step it spreads, the rest going to its neighbours.
const KEEP: f32 = 0.2;
/// How much of the chance goes a little way on, the way they were going, when they are lost, and
/// how far on, in cells.
const AHEAD: f32 = 0.6;
const AHEAD_CELLS: f32 = 6.0;
/// How far round a cell, in cells, counts towards how much it is worth searching; and how far, in
/// cells walked, halves how much a cell is worth.
const AROUND: isize = 3;
const HALVED_AFTER: f32 = 25.0;

/// Where someone lost from sight could be: a chance for every cell of the ground.
#[derive(Debug, Clone, PartialEq)]
pub struct SearchMap {
    width: usize,
    depth: usize,
    chance: Vec<f32>,
    /// How far, in cells, they could have gone that has not been spread yet.
    owed: f32,
}

impl SearchMap {
    /// A map of `grid`'s ground with nobody lost on it.
    pub fn new(grid: &WalkGrid) -> Self {
        Self {
            width: grid.width,
            depth: grid.depth,
            chance: vec![0.0; grid.width * grid.depth],
            owed: 0.0,
        }
    }

    /// They have just been lost at `at`, going `going` - a way across the grid, in cells, if it is
    /// known: they are there, or a little way on that way.
    pub fn lose(&mut self, grid: &WalkGrid, at: (usize, usize), going: Option<(f32, f32)>) {
        self.chance.iter_mut().for_each(|c| *c = 0.0);
        self.owed = 0.0;
        if !grid.is_walkable(at.0, at.1) {
            return;
        }
        let ahead = going
            .and_then(|(dx, dz)| {
                let length = (dx * dx + dz * dz).sqrt();
                (length > 1.0e-3).then(|| {
                    let x = at.0 as f32 + dx / length * AHEAD_CELLS;
                    let z = at.1 as f32 + dz / length * AHEAD_CELLS;
                    (x.max(0.0) as usize, z.max(0.0) as usize)
                })
            })
            .filter(|&(x, z)| x < self.width && z < self.depth && grid.is_walkable(x, z))
            .filter(|&cell| grid.sees_across(at, cell));
        match ahead {
            Some(ahead) => {
                let (ahead, at) = (self.index(ahead), self.index(at));
                self.chance[ahead] = AHEAD;
                self.chance[at] = 1.0 - AHEAD;
            }
            None => {
                let at = self.index(at);
                self.chance[at] = 1.0;
            }
        }
    }

    /// Whether there is still anywhere left they could be.
    pub fn is_empty(&self) -> bool {
        self.total() <= 1.0e-6
    }

    /// The chance that they are in cell `(x, z)`.
    pub fn chance(&self, (x, z): (usize, usize)) -> f32 {
        self.chance[self.index((x, z))]
    }

    /// They could have gone `cells` further along the ground since this was last asked.
    pub fn spread(&mut self, grid: &WalkGrid, cells: f32) {
        self.owed += cells.max(0.0);
        while self.owed >= 1.0 {
            self.owed -= 1.0;
            self.step(grid);
        }
    }

    /// One cell further: each cell keeps some of its chance and gives the rest out evenly among
    /// its walkable neighbours.
    fn step(&mut self, grid: &WalkGrid) {
        let mut next = vec![0.0; self.chance.len()];
        for (index, &chance) in self.chance.iter().enumerate() {
            if chance <= 0.0 {
                continue;
            }
            let (x, z) = (index % self.width, index / self.width);
            let neighbours: Vec<usize> = [(-1, 0), (1, 0), (0, -1), (0, 1)]
                .into_iter()
                .filter_map(|(dx, dz)| {
                    let (nx, nz) = (x.checked_add_signed(dx)?, z.checked_add_signed(dz)?);
                    (nx < self.width && nz < self.depth && grid.is_walkable(nx, nz))
                        .then(|| self.index((nx, nz)))
                })
                .collect();
            if neighbours.is_empty() {
                next[index] += chance;
                continue;
            }
            next[index] += chance * KEEP;
            let share = chance * (1.0 - KEEP) / neighbours.len() as f32;
            for neighbour in neighbours {
                next[neighbour] += share;
            }
        }
        self.chance = next;
    }

    /// A searcher at cell `from`, facing `heading`, sees as far as `reach` cells, and as far as
    /// `cone` either side of ahead - and all round, as near as `near` cells - and does not see
    /// them: wherever it sees is cleared, and the rest of the chance made up to the whole again.
    /// How much of it that cleared, before it was made up.
    pub fn look(
        &mut self,
        grid: &WalkGrid,
        from: (usize, usize),
        heading: f32,
        (reach, cone, near): (f32, f32, f32),
    ) -> f32 {
        let total = self.total();
        if total <= 0.0 {
            return 0.0;
        }
        let (ahead_x, ahead_z) = (heading.sin(), heading.cos());
        let r = reach.ceil() as isize;
        let mut cleared = 0.0;
        for dz in -r..=r {
            for dx in -r..=r {
                let (Some(x), Some(z)) = (from.0.checked_add_signed(dx), from.1.checked_add_signed(dz)) else {
                    continue;
                };
                if x >= self.width || z >= self.depth {
                    continue;
                }
                let index = self.index((x, z));
                if self.chance[index] <= 0.0 {
                    continue;
                }
                let (fx, fz) = (dx as f32, dz as f32);
                let distance = (fx * fx + fz * fz).sqrt();
                if distance > reach {
                    continue;
                }
                let in_cone = distance <= near
                    || ((fx * ahead_x + fz * ahead_z) / distance.max(1.0e-3)).clamp(-1.0, 1.0).acos() <= cone;
                if in_cone && grid.sees_across(from, (x, z)) {
                    cleared += self.chance[index];
                    self.chance[index] = 0.0;
                }
            }
        }
        let left = total - cleared;
        if left > 1.0e-6 {
            let scale = total / left;
            self.chance.iter_mut().for_each(|c| *c *= scale);
        }
        cleared / total
    }

    /// Where a searcher at cell `from` had best go to look next: the cell with the most chance
    /// round it, for how far it is to walk there - no further than `within` cells - and at least
    /// `apart` cells from each of `taken`, where others are already going. None if nowhere is
    /// worth going to.
    pub fn best(
        &self,
        grid: &WalkGrid,
        from: (usize, usize),
        taken: &[(usize, usize)],
        apart: f32,
        within: f32,
    ) -> Option<(usize, usize)> {
        let routes = grid.routes_from(from, within);
        let mut best: Option<(f32, (usize, usize))> = None;
        for (index, cost) in routes.costs.iter().enumerate() {
            let Some(cost) = *cost else { continue };
            let cell = (index % self.width, index / self.width);
            let clear_of_others = taken.iter().all(|&(tx, tz)| {
                let (dx, dz) = (tx as f32 - cell.0 as f32, tz as f32 - cell.1 as f32);
                (dx * dx + dz * dz).sqrt() >= apart
            });
            if !clear_of_others {
                continue;
            }
            let around = self.around(cell);
            if around <= 1.0e-6 {
                continue;
            }
            let worth = around / (1.0 + cost / HALVED_AFTER);
            if best.is_none_or(|(b, _)| worth > b) {
                best = Some((worth, cell));
            }
        }
        best.map(|(_, cell)| cell)
    }

    /// The chance within [`AROUND`] cells of `cell`.
    fn around(&self, (x, z): (usize, usize)) -> f32 {
        let mut sum = 0.0;
        for dz in -AROUND..=AROUND {
            for dx in -AROUND..=AROUND {
                if let (Some(nx), Some(nz)) = (x.checked_add_signed(dx), z.checked_add_signed(dz)) {
                    if nx < self.width && nz < self.depth {
                        sum += self.chance[self.index((nx, nz))];
                    }
                }
            }
        }
        sum
    }

    fn total(&self) -> f32 {
        self.chance.iter().sum()
    }

    fn index(&self, (x, z): (usize, usize)) -> usize {
        z * self.width + x
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        grid::Rng,
        route::{between, plan, route_to},
    };
    use nalgebra::Vector3;

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

    fn cell(grid: &WalkGrid, p: Vector3<f32>) -> (usize, usize) {
        grid.walkable_cell(Vector3::zeros(), p).unwrap()
    }

    const DT: f32 = 0.1;
    const SEARCH_TIME: f32 = 30.0;
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
    fn sees(grid: &WalkGrid, from: Vector3<f32>, heading: f32, them: Vector3<f32>) -> bool {
        let to = them - from;
        let distance = (to.x * to.x + to.z * to.z).sqrt();
        if distance > SIGHT.0 {
            return false;
        }
        let ahead = Vector3::new(heading.sin(), 0.0, heading.cos());
        let in_cone = distance <= SIGHT.2 || (to.dot(&ahead) / distance).clamp(-1.0, 1.0).acos() <= SIGHT.1;
        in_cone && grid.sees_across(cell(grid, from), cell(grid, them))
    }

    /// How long the runner has been running when the search starts: out of sight, round a corner.
    const HEAD_START: f32 = 3.0;

    /// One chase lost and searched for, with the map or without: how long it took to find them,
    /// if they were found in time. None as a whole if the chase was never lost - a searcher could
    /// still see them when the search started.
    fn search(seed: u64, with_map: bool) -> Option<Option<f32>> {
        let mut rng = Rng::new(seed);
        let grid = maze(12, &mut rng);
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
                (5.0..9.0).contains(&d) && grid.sees_across(c, lost)
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
        let mut map = SearchMap::new(&grid);
        let mut time = -HEAD_START;
        while time < SEARCH_TIME {
            time += DT;
            if time > 0.0 && std::mem::take(&mut in_sight_at_start) {
                // Lost only if nobody can see them as the search starts.
                if searchers.iter().any(|s| sees(&grid, s.position, s.heading, runner)) {
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
                map.lose(&grid, lost, first_way);
                // They have had their head start.
                map.spread(&grid, RUNNER_SPEED * HEAD_START);
            }
            if with_map {
                map.spread(&grid, RUNNER_SPEED * DT);
            }
            // Each searcher looks, then goes on searching.
            let goals: Vec<Option<Vector3<f32>>> = searchers.iter().map(|s| s.route.first().copied()).collect();
            for (n, s) in searchers.iter_mut().enumerate() {
                if sees(&grid, s.position, s.heading, runner) {
                    return Some(Some(time));
                }
                if with_map {
                    map.look(&grid, cell(&grid, s.position), s.heading, SIGHT);
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
                    let here = cell(&grid, s.position);
                    s.route = if with_map {
                        let taken: Vec<(usize, usize)> = goals
                            .iter()
                            .enumerate()
                            .filter(|&(m, _)| m != n)
                            .filter_map(|(_, g)| g.map(|g| cell(&grid, g)))
                            .collect();
                        match map.best(&grid, here, &taken, 8.0, 80.0) {
                            Some(goal) => route_to((&grid, Vector3::zeros()), s.position, at(goal), f32::INFINITY),
                            None => plan((&grid, Vector3::zeros()), s.position, (10.0, 40.0), &mut rng),
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
                        route_to((&grid, Vector3::zeros()), s.position, guess, f32::INFINITY)
                    } else {
                        plan((&grid, Vector3::zeros()), s.position, (10.0, 40.0), &mut rng)
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
        assert!(mapped > wandering + 0.1, "{mapped} against {wandering}");
    }

    #[test]
    fn the_chance_spreads_along_the_ground_and_looking_clears_it() {
        // A corridor, 1 wide and 21 long.
        let mut grid = WalkGrid::new(21, 1, 1.0);
        for x in 0..21 {
            grid.set(x, 0, true);
        }
        let mut map = SearchMap::new(&grid);
        map.lose(&grid, (10, 0), None);
        assert_eq!(map.chance((10, 0)), 1.0);
        map.spread(&grid, 5.0);
        assert!(map.chance((14, 0)) > 0.0 && map.chance((6, 0)) > 0.0);
        assert_eq!(map.chance((16, 0)), 0.0, "no further than they could go");
        assert!((map.total() - 1.0).abs() < 1.0e-4);
        // Looking along the corridor from the left end, facing right, it sees all of it.
        let cleared = map.look(&grid, (0, 0), std::f32::consts::FRAC_PI_2, (30.0, 0.5, 1.0));
        assert!((cleared - 1.0).abs() < 1.0e-4);
        assert!(map.is_empty());
    }
}
