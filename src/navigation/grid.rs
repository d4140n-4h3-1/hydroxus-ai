//! A level's walkable ground as a grid of cells: what a survey of a level finds, and what its
//! navmesh is made from (see [`crate::navigation::navmesh`]).
//!
//! Nothing says which parts of a level are floor, so the game samples it on a grid and marks
//! the cells someone could stand in, with how high the floor is there, and where a step can be
//! taken from one to the next. Once the navmesh is made, the grid can go: routes, and everything
//! else that finds its way about, are the navmesh's. Walking distances across it, a cell at a
//! time, are for laying out a level (see [`WalkGrid::distances_from`]).

use nalgebra::Vector3;
use std::{collections::VecDeque, sync::OnceLock};

/// How much higher or lower one cell's floor can be than its neighbour's for a step between them:
/// a stair's step, or two where a cell spans them, but not a ledge or the top of a crate.
pub const MAX_CLIMB: f32 = 0.55;

/// How far above or below a height a floor can be and still be at it: on the same level, not a
/// storey up or down.
pub const LEVEL: f32 = 1.0;

/// How many cells across a storey's height counts as, for the nearest walkable cell to a point:
/// the floor under someone's feet before the one under the floor they are on, however near.
pub const STOREY_AWAY: f32 = 8.0;

tuning!(
    /// The ground as a walker takes it (see the module's constants, its defaults): kept with the
    /// grid, and the navmesh made from it.
    {
        /// How much higher or lower a neighbour's floor can be for a step: a stair, not a ledge.
        max_climb: f32 = MAX_CLIMB,
        /// How far above or below a height a floor can be and still be at it.
        level: f32 = LEVEL,
        /// How many cells across a storey's height counts as, for the nearest floor to a point.
        storey_away: f32 = STOREY_AWAY,
    }
);

/// A grid of walkable cells over a level's footprint, each `cell_size` meters across, with how
/// high the floor is in each - on as many storeys as the level has floors over one another (see
/// [`WalkGrid::with_storeys`]).
///
/// A cell is `(x, row)`: across, and down the rows of every storey in turn. On one storey, as
/// most levels are, the rows are just the plan's.
#[derive(Debug, Clone)]
pub struct WalkGrid {
    /// How many cells across, and down, the plan is.
    pub width: usize,
    pub depth: usize,
    /// How many floors over one another it has room for.
    pub storeys: usize,
    pub cell_size: f32,
    cells: Vec<bool>,
    /// How high the floor is in each cell, in meters.
    floors: Vec<f32>,
    /// Where a step each of the [`WAYS`] from each cell lands (see [`Links`]): worked out once,
    /// the first time a search needs it, and again only after the grid changes.
    links: OnceLock<Vec<Links>>,
    /// How the ground is taken: how high a step, how far a storey.
    pub tuning: Tuning,
}

impl PartialEq for WalkGrid {
    /// Grids are the same with the same cells, whether or not either has worked out its steps.
    fn eq(&self, other: &Self) -> bool {
        (self.width, self.depth, self.storeys, self.cell_size) == (other.width, other.depth, other.storeys, other.cell_size)
            && self.cells == other.cells
            && self.floors == other.floors
            && self.tuning == other.tuning
    }
}

/// The ways a step across the plan can go: along the grid first, then across its corners.
const WAYS: [(isize, isize); 8] = [(-1, 0), (1, 0), (0, -1), (0, 1), (-1, -1), (1, -1), (-1, 1), (1, 1)];

/// Where a step each of the [`WAYS`] from a cell lands: 0 where it cannot be taken, otherwise one
/// more than the storey it lands on. Read off a table, a search costs a lookup a step instead of
/// looking through every storey of each neighbour, and its neighbours' neighbours for the edges.
type Links = [u8; 8];

impl WalkGrid {
    /// A grid `width` cells by `depth`, each `cell_size` meters across, with nowhere walkable yet:
    /// one storey.
    pub fn new(width: usize, depth: usize, cell_size: f32) -> Self {
        Self::with_storeys(width, depth, 1, cell_size)
    }

    /// A grid `width` cells by `depth` of ground, each `cell_size` meters across, with `storeys`
    /// floors over one another - a floor up above the ground, and another over that - stacked as
    /// rows of their own: storey `s`'s rows are `s * depth` to `(s + 1) * depth`. Nowhere
    /// walkable yet.
    pub fn with_storeys(width: usize, depth: usize, storeys: usize, cell_size: f32) -> Self {
        let storeys = storeys.max(1);
        Self {
            width,
            depth,
            storeys,
            cell_size,
            cells: vec![false; width * depth * storeys],
            floors: vec![0.0; width * depth * storeys],
            links: OnceLock::new(),
            tuning: Tuning::DEFAULT,
        }
    }

    /// The grid, the ground taken as `tuning` says.
    pub fn with_tuning(mut self, tuning: Tuning) -> Self {
        self.tuning = tuning;
        self.links = OnceLock::new();
        self
    }

    /// How many rows of cells there are, every storey's: `depth` times `storeys`.
    pub fn rows(&self) -> usize {
        self.depth * self.storeys
    }

    /// Where cell `(x, row)` is on the ground's plan, whichever storey it is on.
    pub fn plan(&self, (x, row): (usize, usize)) -> (usize, usize) {
        (x, row % self.depth)
    }

    /// Which storey cell `(x, row)` is on, the ground's being 0.
    pub fn storey(&self, (_, row): (usize, usize)) -> usize {
        row / self.depth
    }

    /// The cell over `(x, z)` of the plan on storey `storey`.
    pub fn on_storey(&self, (x, z): (usize, usize), storey: usize) -> (usize, usize) {
        (x, z + storey * self.depth)
    }

    /// The walkable cells over `(x, z)` of the plan, the ground's first.
    pub fn cells_over(&self, (x, z): (usize, usize)) -> impl Iterator<Item = (usize, usize)> + '_ {
        (0..self.storeys)
            .map(move |storey| self.on_storey((x, z), storey))
            .filter(|&(x, row)| self.is_walkable(x, row))
    }

    pub fn set(&mut self, x: usize, z: usize, walkable: bool) {
        self.cells[z * self.width + x] = walkable;
        self.relink((x, z));
    }

    pub fn is_walkable(&self, x: usize, z: usize) -> bool {
        self.cells[z * self.width + x]
    }

    pub fn set_floor(&mut self, x: usize, z: usize, height: f32) {
        self.floors[z * self.width + x] = height;
        self.relink((x, z));
    }

    /// The links of the cells a change to cell `(x, row)` touches, worked out again if they have
    /// been worked out at all: its own and its neighbours' on the plan, on every storey - a cell's
    /// links look no further than a step. Thrown away and worked out afresh for the whole grid,
    /// as they were, a city's took seconds - for a door opened, in the middle of a game.
    fn relink(&mut self, cell: (usize, usize)) {
        let Some(mut links) = self.links.take() else {
            return;
        };
        let (x, z) = self.plan(cell);
        for dz in -1isize..=1 {
            for dx in -1isize..=1 {
                let (Some(nx), Some(nz)) = (x.checked_add_signed(dx), z.checked_add_signed(dz)) else {
                    continue;
                };
                if nx >= self.width || nz >= self.depth {
                    continue;
                }
                for storey in 0..self.storeys {
                    let index = (nz + storey * self.depth) * self.width + nx;
                    links[index] = self.links_of(index);
                }
            }
        }
        let _ = self.links.set(links);
    }

    /// How high the floor is in a cell, in meters.
    pub fn floor(&self, x: usize, z: usize) -> f32 {
        self.floors[z * self.width + x]
    }

    /// Whether a step from cell `from` to `to` can be taken: `to` is walkable, and its floor no
    /// more than [`MAX_CLIMB`] above or below `from`'s - up a stair, not a ledge.
    pub fn can_step(&self, from: (usize, usize), to: (usize, usize)) -> bool {
        self.is_walkable(to.0, to.1) && (self.floor(to.0, to.1) - self.floor(from.0, from.1)).abs() <= self.tuning.max_climb
    }

    /// The cell a step `(dx, dz)` across the plan from `from` lands in: the walkable one there, on
    /// whichever storey, a step up or down from `from`'s floor - the nearest to it, if more than
    /// one is. None off the grid, or with no floor there to step to.
    pub fn step_to(&self, from: (usize, usize), dx: isize, dz: isize) -> Option<(usize, usize)> {
        let (x, z) = self.plan(from);
        let (nx, nz) = (x.checked_add_signed(dx)?, z.checked_add_signed(dz)?);
        if nx >= self.width || nz >= self.depth {
            return None;
        }
        let floor = self.floor(from.0, from.1);
        self.cells_over((nx, nz))
            .filter(|&to| self.can_step(from, to))
            .min_by(|&a, &b| (self.floor(a.0, a.1) - floor).abs().total_cmp(&(self.floor(b.0, b.1) - floor).abs()))
    }

    /// The cells a step along the plan's four ways from `from` lands in.
    pub fn steps(&self, from: (usize, usize)) -> impl Iterator<Item = (usize, usize)> + '_ {
        let index = from.1 * self.width + from.0;
        let links = self.links()[index];
        (0..4).filter_map(move |way| self.landing(index, way, links[way]).map(|at| (at % self.width, at / self.width)))
    }

    /// Where a step each way from every cell lands, worked out now if it has not been yet.
    fn links(&self) -> &[Links] {
        self.links.get_or_init(|| self.all_links())
    }

    /// Where a step each way from cell `index` lands (see [`Links`]).
    fn links_of(&self, index: usize) -> Links {
        let mut links = [0; 8];
        if self.cells[index] {
            let cell = (index % self.width, index / self.width);
            for (way, &(dx, dz)) in WAYS.iter().enumerate() {
                if let Some(to) = self.step_to(cell, dx, dz) {
                    links[way] = u8::try_from(self.storey(to) + 1).unwrap_or(0);
                }
            }
        }
        links
    }

    /// Every cell's links: a band of cells each on as many threads as there are cores, which in a
    /// city of tens of millions of cells is the difference between seconds and a fraction of one.
    /// On the web, where there are no threads to be had, one after another.
    fn all_links(&self) -> Vec<Links> {
        let cells = self.cells.len();
        let threads = if cfg!(target_arch = "wasm32") {
            1
        } else {
            std::thread::available_parallelism().map_or(1, |n| n.get()).clamp(1, 64)
        };
        if threads == 1 || cells < 1 << 16 {
            return (0..cells).map(|index| self.links_of(index)).collect();
        }
        let band = cells.div_ceil(threads);
        std::thread::scope(|scope| {
            let workers: Vec<_> = (0..cells)
                .step_by(band)
                .map(|from| scope.spawn(move || (from..(from + band).min(cells)).map(|index| self.links_of(index)).collect::<Vec<Links>>()))
                .collect();
            workers.into_iter().flat_map(|worker| worker.join().expect("a links thread panicked")).collect()
        })
    }

    /// The index of the cell a step `way` from cell `index` lands in, `link` being that way's
    /// entry in its [`Links`].
    fn landing(&self, index: usize, way: usize, link: u8) -> Option<usize> {
        if link == 0 {
            return None;
        }
        let (dx, dz) = WAYS[way];
        let (x, z) = self.plan((index % self.width, index / self.width));
        let (nx, nz) = (x.wrapping_add_signed(dx), z.wrapping_add_signed(dz));
        Some((nz + (link as usize - 1) * self.depth) * self.width + nx)
    }

    /// Every walkable cell, on every storey.
    pub fn walkable_cells(&self) -> impl Iterator<Item = (usize, usize)> + '_ {
        (0..self.rows())
            .flat_map(move |z| (0..self.width).map(move |x| (x, z)))
            .filter(|&(x, z)| self.is_walkable(x, z))
    }

    /// Walking distance, in cells, from `start` to every cell, by its index (`row * width + x`);
    /// `None` where it cannot be reached.
    pub fn distances_from(&self, start: (usize, usize)) -> Vec<Option<u32>> {
        let mut distances = vec![None; self.cells.len()];
        if !self.is_walkable(start.0, start.1) {
            return distances;
        }
        let links = self.links();
        let first = start.1 * self.width + start.0;
        distances[first] = Some(0);
        let mut queue = VecDeque::from([first]);
        while let Some(at) = queue.pop_front() {
            let next = distances[at].unwrap() + 1;
            for (way, &link) in links[at][..4].iter().enumerate() {
                let Some(there) = self.landing(at, way, link) else {
                    continue;
                };
                if distances[there].is_none() {
                    distances[there] = Some(next);
                    queue.push_back(there);
                }
            }
        }
        distances
    }

    /// Returns the reachable cell that is the longest walk away from `start`, with its distance.
    pub fn farthest_from(&self, start: (usize, usize)) -> Option<((usize, usize), u32)> {
        self.distances_from(start)
            .iter()
            .enumerate()
            .filter_map(|(i, d)| d.map(|d| ((i % self.width, i / self.width), d)))
            .max_by_key(|&(_, d)| d)
    }

    /// The middle of cell `(x, row)` in the world, at height 0, with the grid's corner at `origin`.
    pub fn center(&self, origin: Vector3<f32>, cell: (usize, usize)) -> Vector3<f32> {
        let (x, z) = self.plan(cell);
        cell_center(origin, self.cell_size, x, z)
    }

    /// The middle of cell `(x, row)`, on its floor.
    pub fn on_floor(&self, origin: Vector3<f32>, (x, z): (usize, usize)) -> Vector3<f32> {
        self.center(origin, (x, z)) + Vector3::new(0.0, self.floor(x, z), 0.0)
    }

    /// The cell that `position` is in, if it is on the grid at all: over its spot on the plan, on
    /// the storey whose floor is nearest its height, of those walkable there - or the ground's,
    /// with none walkable.
    pub fn cell_at(&self, origin: Vector3<f32>, position: Vector3<f32>) -> Option<(usize, usize)> {
        let x = ((position.x - origin.x) / self.cell_size).floor();
        let z = ((position.z - origin.z) / self.cell_size).floor();
        if !(x >= 0.0 && z >= 0.0 && (x as usize) < self.width && (z as usize) < self.depth) {
            return None;
        }
        let plan = (x as usize, z as usize);
        Some(self.nearest_over(plan, position.y).unwrap_or(plan))
    }

    /// Of the walkable cells over `plan`, the one whose floor is nearest `height`.
    fn nearest_over(&self, plan: (usize, usize), height: f32) -> Option<(usize, usize)> {
        self.cells_over(plan)
            .min_by(|&a, &b| (self.floor(a.0, a.1) - height).abs().total_cmp(&(self.floor(b.0, b.1) - height).abs()))
    }

    /// The walkable cell nearest `point`: looked for in rings round it, nearest first, a storey's
    /// height counting as far as [`STOREY_AWAY`] cells across - so that it is found at once
    /// however big the grid.
    pub fn nearest_walkable(&self, origin: Vector3<f32>, point: Vector3<f32>) -> Option<(usize, usize)> {
        let x = ((point.x - origin.x) / self.cell_size).floor().clamp(0.0, (self.width.max(1) - 1) as f32) as isize;
        let z = ((point.z - origin.z) / self.cell_size).floor().clamp(0.0, (self.depth.max(1) - 1) as f32) as isize;
        let away = |cell: (usize, usize)| {
            let flat = crate::flat(self.center(origin, cell) - point).norm() / self.cell_size;
            flat + (self.floor(cell.0, cell.1) - point.y).abs() / self.tuning.level * self.tuning.storey_away
        };
        let most = self.width.max(self.depth) as isize;
        let mut best: Option<((usize, usize), f32)> = None;
        for ring in 0..=most {
            // Nothing further out can beat what has been found.
            if best.is_some_and(|(_, d)| d < ring as f32 - 1.0) {
                break;
            }
            for (dx, dz) in ring_of(ring) {
                let (cx, cz) = (x + dx, z + dz);
                if cx < 0 || cz < 0 || cx as usize >= self.width || cz as usize >= self.depth {
                    continue;
                }
                for cell in self.cells_over((cx as usize, cz as usize)) {
                    let d = away(cell);
                    if best.is_none_or(|(_, b)| d < b) {
                        best = Some((cell, d));
                    }
                }
            }
        }
        best.map(|(cell, _)| cell)
    }

    /// The walkable cell `point` is in, or failing that the nearest one.
    pub fn walkable_cell(&self, origin: Vector3<f32>, point: Vector3<f32>) -> Option<(usize, usize)> {
        self.cell_at(origin, point)
            .filter(|&(x, z)| self.is_walkable(x, z))
            .filter(|&(x, z)| (self.floor(x, z) - point.y).abs() <= self.tuning.level)
            .or_else(|| self.nearest_walkable(origin, point))
    }
}

/// The offsets of the cells `ring` cells round one, on the square's edge: the cell itself for 0.
fn ring_of(ring: isize) -> Vec<(isize, isize)> {
    if ring == 0 {
        return vec![(0, 0)];
    }
    let mut cells = Vec::with_capacity(8 * ring as usize);
    for d in -ring..=ring {
        cells.push((d, -ring));
        cells.push((d, ring));
    }
    for d in -ring + 1..ring {
        cells.push((-ring, d));
        cells.push((ring, d));
    }
    cells
}

/// The middle of cell `(x, z)` of a grid of cells `cell_size` meters across whose corner is at
/// `origin`, at the corner's height.
pub fn cell_center(origin: Vector3<f32>, cell_size: f32, x: usize, z: usize) -> Vector3<f32> {
    origin + Vector3::new((x as f32 + 0.5) * cell_size, 0.0, (z as f32 + 0.5) * cell_size)
}

/// A small deterministic random number generator, so that a seed plays the same way every time.
#[derive(Debug, Clone, PartialEq)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed | 1)
    }

    pub fn below(&mut self, n: usize) -> usize {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        (self.0.wrapping_mul(0x2545F4914F6CDD1D) >> 33) as usize % n.max(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A grid of `width` by `depth` on two storeys, its floor walkable but for a scatter of
    /// cells, and a floor up above over part of it at a stair's height and more.
    fn scattered(width: usize, depth: usize) -> WalkGrid {
        let mut grid = WalkGrid::with_storeys(width, depth, 2, 0.5);
        for z in 0..depth {
            for x in 0..width {
                grid.set(x, z, (x * 7 + z * 13) % 11 != 0);
                if x > width / 2 && z > depth / 3 {
                    let (ux, row) = grid.on_storey((x, z), 1);
                    grid.set(ux, row, (x + z) % 5 != 0);
                    grid.set_floor(ux, row, if x < width / 2 + 3 { 0.25 * (x - width / 2) as f32 } else { 3.0 });
                }
            }
        }
        grid
    }

    #[test]
    fn links_worked_out_on_many_threads_are_those_worked_out_one_by_one() {
        // Big enough to be split among threads.
        let grid = scattered(300, 300);
        let one_by_one: Vec<Links> = (0..grid.cells.len()).map(|index| grid.links_of(index)).collect();
        assert_eq!(grid.all_links(), one_by_one);
    }

    #[test]
    fn links_mended_after_a_change_are_those_worked_out_afresh() {
        let mut grid = scattered(60, 40);
        let _ = grid.links();
        // A door shut and opened, a floor raised, on the ground and up above.
        grid.set(10, 10, false);
        grid.set(31, 20, true);
        let (ux, row) = grid.on_storey((40, 30), 1);
        grid.set_floor(ux, row, 0.5);
        grid.set(ux, row, true);
        let mended = grid.links().to_vec();
        let afresh: Vec<Links> = (0..grid.cells.len()).map(|index| grid.links_of(index)).collect();
        assert_eq!(mended, afresh);
    }


    /// Builds a grid from rows of `#` (wall) and `.` (floor).
    fn grid(rows: &[&str]) -> WalkGrid {
        let mut grid = WalkGrid::new(rows[0].len(), rows.len(), 1.0);
        for (z, row) in rows.iter().enumerate() {
            for (x, c) in row.chars().enumerate() {
                grid.set(x, z, c == '.');
            }
        }
        grid
    }

    #[test]
    fn distances_follow_corridors_not_straight_lines() {
        let maze = grid(&[
            ".#...", //
            ".#.#.", //
            "...#.", //
        ]);
        let distances = maze.distances_from((0, 0));
        // (2, 0) is two cells away in a straight line, but six along the corridor.
        assert_eq!(distances[2], Some(6));
        assert_eq!(distances[1], None);
        assert_eq!(maze.farthest_from((0, 0)), Some(((4, 2), 10)));
    }

    #[test]
    fn cells_are_found_where_they_are_in_the_world() {
        let mut maze = WalkGrid::new(4, 4, 0.5);
        maze.set(3, 1, true);
        maze.set_floor(3, 1, 0.2);
        let origin = Vector3::new(10.0, 0.0, -2.0);
        let middle = maze.center(origin, (3, 1));
        assert_eq!(middle, Vector3::new(11.75, 0.0, -1.25));
        assert_eq!(maze.cell_at(origin, middle), Some((3, 1)));
        assert_eq!(maze.cell_at(origin, Vector3::new(9.0, 0.0, 0.0)), None, "off the grid");
        assert_eq!(maze.walkable_cell(origin, origin), Some((3, 1)), "the nearest walkable");
        assert_eq!(maze.on_floor(origin, (3, 1)).y, 0.2);
    }

    /// A room 10 by 4 on the ground, and a floor 3 m up over all of it but its two right-hand
    /// columns, where stairs run up from the ground along x = 9, from z = 0 to 3, a step of 0.5 m
    /// a cell, and back along x = 8 to the floor up above.
    fn two_storeys() -> WalkGrid {
        let mut grid = WalkGrid::with_storeys(10, 4, 2, 1.0);
        for z in 0..4 {
            for x in 0..10 {
                grid.set(x, z, true);
                grid.set_floor(x, z, 0.0);
                if x < 8 {
                    let (ux, uz) = grid.on_storey((x, z), 1);
                    grid.set(ux, uz, true);
                    grid.set_floor(ux, uz, 3.0);
                }
            }
        }
        // The stairs, on the ground's storey, over the right-hand columns: up along z, then
        // across to the floor up above at x = 7.
        for (z, h) in [(0, 0.5), (1, 1.0), (2, 1.5), (3, 2.0)] {
            grid.set_floor(9, z, h);
        }
        for (z, h) in [(3, 2.5), (2, 3.0)] {
            grid.set_floor(8, z, h);
        }
        grid
    }

    #[test]
    fn a_point_is_on_the_storey_at_its_height() {
        let grid = two_storeys();
        let origin = Vector3::zeros();
        let down = Vector3::new(3.5, 0.0, 1.5);
        let up = Vector3::new(3.5, 3.0, 1.5);
        assert_eq!(grid.cell_at(origin, down), Some((3, 1)));
        assert_eq!(grid.cell_at(origin, up), Some(grid.on_storey((3, 1), 1)));
        assert_eq!(grid.on_floor(origin, grid.cell_at(origin, up).unwrap()).y, 3.0);
        assert_eq!(grid.walkable_cell(origin, up), grid.cell_at(origin, up));
        // Off the floor, the nearest at its height: from over the stairwell's gap, up there.
        assert_eq!(grid.walkable_cell(origin, Vector3::new(8.5, 3.0, 0.5)), Some(grid.on_storey((7, 0), 1)));
    }

    #[test]
    fn the_nearest_walkable_cell_is_found_without_going_through_them_all() {
        // A big grid with one walkable cell: found near or far.
        let mut grid = WalkGrid::new(400, 400, 0.5);
        grid.set(390, 5, true);
        assert_eq!(grid.nearest_walkable(Vector3::zeros(), Vector3::new(190.0, 0.0, 190.0)), Some((390, 5)));
        grid.set(10, 10, true);
        assert_eq!(grid.nearest_walkable(Vector3::zeros(), Vector3::new(5.0, 0.0, 5.0)), Some((10, 10)));
    }

    #[test]
    fn the_steps_worked_out_once_follow_changes_to_the_grid() {
        let mut maze = grid(&["...", "...", "..."]);
        // Cell (2, 1), by `row * width + x`.
        assert!(maze.distances_from((0, 1))[5].is_some());
        for z in 0..3 {
            maze.set(1, z, false);
        }
        assert_eq!(maze.distances_from((0, 1))[5], None);
        assert_eq!(maze.steps((0, 1)).count(), 2);
    }

    #[test]
    fn rng_stays_in_range() {
        let mut rng = Rng::new(7);
        assert!((0..1000).all(|_| rng.below(5) < 5));
    }
}
