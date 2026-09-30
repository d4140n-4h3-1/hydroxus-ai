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

/// A grid of walkable cells over a level's footprint, each `cell_size` meters across, with how
/// high the floor is in each.
#[derive(Debug, Clone, PartialEq)]
pub struct WalkGrid {
    pub width: usize,
    pub depth: usize,
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
    /// A grid `width` cells by `depth`, each `cell_size` meters across, with nowhere walkable yet.
    pub fn new(width: usize, depth: usize, cell_size: f32) -> Self {
        Self {
            width,
            depth,
            cell_size,
            cells: vec![false; width * depth],
            floors: vec![0.0; width * depth],
        }
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

    /// Whether `(x + dx, z + dz)` is on the grid and walkable.
    fn walkable_at(&self, x: usize, z: usize, dx: isize, dz: isize) -> bool {
        match (x.checked_add_signed(dx), z.checked_add_signed(dz)) {
            (Some(x), Some(z)) => x < self.width && z < self.depth && self.is_walkable(x, z),
            _ => false,
        }
    }

    /// Whether a cell is at the edge of the floor: next to a wall, even across a corner.
    fn is_edge(&self, x: usize, z: usize) -> bool {
        (-1..=1).any(|dz| (-1..=1).any(|dx| !self.walkable_at(x, z, dx, dz)))
    }

    /// The cheapest ways from `start` to every walkable cell it connects to, going across
    /// corners as well as along the grid, but never cutting past a wall's corner. Steps into
    /// cells at the edge of the floor are dearer, so that routes keep to the middle. The search
    /// goes no further than routes costing `within`: cells much further away are left unreached.
    pub fn routes_from(&self, start: (usize, usize), within: f32) -> Routes {
        self.routes_from_weighted(start, within, |_, _| 0.0)
    }

    /// As [`Self::routes_from`], but each step into a cell `(x, z)` costs `extra(x, z)` more -
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
            let (x, z) = (at % self.width, at / self.width);
            for (dx, dz) in [
                (-1, 0),
                (1, 0),
                (0, -1),
                (0, 1),
                (-1, -1),
                (1, -1),
                (-1, 1),
                (1, 1),
            ] {
                if !self.walkable_at(x, z, dx, dz) {
                    continue;
                }
                let diagonal = dx != 0 && dz != 0;
                if diagonal && !(self.walkable_at(x, z, dx, 0) && self.walkable_at(x, z, 0, dz)) {
                    continue;
                }
                let (nx, nz) = (x.wrapping_add_signed(dx), z.wrapping_add_signed(dz));
                let step = if diagonal {
                    std::f32::consts::SQRT_2
                } else {
                    1.0
                };
                let weight = if self.is_edge(nx, nz) { EDGE_COST } else { 1.0 };
                let next = cost + step * (weight + extra(nx, nz).max(0.0));
                let there = nz * self.width + nx;
                if routes.costs[there].is_none_or(|best| next < best) {
                    routes.costs[there] = Some(next);
                    routes.previous[there] = at;
                    frontier.push(Frontier(next, there));
                }
            }
        }
        routes
    }

    pub fn walkable_cells(&self) -> impl Iterator<Item = (usize, usize)> + '_ {
        (0..self.depth)
            .flat_map(move |z| (0..self.width).map(move |x| (x, z)))
            .filter(|&(x, z)| self.is_walkable(x, z))
    }

    /// Walking distance, in cells, from `start` to every cell; `None` where it cannot be reached.
    pub fn distances_from(&self, start: (usize, usize)) -> Vec<Option<u32>> {
        let mut distances = vec![None; self.cells.len()];
        if !self.is_walkable(start.0, start.1) {
            return distances;
        }
        distances[start.1 * self.width + start.0] = Some(0);
        let mut queue = VecDeque::from([start]);
        while let Some((x, z)) = queue.pop_front() {
            let next = distances[z * self.width + x].unwrap() + 1;
            let neighbours = [
                (x.wrapping_sub(1), z),
                (x + 1, z),
                (x, z.wrapping_sub(1)),
                (x, z + 1),
            ];
            for (nx, nz) in neighbours {
                if nx >= self.width || nz >= self.depth || !self.is_walkable(nx, nz) {
                    continue;
                }
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
    /// the middle of cell `b` - nothing but floor between them, so that one can be seen from the
    /// other, going by the ground alone.
    pub fn sees_across(&self, a: (usize, usize), b: (usize, usize)) -> bool {
        let (ax, az) = (a.0 as f32 + 0.5, a.1 as f32 + 0.5);
        let (dx, dz) = (b.0 as f32 - a.0 as f32, b.1 as f32 - a.1 as f32);
        // A few samples a cell, so that the line cannot slip past the corner of a wall.
        let samples = ((dx.abs().max(dz.abs())) * 3.0).ceil().max(1.0) as usize;
        (0..=samples).all(|i| {
            let t = i as f32 / samples as f32;
            let (x, z) = (ax + dx * t, az + dz * t);
            let (x, z) = (x.floor(), z.floor());
            x >= 0.0
                && z >= 0.0
                && (x as usize) < self.width
                && (z as usize) < self.depth
                && self.is_walkable(x as usize, z as usize)
        })
    }

    /// The middle of cell `(x, z)` in the world, at height 0, with the grid's corner at `origin`.
    pub fn center(&self, origin: Vector3<f32>, (x, z): (usize, usize)) -> Vector3<f32> {
        cell_center(origin, self.cell_size, x, z)
    }

    /// The middle of cell `(x, z)`, on its floor.
    pub fn on_floor(&self, origin: Vector3<f32>, (x, z): (usize, usize)) -> Vector3<f32> {
        self.center(origin, (x, z)) + Vector3::new(0.0, self.floor(x, z), 0.0)
    }

    /// The cell that `position` is in, if it is on the grid at all.
    pub fn cell_at(&self, origin: Vector3<f32>, position: Vector3<f32>) -> Option<(usize, usize)> {
        let x = ((position.x - origin.x) / self.cell_size).floor();
        let z = ((position.z - origin.z) / self.cell_size).floor();
        (x >= 0.0 && z >= 0.0 && (x as usize) < self.width && (z as usize) < self.depth)
            .then_some((x as usize, z as usize))
    }

    /// The walkable cell nearest `point`, as the crow flies.
    pub fn nearest_walkable(&self, origin: Vector3<f32>, point: Vector3<f32>) -> Option<(usize, usize)> {
        self.walkable_cells().min_by(|&a, &b| {
            let distance = |cell| crate::flat(self.center(origin, cell) - point).norm_squared();
            distance(a).total_cmp(&distance(b))
        })
    }

    /// The walkable cell `point` is in, or failing that the nearest one.
    pub fn walkable_cell(&self, origin: Vector3<f32>, point: Vector3<f32>) -> Option<(usize, usize)> {
        self.cell_at(origin, point)
            .filter(|&(x, z)| self.is_walkable(x, z))
            .or_else(|| self.nearest_walkable(origin, point))
    }
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

    #[test]
    fn rng_stays_in_range() {
        let mut rng = Rng::new(7);
        assert!((0..1000).all(|_| rng.below(5) < 5));
    }
}
