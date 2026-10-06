//! A level's walkable ground as a grid of cells, and the cheapest ways across it.
//!
//! Nothing says which parts of a level are floor, so the game samples it on a grid and marks
//! the cells someone could stand in, with how high the floor is there. Routes between cells come
//! from a search that keeps to the middle of the corridors (see [`WalkGrid::routes_from`]), and
//! walking distances from a breadth-first one (see [`WalkGrid::distances_from`]).

use nalgebra::Vector3;
use std::{
    cmp::Ordering,
    collections::{BinaryHeap, VecDeque},
};

/// How much dearer a step is into a cell at the edge of the floor, next to a wall, than one out in
/// the open: enough that a route keeps to the middle of a corridor, not so much that it goes far
/// out of its way for it.
const EDGE_COST: f32 = 2.0;

/// How much higher or lower one cell's floor can be than its neighbour's for a step between them:
/// a stair's step, or two where a cell spans them, but not a ledge or the top of a crate.
pub const MAX_CLIMB: f32 = 0.55;

/// How far above or below a height a floor can be and still be at it: on the same level, not a
/// storey up or down.
pub const LEVEL: f32 = 1.0;

/// How many cells across a storey's height counts as, for the nearest walkable cell to a point:
/// the floor under someone's feet before the one under the floor they are on, however near.
pub const STOREY_AWAY: f32 = 8.0;

/// A grid of walkable cells over a level's footprint, each `cell_size` meters across, with how
/// high the floor is in each - on as many storeys as the level has floors over one another (see
/// [`WalkGrid::with_storeys`]).
///
/// A cell is `(x, row)`: across, and down the rows of every storey in turn. On one storey, as
/// most levels are, the rows are just the plan's.
#[derive(Debug, Clone, PartialEq)]
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
}

/// The cheapest ways from one cell to every other, found by [`WalkGrid::routes_from`].
#[derive(Debug, Clone, PartialEq)]
pub struct Routes {
    width: usize,
    /// What it costs to get to each cell, in cells walked with the edges dearer; none where it
    /// cannot be reached.
    pub costs: Vec<Option<f32>>,
    /// The cell each is reached from, as an index.
    previous: Vec<usize>,
}

impl Routes {
    /// The cells from the start to `goal`, both included; none if it cannot be reached.
    pub fn path_to(&self, goal: (usize, usize)) -> Option<Vec<(usize, usize)>> {
        let mut at = goal.1 * self.width + goal.0;
        self.costs.get(at)?.as_ref()?;
        let mut path = vec![goal];
        while self.costs[at] != Some(0.0) {
            at = self.previous[at];
            path.push((at % self.width, at / self.width));
        }
        path.reverse();
        Some(path)
    }
}

/// A cell waiting to be walked from, cheapest first.
#[derive(Debug, PartialEq)]
struct Frontier(f32, usize);

impl Eq for Frontier {}

impl Ord for Frontier {
    fn cmp(&self, other: &Self) -> Ordering {
        other.0.total_cmp(&self.0)
    }
}

impl PartialOrd for Frontier {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

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
        }
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
    }

    pub fn is_walkable(&self, x: usize, z: usize) -> bool {
        self.cells[z * self.width + x]
    }

    pub fn set_floor(&mut self, x: usize, z: usize, height: f32) {
        self.floors[z * self.width + x] = height;
    }

    /// How high the floor is in a cell, in meters.
    pub fn floor(&self, x: usize, z: usize) -> f32 {
        self.floors[z * self.width + x]
    }

    /// Whether a step from cell `from` to `to` can be taken: `to` is walkable, and its floor no
    /// more than [`MAX_CLIMB`] above or below `from`'s - up a stair, not a ledge.
    pub fn can_step(&self, from: (usize, usize), to: (usize, usize)) -> bool {
        self.is_walkable(to.0, to.1) && (self.floor(to.0, to.1) - self.floor(from.0, from.1)).abs() <= MAX_CLIMB
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
        [(-1, 0), (1, 0), (0, -1), (0, 1)]
            .into_iter()
            .filter_map(move |(dx, dz)| self.step_to(from, dx, dz))
    }

    /// Whether a cell is at the edge of the floor: next to a wall or a drop, even across a corner.
    fn is_edge(&self, cell: (usize, usize)) -> bool {
        (-1..=1).any(|dz| (-1..=1).any(|dx| (dx, dz) != (0, 0) && self.step_to(cell, dx, dz).is_none()))
    }

    /// The cheapest ways from `start` to every walkable cell it connects to, going across
    /// corners as well as along the grid, but never cutting past a wall's corner, and up and down
    /// stairs from one storey to another. Steps into cells at the edge of the floor are dearer,
    /// so that routes keep to the middle. The search goes no further than routes costing
    /// `within`: cells much further away are left unreached.
    pub fn routes_from(&self, start: (usize, usize), within: f32) -> Routes {
        self.routes_from_weighted(start, within, |_, _| 0.0)
    }

    /// As [`Self::routes_from`], but each step into a cell `(x, row)` costs `extra(x, row)` more -
    /// danger, say: ground an enemy can see, which a route then goes round if it is not too far.
    /// `extra` is never below 0.
    pub fn routes_from_weighted(
        &self,
        start: (usize, usize),
        within: f32,
        extra: impl Fn(usize, usize) -> f32,
    ) -> Routes {
        let mut routes = Routes {
            width: self.width,
            costs: vec![None; self.cells.len()],
            previous: vec![0; self.cells.len()],
        };
        if !self.is_walkable(start.0, start.1) {
            return routes;
        }
        let first = start.1 * self.width + start.0;
        routes.costs[first] = Some(0.0);
        let mut frontier = BinaryHeap::from([Frontier(0.0, first)]);
        while let Some(Frontier(cost, at)) = frontier.pop() {
            if routes.costs[at].is_some_and(|best| cost > best) {
                continue;
            }
            if cost > within {
                break;
            }
            let cell = (at % self.width, at / self.width);
            for (dx, dz) in [(-1, 0), (1, 0), (0, -1), (0, 1), (-1, -1), (1, -1), (-1, 1), (1, 1)] {
                let Some(next_cell) = self.step_to(cell, dx, dz) else {
                    continue;
                };
                let diagonal = dx != 0 && dz != 0;
                if diagonal && !(self.step_to(cell, dx, 0).is_some() && self.step_to(cell, 0, dz).is_some()) {
                    continue;
                }
                let step = if diagonal { std::f32::consts::SQRT_2 } else { 1.0 };
                let weight = if self.is_edge(next_cell) { EDGE_COST } else { 1.0 };
                let next = cost + step * (weight + extra(next_cell.0, next_cell.1).max(0.0));
                let there = next_cell.1 * self.width + next_cell.0;
                if routes.costs[there].is_none_or(|best| next < best) {
                    routes.costs[there] = Some(next);
                    routes.previous[there] = at;
                    frontier.push(Frontier(next, there));
                }
            }
        }
        routes
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
        distances[start.1 * self.width + start.0] = Some(0);
        let mut queue = VecDeque::from([start]);
        while let Some(cell) = queue.pop_front() {
            let next = distances[cell.1 * self.width + cell.0].unwrap() + 1;
            for (nx, nz) in self.steps(cell) {
                let slot = &mut distances[nz * self.width + nx];
                if slot.is_none() {
                    *slot = Some(next);
                    queue.push_back((nx, nz));
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

    /// Whether there is floor all the way along the straight line from the middle of cell `a` to
    /// the middle of cell `b` - nothing but floor between them, at about the height the line is
    /// at there, so that one can be seen from the other, going by the ground alone. Not across a
    /// floor up above or below.
    pub fn sees_across(&self, a: (usize, usize), b: (usize, usize)) -> bool {
        let ((pax, paz), (pbx, pbz)) = (self.plan(a), self.plan(b));
        let (ha, hb) = (self.floor(a.0, a.1), self.floor(b.0, b.1));
        let (ax, az) = (pax as f32 + 0.5, paz as f32 + 0.5);
        let (dx, dz) = (pbx as f32 - pax as f32, pbz as f32 - paz as f32);
        // A few samples a cell, so that the line cannot slip past the corner of a wall.
        let samples = ((dx.abs().max(dz.abs())) * 3.0).ceil().max(1.0) as usize;
        (0..=samples).all(|i| {
            let t = i as f32 / samples as f32;
            let (x, z) = ((ax + dx * t).floor(), (az + dz * t).floor());
            let height = ha + (hb - ha) * t;
            x >= 0.0
                && z >= 0.0
                && (x as usize) < self.width
                && (z as usize) < self.depth
                && self
                    .cells_over((x as usize, z as usize))
                    .any(|(cx, cz)| (self.floor(cx, cz) - height).abs() <= LEVEL)
        })
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
            flat + (self.floor(cell.0, cell.1) - point.y).abs() / LEVEL * STOREY_AWAY
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
            .filter(|&(x, z)| (self.floor(x, z) - point.y).abs() <= LEVEL)
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
    #[test]
    fn a_weighted_route_goes_round_the_dear_ground_if_it_can() {
        // An open room, 11 by 11: straight across the middle is cheapest, until the middle
        // column is made dear everywhere but at the top row.
        let mut grid = WalkGrid::new(11, 11, 1.0);
        for z in 0..11 {
            for x in 0..11 {
                grid.set(x, z, true);
            }
        }
        let (from, to) = ((0, 5), (10, 5));
        let plain = grid.routes_from(from, f32::INFINITY).path_to(to).unwrap();
        assert!(plain.iter().all(|&(_, z)| (4..=6).contains(&z)), "straight across: {plain:?}");
        let dear = |x: usize, z: usize| if x == 5 && z != 0 { 50.0 } else { 0.0 };
        let round = grid.routes_from_weighted(from, f32::INFINITY, dear).path_to(to).unwrap();
        let crossing = round.iter().find(|&&(x, _)| x == 5).unwrap();
        assert_eq!(crossing.1, 0, "over the top: {round:?}");
        // With nothing dear, the same as the plain routes.
        let same = grid.routes_from_weighted(from, f32::INFINITY, |_, _| 0.0).path_to(to).unwrap();
        assert_eq!(same, plain);
    }

    use super::*;

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
    fn a_route_goes_round_walls_and_keeps_off_them() {
        let maze = grid(&[
            ".......", //
            ".......", //
            ".......", //
            "#####..", //
            ".......", //
        ]);
        let path = maze
            .routes_from((0, 1), f32::INFINITY)
            .path_to((0, 4))
            .unwrap();
        assert_eq!(path.first(), Some(&(0, 1)));
        assert_eq!(path.last(), Some(&(0, 4)));
        // Every step goes to a neighbour, and only ever onto floor.
        for pair in path.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            assert!(
                a.0.abs_diff(b.0) <= 1 && a.1.abs_diff(b.1) <= 1,
                "{a:?} to {b:?}"
            );
            assert!(maze.is_walkable(b.0, b.1));
        }
        // Round the end of the wall, and along the middle row rather than hugging the wall.
        assert!(path.iter().any(|&(x, _)| x >= 5));
        assert!(path.contains(&(2, 1)) || path.contains(&(3, 1)));
    }

    #[test]
    fn a_route_climbs_the_stairs_and_not_the_ledge() {
        // A floor 1 m up along the top row, reached by the stairs at the right-hand end.
        let mut maze = grid(&[
            ".....", //
            ".....", //
        ]);
        for x in 0..5 {
            maze.set_floor(x, 0, 1.0);
        }
        for (z, height) in [(0, 1.0), (1, 0.5)] {
            maze.set_floor(4, z, height);
        }
        assert!(!maze.can_step((0, 1), (0, 0)), "a 1 m ledge is no step");
        assert!(maze.can_step((4, 1), (4, 0)), "half a meter is");
        let path = maze.routes_from((0, 1), f32::INFINITY).path_to((0, 0)).unwrap();
        assert!(path.contains(&(4, 1)), "round by the stairs: {path:?}");
        for pair in path.windows(2) {
            assert!(maze.can_step(pair[0], pair[1]), "{:?} to {:?}", pair[0], pair[1]);
        }
        assert_eq!(maze.distances_from((0, 1))[0], Some(9), "along, up and back");
    }

    #[test]
    fn a_route_never_cuts_a_corner() {
        let maze = grid(&[
            "..", //
            "#.", //
        ]);
        let path = maze
            .routes_from((0, 0), f32::INFINITY)
            .path_to((1, 1))
            .unwrap();
        assert_eq!(path, [(0, 0), (1, 0), (1, 1)]);
    }

    #[test]
    fn there_is_no_route_to_where_cannot_be_reached() {
        let maze = grid(&[".#."]);
        assert_eq!(
            maze.routes_from((0, 0), f32::INFINITY).path_to((2, 0)),
            None
        );
        assert_eq!(
            maze.routes_from((1, 0), f32::INFINITY).path_to((2, 0)),
            None
        );
    }

    #[test]
    fn a_search_within_a_cost_reaches_no_further() {
        let maze = grid(&["......"]);
        let routes = maze.routes_from((0, 0), 2.5);
        // The ends of the row are next to the grid's edge, and so dearer: 2 to the first cell,
        // then 2 more for each after it.
        assert!(routes.path_to((1, 0)).is_some());
        assert_eq!(routes.path_to((5, 0)), None);
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
    fn a_route_goes_up_the_stairs_to_the_floor_over_the_ground() {
        let grid = two_storeys();
        let up = grid.on_storey((0, 0), 1);
        let path = grid.routes_from((0, 0), f32::INFINITY).path_to(up).expect("a way up");
        assert!(path.iter().any(|&cell| cell.0 == 9), "by the stairs: {path:?}");
        for pair in path.windows(2) {
            assert!(grid.can_step(pair[0], pair[1]), "{:?} to {:?}", pair[0], pair[1]);
        }
        // Straight up from under it, there is no step.
        assert!(!grid.can_step((0, 0), up));
        assert!(grid.distances_from((0, 0))[up.1 * grid.width + up.0].is_some());
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
    fn one_floor_is_not_seen_across_from_the_other() {
        let grid = two_storeys();
        let (a, b) = ((0, 1), (6, 1));
        assert!(grid.sees_across(a, b), "along the ground");
        assert!(grid.sees_across(grid.on_storey(a, 1), grid.on_storey(b, 1)), "along the floor up above");
        assert!(!grid.sees_across(a, grid.on_storey(b, 1)), "not from one to the other");
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
    fn rng_stays_in_range() {
        let mut rng = Rng::new(7);
        assert!((0..1000).all(|_| rng.below(5) < 5));
    }
}
