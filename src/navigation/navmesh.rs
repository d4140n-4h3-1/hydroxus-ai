//! A navmesh: a level's walkable floor as convex polygons, and the ways between them.
//!
//! It is built from a surveyed [`WalkGrid`] - which has already found where there is floor, how
//! high it is, on how many storeys, and where a step up or down a stair can be taken - by merging
//! its cells into as few rectangles as it can: each on one storey, its floor level to within
//! [`FLAT`], and all of one area (a road, a doorway that can be shut - whatever the game tells
//! apart). Where a step can be taken from one rectangle into another, a portal joins them: the
//! stretch of their shared edge it can be taken along.
//!
//! A city's grid is millions of cells; its navmesh, a few hundred thousand polygons at most, and a
//! route over it a search over polygons, not cells - straightened out afterwards by pulling it
//! taut through the portals (the funnel), so that it cuts every corner it can and no more.
//!
//! Once built, the grid can go: everything a route needs is here.

use crate::navigation::grid::WalkGrid;
use nalgebra::Vector3;
use std::{
    cmp::Ordering,
    collections::{BinaryHeap, HashMap},
};

/// How far a polygon's floor may be above or below its first cell's, in meters: a floor level
/// enough to walk as one, so that each step of a stair is a polygon of its own.
pub const FLAT: f32 = 0.05;
/// How many cells across each square of the index of where the polygons are is.
const BUCKET: usize = 32;

/// A polygon of the navmesh, by number.
pub type Poly = u32;

/// One of the navmesh's polygons: a rectangle of the grid's cells `[x0, x1)` across and `[z0, z1)`
/// down the plan, on one storey, its floor at one height - over the navmesh's origin - and of one
/// area.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Polygon {
    pub x0: u32,
    pub z0: u32,
    pub x1: u32,
    pub z1: u32,
    pub storey: u16,
    pub area: u8,
    pub floor: f32,
    /// Its portals: from `first`, so many of them.
    first: u32,
    count: u16,
}

/// A way from one polygon into another: the polygon it leads to, and the ends of the stretch of
/// their shared edge it can be crossed along, on the plan, in the world's x and z.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Portal {
    pub to: Poly,
    pub a: [f32; 2],
    pub b: [f32; 2],
}

impl Portal {
    fn middle(&self) -> [f32; 2] {
        [(self.a[0] + self.b[0]) / 2.0, (self.a[1] + self.b[1]) / 2.0]
    }
}

/// The navmesh.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct NavMesh {
    /// Where the grid's corner was, and how big its cells, in meters; and how many there were
    /// across and down its plan.
    pub origin: Vector3<f32>,
    pub cell_size: f32,
    width: usize,
    depth: usize,
    polygons: Vec<Polygon>,
    portals: Vec<Portal>,
    /// For each square of [`BUCKET`] cells of the plan, row by row, the polygons over it.
    buckets: Vec<Vec<Poly>>,
    /// Which polygons are shut, for the while: a door closed, a barrier up.
    blocked: Vec<bool>,
}

/// A side of a polygon: the cells along it inside, and the step out across it.
type Side = (Vec<(u32, u32)>, (isize, isize));
/// A polygon a search has reached: where it was entered, the cost to there, and from which
/// polygon, by which of its portals.
type Reached = ([f32; 2], f32, Option<(Poly, usize)>);

/// What came of a search over the navmesh for a route.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Open {
    cost: f32,
    estimate: f32,
    poly: Poly,
}

impl Eq for Open {}

impl Ord for Open {
    fn cmp(&self, other: &Self) -> Ordering {
        other.estimate.total_cmp(&self.estimate).then_with(|| other.cost.total_cmp(&self.cost))
    }
}

impl PartialOrd for Open {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// `b - a` crossed with `c - a`, on the plan: which side of the line from `a` through `b` `c` is.
fn side(a: [f32; 2], b: [f32; 2], c: [f32; 2]) -> f32 {
    (c[0] - a[0]) * (b[1] - a[1]) - (b[0] - a[0]) * (c[1] - a[1])
}

fn distance(a: [f32; 2], b: [f32; 2]) -> f32 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

impl NavMesh {
    /// The navmesh of `grid`, whose corner is at `origin`: its walkable cells merged into
    /// rectangles of one storey, a level floor and one `area` - as `area` says of each cell, by
    /// `(x, row)` - and joined where a step can be taken from one into the next.
    pub fn build(grid: &WalkGrid, origin: Vector3<f32>, area: impl Fn(usize, usize) -> u8) -> Self {
        Self::build_by(grid, origin, &area)
    }

    /// [`NavMesh::build`], not generic (see [`NavMesh::route_by`]).
    fn build_by(grid: &WalkGrid, origin: Vector3<f32>, area: &dyn Fn(usize, usize) -> u8) -> Self {
        let (width, depth) = (grid.width, grid.depth);
        let mut polygons = Vec::new();
        let mut taken = vec![0u64; (width * depth).div_ceil(64)];
        for storey in 0..grid.storeys {
            taken.iter_mut().for_each(|bits| *bits = 0);
            let is_taken = |taken: &[u64], i: usize| taken[i / 64] & (1 << (i % 64)) != 0;
            for z in 0..depth {
                for x in 0..width {
                    let row = z + storey * depth;
                    if !grid.is_walkable(x, row) || is_taken(&taken, z * width + x) {
                        continue;
                    }
                    let (floor, kind) = (grid.floor(x, row), area(x, row));
                    let fits = |taken: &[u64], cx: usize, cz: usize| {
                        let row = cz + storey * depth;
                        grid.is_walkable(cx, row)
                            && !is_taken(taken, cz * width + cx)
                            && (grid.floor(cx, row) - floor).abs() <= FLAT
                            && area(cx, row) == kind
                    };
                    let mut x1 = x + 1;
                    while x1 < width && fits(&taken, x1, z) {
                        x1 += 1;
                    }
                    let mut z1 = z + 1;
                    while z1 < depth && (x..x1).all(|cx| fits(&taken, cx, z1)) {
                        z1 += 1;
                    }
                    for cz in z..z1 {
                        for cx in x..x1 {
                            let i = cz * width + cx;
                            taken[i / 64] |= 1 << (i % 64);
                        }
                    }
                    polygons.push(Polygon {
                        x0: x as u32,
                        z0: z as u32,
                        x1: x1 as u32,
                        z1: z1 as u32,
                        storey: storey as u16,
                        area: kind,
                        floor,
                        first: 0,
                        count: 0,
                    });
                }
            }
        }
        let mut mesh = Self {
            origin,
            cell_size: grid.cell_size,
            width,
            depth,
            blocked: vec![false; polygons.len()],
            polygons,
            portals: Vec::new(),
            buckets: vec![Vec::new(); width.div_ceil(BUCKET) * depth.div_ceil(BUCKET)],
        };
        for (n, p) in mesh.polygons.iter().enumerate() {
            for bz in p.z0 as usize / BUCKET..=(p.z1 as usize - 1) / BUCKET {
                for bx in p.x0 as usize / BUCKET..=(p.x1 as usize - 1) / BUCKET {
                    mesh.buckets[bz * width.div_ceil(BUCKET) + bx].push(n as Poly);
                }
            }
        }
        mesh.join(grid);
        mesh
    }

    /// The portals between the polygons: along each edge of each, cell by cell, wherever the grid
    /// steps across it into another polygon, a stretch for each polygon it steps into.
    fn join(&mut self, grid: &WalkGrid) {
        let cs = self.cell_size;
        let (ox, oz) = (self.origin.x, self.origin.z);
        let mut portals = Vec::new();
        for n in 0..self.polygons.len() {
            let p = self.polygons[n];
            let first = portals.len() as u32;
            let row_of = |z: u32| z as usize + p.storey as usize * self.depth;
            // Each side: the cells along it inside, the step out across it, and where along the
            // edge each cell's stretch runs, in the world.
            let sides: [Side; 4] = [
                ((p.z0..p.z1).map(|z| (p.x1 - 1, z)).collect(), (1, 0)),
                ((p.z0..p.z1).map(|z| (p.x0, z)).collect(), (-1, 0)),
                ((p.x0..p.x1).map(|x| (x, p.z1 - 1)).collect(), (0, 1)),
                ((p.x0..p.x1).map(|x| (x, p.z0)).collect(), (0, -1)),
            ];
            for (cells, (dx, dz)) in sides {
                let mut run: Option<(Poly, [f32; 2], [f32; 2])> = None;
                for (x, z) in cells {
                    let to = grid
                        .step_to((x as usize, row_of(z)), dx, dz)
                        .and_then(|(tx, trow)| self.over(grid.plan((tx, trow)), grid.storey((tx, trow))));
                    // This cell's stretch of the edge.
                    let (a, b) = match (dx, dz) {
                        (1, _) => ([ox + (x + 1) as f32 * cs, oz + z as f32 * cs], [ox + (x + 1) as f32 * cs, oz + (z + 1) as f32 * cs]),
                        (-1, _) => ([ox + x as f32 * cs, oz + z as f32 * cs], [ox + x as f32 * cs, oz + (z + 1) as f32 * cs]),
                        (_, 1) => ([ox + x as f32 * cs, oz + (z + 1) as f32 * cs], [ox + (x + 1) as f32 * cs, oz + (z + 1) as f32 * cs]),
                        _ => ([ox + x as f32 * cs, oz + z as f32 * cs], [ox + (x + 1) as f32 * cs, oz + z as f32 * cs]),
                    };
                    run = match (run, to) {
                        (Some((q, start, _)), Some(t)) if q == t => Some((q, start, b)),
                        (previous, to) => {
                            if let Some((q, start, end)) = previous {
                                portals.push(Portal { to: q, a: start, b: end });
                            }
                            to.filter(|&t| t != n as Poly).map(|t| (t, a, b))
                        }
                    };
                }
                if let Some((q, start, end)) = run {
                    portals.push(Portal { to: q, a: start, b: end });
                }
            }
            let p = &mut self.polygons[n];
            p.first = first;
            p.count = (portals.len() as u32 - first) as u16;
        }
        self.portals = portals;
    }

    /// The polygon over cell `(x, z)` of the plan on `storey`.
    fn over(&self, (x, z): (usize, usize), storey: usize) -> Option<Poly> {
        if x >= self.width || z >= self.depth {
            return None;
        }
        let bucket = &self.buckets[(z / BUCKET) * self.width.div_ceil(BUCKET) + x / BUCKET];
        bucket.iter().copied().find(|&n| {
            let p = &self.polygons[n as usize];
            p.storey as usize == storey && (p.x0..p.x1).contains(&(x as u32)) && (p.z0..p.z1).contains(&(z as u32))
        })
    }

    /// How many polygons there are, and portals.
    pub fn len(&self) -> usize {
        self.polygons.len()
    }

    pub fn is_empty(&self) -> bool {
        self.polygons.is_empty()
    }

    pub fn portal_count(&self) -> usize {
        self.portals.len()
    }

    /// Polygon `n`.
    pub fn polygon(&self, n: Poly) -> &Polygon {
        &self.polygons[n as usize]
    }

    /// The portals out of polygon `n`.
    pub fn portals(&self, n: Poly) -> &[Portal] {
        let p = &self.polygons[n as usize];
        &self.portals[p.first as usize..p.first as usize + p.count as usize]
    }

    /// The middle of polygon `n`, on its floor, in the world.
    pub fn centre(&self, n: Poly) -> Vector3<f32> {
        let p = &self.polygons[n as usize];
        let cs = self.cell_size;
        self.origin
            + Vector3::new((p.x0 + p.x1) as f32 * cs / 2.0, p.floor, (p.z0 + p.z1) as f32 * cs / 2.0)
    }

    /// How big polygon `n`'s floor is, in square meters.
    pub fn area_of(&self, n: Poly) -> f32 {
        let p = &self.polygons[n as usize];
        ((p.x1 - p.x0) * (p.z1 - p.z0)) as f32 * self.cell_size * self.cell_size
    }

    /// The floor's height over the world's origin in polygon `n`.
    pub fn floor_of(&self, n: Poly) -> f32 {
        self.origin.y + self.polygons[n as usize].floor
    }

    /// Shuts polygon `n`, or opens it again: a door, a barrier.
    pub fn set_blocked(&mut self, n: Poly, blocked: bool) {
        self.blocked[n as usize] = blocked;
    }

    pub fn is_blocked(&self, n: Poly) -> bool {
        self.blocked[n as usize]
    }

    /// The polygon `at` is in: over its spot on the plan, with its floor nearest its height - of
    /// those within a storey's height of it, [`crate::navigation::grid::LEVEL`]. None if there is
    /// no floor there.
    pub fn poly_at(&self, at: Vector3<f32>) -> Option<Poly> {
        let x = ((at.x - self.origin.x) / self.cell_size).floor();
        let z = ((at.z - self.origin.z) / self.cell_size).floor();
        if !(x >= 0.0 && z >= 0.0 && (x as usize) < self.width && (z as usize) < self.depth) {
            return None;
        }
        let (x, z) = (x as u32, z as u32);
        let bucket = &self.buckets[(z as usize / BUCKET) * self.width.div_ceil(BUCKET) + x as usize / BUCKET];
        bucket
            .iter()
            .copied()
            .filter(|&n| {
                let p = &self.polygons[n as usize];
                (p.x0..p.x1).contains(&x) && (p.z0..p.z1).contains(&z)
            })
            .map(|n| (n, (self.floor_of(n) - at.y).abs()))
            .filter(|&(_, off)| off <= crate::navigation::grid::LEVEL)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(n, _)| n)
    }

    /// The polygon nearest `at`, a storey's height counting as far as
    /// [`crate::navigation::grid::STOREY_AWAY`] cells across: looked for in rings of buckets round
    /// it, nearest first. None on an empty navmesh.
    pub fn nearest(&self, at: Vector3<f32>) -> Option<Poly> {
        if let Some(n) = self.poly_at(at) {
            return Some(n);
        }
        let cols = self.width.div_ceil(BUCKET) as isize;
        let rows = self.depth.div_ceil(BUCKET) as isize;
        let bx = (((at.x - self.origin.x) / self.cell_size) as isize / BUCKET as isize).clamp(0, cols - 1);
        let bz = (((at.z - self.origin.z) / self.cell_size) as isize / BUCKET as isize).clamp(0, rows - 1);
        let away = |n: Poly| {
            let p = &self.polygons[n as usize];
            let cs = self.cell_size;
            let (x, z) = ((at.x - self.origin.x) / cs, (at.z - self.origin.z) / cs);
            let dx = (p.x0 as f32 - x).max(x - p.x1 as f32).max(0.0);
            let dz = (p.z0 as f32 - z).max(z - p.z1 as f32).max(0.0);
            dx.hypot(dz) + (self.floor_of(n) - at.y).abs() / crate::navigation::grid::LEVEL * crate::navigation::grid::STOREY_AWAY
        };
        let mut best: Option<(Poly, f32)> = None;
        for ring in 0..cols.max(rows) {
            // Nothing in a ring further out can be nearer than this, in cells.
            if best.is_some_and(|(_, d)| d < ((ring - 1).max(0) * BUCKET as isize) as f32) {
                break;
            }
            for dz in -ring..=ring {
                for dx in -ring..=ring {
                    if dx.abs() != ring && dz.abs() != ring {
                        continue;
                    }
                    let (cx, cz) = (bx + dx, bz + dz);
                    if cx < 0 || cz < 0 || cx >= cols || cz >= rows {
                        continue;
                    }
                    for &n in &self.buckets[(cz * cols + cx) as usize] {
                        let d = away(n);
                        if best.is_none_or(|(_, b)| d < b) {
                            best = Some((n, d));
                        }
                    }
                }
            }
        }
        best.map(|(n, _)| n)
    }

    /// The polygons from `from` to `to`, the way that costs least - each meter across polygon `n`
    /// costing `1 + extra(n)` - looked for no further than `within` of that cost; and the portal
    /// taken out of each, by index into its own. None with no way there, or none within reach.
    fn corridor(
        &self,
        (from, start): (Poly, [f32; 2]),
        (to, goal): (Poly, [f32; 2]),
        within: f32,
        extra: &dyn Fn(Poly) -> f32,
    ) -> Option<Vec<(Poly, usize)>> {
        if self.blocked[from as usize] && from != to || self.blocked[to as usize] {
            return None;
        }
        // Where each polygon reached was entered, the cost to there, and from which polygon by
        // which of its portals - only for those reached, so that a short search costs little
        // however big the navmesh.
        let mut best: HashMap<Poly, Reached> = HashMap::new();
        let mut open = BinaryHeap::new();
        best.insert(from, (start, 0.0, None));
        open.push(Open { cost: 0.0, estimate: distance(start, goal), poly: from });
        while let Some(Open { cost, poly, .. }) = open.pop() {
            let (at, so_far, _) = best[&poly];
            if cost > so_far {
                continue;
            }
            if poly == to {
                let mut corridor = Vec::new();
                let mut n = to;
                while let Some((back, portal)) = best[&n].2 {
                    corridor.push((back, portal));
                    n = back;
                }
                corridor.reverse();
                return Some(corridor);
            }
            let dear = 1.0 + extra(poly);
            for (k, portal) in self.portals(poly).iter().enumerate() {
                if self.blocked[portal.to as usize] {
                    continue;
                }
                let through = if portal.to == to { goal } else { portal.middle() };
                let cost = so_far + distance(at, through) * dear;
                if cost > within {
                    continue;
                }
                if best.get(&portal.to).is_none_or(|&(_, c, _)| cost < c) {
                    best.insert(portal.to, (through, cost, Some((poly, k))));
                    open.push(Open { cost, estimate: cost + distance(through, goal), poly: portal.to });
                }
            }
        }
        None
    }

    /// A route from `feet` to `to`: the way that costs least, each meter across polygon `n`
    /// costing `1 + extra(n)`, within `within` of that cost - `f32::INFINITY` for anywhere - pulled
    /// taut through the portals it goes by, kept `clearance` meters in from their ends, and ending
    /// at `to` itself, on the floor. As the grid's routes are, its points last first, the start
    /// left out. Empty with no way there.
    pub fn route(
        &self,
        feet: Vector3<f32>,
        to: Vector3<f32>,
        within: f32,
        clearance: f32,
        extra: impl Fn(Poly) -> f32,
    ) -> Vec<Vector3<f32>> {
        self.route_by(feet, to, within, clearance, &extra)
    }

    /// [`NavMesh::route`], not generic, so that it is compiled - optimized - here, whatever the
    /// game's own code is built as: a generic function is built in the crate that calls it.
    fn route_by(
        &self,
        feet: Vector3<f32>,
        to: Vector3<f32>,
        within: f32,
        clearance: f32,
        extra: &dyn Fn(Poly) -> f32,
    ) -> Vec<Vector3<f32>> {
        let (Some(from), Some(goal)) = (self.nearest(feet), self.nearest(to)) else {
            return Vec::new();
        };
        let start = [feet.x, feet.z];
        let end = [to.x, to.z];
        let Some(corridor) = self.corridor((from, start), (goal, end), within, extra) else {
            return Vec::new();
        };
        // The portals, each end left or right as seen going through it, drawn in from each end.
        let mut gates: Vec<([f32; 2], [f32; 2], Poly)> = vec![(start, start, from)];
        let mut came = start;
        for &(poly, k) in &corridor {
            let portal = self.portals(poly)[k];
            let length = distance(portal.a, portal.b);
            let inset = clearance.min(length / 2.0) / length.max(1.0e-6);
            let a = [portal.a[0] + (portal.b[0] - portal.a[0]) * inset, portal.a[1] + (portal.b[1] - portal.a[1]) * inset];
            let b = [portal.b[0] + (portal.a[0] - portal.b[0]) * inset, portal.b[1] + (portal.a[1] - portal.b[1]) * inset];
            let middle = portal.middle();
            let (left, right) = if side(came, middle, a) > 0.0 { (b, a) } else { (a, b) };
            gates.push((left, right, portal.to));
            came = middle;
        }
        gates.push((end, end, goal));
        // Pulled taut: the funnel.
        let mut points: Vec<([f32; 2], Poly)> = Vec::new();
        let (mut apex, mut left, mut right) = (start, start, start);
        let (mut left_at, mut right_at) = (0, 0);
        let mut i = 1;
        while i < gates.len() {
            let (l, r, _) = gates[i];
            if side(apex, right, r) <= 0.0 {
                if apex == right || side(apex, left, r) > 0.0 {
                    (right, right_at) = (r, i);
                } else {
                    points.push((left, gates[left_at].2));
                    let at = left_at;
                    apex = left;
                    (left, right, left_at, right_at) = (apex, apex, at, at);
                    i = at + 1;
                    continue;
                }
            }
            if side(apex, left, l) >= 0.0 {
                if apex == left || side(apex, right, l) < 0.0 {
                    (left, left_at) = (l, i);
                } else {
                    points.push((right, gates[right_at].2));
                    let at = right_at;
                    apex = right;
                    (left, right, left_at, right_at) = (apex, apex, at, at);
                    i = at + 1;
                    continue;
                }
            }
            i += 1;
        }
        let mut route: Vec<Vector3<f32>> = points
            .into_iter()
            .filter(|&(at, _)| at != start)
            .map(|(at, poly)| Vector3::new(at[0], self.floor_of(poly), at[1]))
            .collect();
        route.push(Vector3::new(to.x, self.floor_of(goal), to.z));
        route.dedup();
        route.reverse();
        route
    }

    /// The cost, as [`NavMesh::route`] reckons it, of the way from `feet` to every polygon within
    /// `within` of it - each meter across polygon `n` costing `1 + extra(n)` - by way of the
    /// middles of the portals: for how far a noise carries, what can be got to, how far off along
    /// the floor anything is.
    pub fn costs_from(&self, feet: Vector3<f32>, within: f32, extra: impl Fn(Poly) -> f32) -> HashMap<Poly, f32> {
        self.costs_by(feet, within, &extra)
    }

    /// [`NavMesh::costs_from`], not generic (see [`NavMesh::route_by`]).
    fn costs_by(&self, feet: Vector3<f32>, within: f32, extra: &dyn Fn(Poly) -> f32) -> HashMap<Poly, f32> {
        let mut best: HashMap<Poly, ([f32; 2], f32)> = HashMap::new();
        let Some(from) = self.nearest(feet) else {
            return HashMap::new();
        };
        let mut open = BinaryHeap::new();
        best.insert(from, ([feet.x, feet.z], 0.0));
        open.push(Open { cost: 0.0, estimate: 0.0, poly: from });
        while let Some(Open { cost, poly, .. }) = open.pop() {
            let (at, so_far) = best[&poly];
            if cost > so_far {
                continue;
            }
            let dear = 1.0 + extra(poly);
            for portal in self.portals(poly) {
                if self.blocked[portal.to as usize] {
                    continue;
                }
                let through = portal.middle();
                let cost = so_far + distance(at, through) * dear;
                if cost > within {
                    continue;
                }
                if best.get(&portal.to).is_none_or(|&(_, c)| cost < c) {
                    best.insert(portal.to, (through, cost));
                    open.push(Open { cost, estimate: cost, poly: portal.to });
                }
            }
        }
        best.into_iter().map(|(n, (_, cost))| (n, cost)).collect()
    }

    /// How much memory it takes, about, in bytes.
    pub fn bytes(&self) -> usize {
        self.polygons.len() * std::mem::size_of::<Polygon>()
            + self.portals.len() * std::mem::size_of::<Portal>()
            + self.buckets.iter().map(|b| b.len() * 4 + std::mem::size_of::<Vec<Poly>>()).sum::<usize>()
            + self.blocked.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flat;

    /// A grid `width` by `depth` cells, half a meter each, walkable where `open` says, its floor 0.
    fn grid(width: usize, depth: usize, open: impl Fn(usize, usize) -> bool) -> WalkGrid {
        let mut grid = WalkGrid::new(width, depth, 0.5);
        for z in 0..depth {
            for x in 0..width {
                grid.set(x, z, open(x, z));
            }
        }
        grid
    }

    /// Whether every point of `route`, and the way between each and the next, is over floor.
    fn on_floor(mesh: &NavMesh, from: Vector3<f32>, route: &[Vector3<f32>]) -> bool {
        let mut at = from;
        for &next in route.iter().rev() {
            for k in 0..=20 {
                let p = at + (next - at) * (k as f32 / 20.0);
                if mesh.poly_at(Vector3::new(p.x, 0.0, p.z)).is_none() {
                    return false;
                }
            }
            at = next;
        }
        true
    }

    #[test]
    fn an_open_floor_is_one_polygon_and_the_way_across_it_is_straight() {
        let grid = grid(40, 30, |_, _| true);
        let mesh = NavMesh::build(&grid, Vector3::zeros(), |_, _| 0);
        assert_eq!(mesh.len(), 1);
        let to = Vector3::new(18.0, 0.0, 13.0);
        assert_eq!(mesh.route(Vector3::new(1.0, 0.0, 1.0), to, f32::INFINITY, 0.3, |_| 0.0), vec![to]);
    }

    #[test]
    fn round_a_corner_it_cuts_it_as_close_as_it_can_and_no_closer() {
        // An L: along the bottom, then up the right-hand side; a wall in the corner of the L.
        let grid = grid(40, 40, |x, z| z < 8 || x >= 32);
        let mesh = NavMesh::build(&grid, Vector3::zeros(), |_, _| 0);
        assert_eq!(mesh.len(), 2);
        let from = Vector3::new(1.0, 0.0, 2.0);
        let to = Vector3::new(18.0, 0.0, 19.0);
        let route = mesh.route(from, to, f32::INFINITY, 0.3, |_| 0.0);
        assert_eq!(route.len(), 2, "one turn, at the corner: {route:?}");
        let corner = route[1];
        assert!((corner.x - (16.0 + 0.3)).abs() < 0.05 && (corner.z - 4.0).abs() < 0.05, "{corner:?}");
        assert!(on_floor(&mesh, from, &route));
    }

    #[test]
    fn a_maze_is_found_through_and_a_walled_off_room_is_not() {
        // Corridors two cells wide in a serpentine, and a room off on its own.
        let open = |x: usize, z: usize| {
            let band = z / 4;
            let wall = z % 4 >= 2;
            if x >= 60 {
                return z < 10;
            }
            if x >= 50 {
                return false;
            }
            !wall || if band.is_multiple_of(2) { x >= 46 } else { x < 2 }
        };
        let grid = grid(70, 40, open);
        let mesh = NavMesh::build(&grid, Vector3::zeros(), |_, _| 0);
        let from = Vector3::new(0.5, 0.0, 0.5);
        let to = Vector3::new(0.5, 0.0, 16.5);
        let route = mesh.route(from, to, f32::INFINITY, 0.25, |_| 0.0);
        assert!(!route.is_empty());
        assert!(on_floor(&mesh, from, &route));
        let length: f32 = route.windows(2).map(|w| flat(w[0] - w[1]).norm()).sum::<f32>() + flat(route[route.len() - 1] - from).norm();
        assert!(length > 4.0 * 20.0, "it goes back and forth: {length}");
        assert!(mesh.route(from, Vector3::new(32.0, 0.0, 2.0), f32::INFINITY, 0.25, |_| 0.0).is_empty());
        // Nor anywhere beyond its reach.
        assert!(mesh.route(from, to, 30.0, 0.25, |_| 0.0).is_empty());
    }

    #[test]
    fn up_a_stair_each_step_is_its_own_and_the_route_climbs_it() {
        // A corridor up a stair of eight steps 0.2 m each, then a landing.
        let mut grid = grid(40, 4, |_, _| true);
        for z in 0..4 {
            for x in 0..40 {
                grid.set_floor(x, z, if x < 10 { 0.0 } else if x < 18 { (x - 9) as f32 * 0.2 } else { 1.8 });
            }
        }
        let mesh = NavMesh::build(&grid, Vector3::zeros(), |_, _| 0);
        assert_eq!(mesh.len(), 10);
        let route = mesh.route(Vector3::new(1.0, 0.0, 1.0), Vector3::new(19.0, 1.8, 1.0), f32::INFINITY, 0.25, |_| 0.0);
        assert_eq!(route.first().map(|p| p.y), Some(1.8));
        assert_eq!(mesh.poly_at(Vector3::new(19.0, 1.8, 1.0)).map(|n| mesh.floor_of(n)), Some(1.8));
    }

    #[test]
    fn a_floor_over_another_is_its_own_and_found_by_height() {
        let mut grid = WalkGrid::with_storeys(20, 20, 2, 0.5);
        for z in 0..20 {
            for x in 0..20 {
                grid.set(x, z, true);
                grid.set(x, z + 20, x < 10);
                grid.set_floor(x, z + 20, 4.0);
            }
        }
        let mesh = NavMesh::build(&grid, Vector3::zeros(), |_, _| 0);
        assert_eq!(mesh.len(), 2);
        let ground = mesh.poly_at(Vector3::new(2.0, 0.1, 2.0)).unwrap();
        let upstairs = mesh.poly_at(Vector3::new(2.0, 3.9, 2.0)).unwrap();
        assert_ne!(ground, upstairs);
        assert_eq!(mesh.floor_of(upstairs), 4.0);
        assert!(mesh.portals(ground).is_empty() && mesh.portals(upstairs).is_empty(), "no stair between them");
    }

    #[test]
    fn dear_ground_is_gone_round_and_a_shut_door_is_not_gone_through() {
        // Open ground, a road (area 1) across the middle but at a crossing (x 30 to 34), which can
        // be shut (area 2).
        let grid = grid(60, 30, |_, _| true);
        let road = |x: usize, z: usize| match ((12..18).contains(&z), (30..34).contains(&x)) {
            (true, false) => 1,
            (true, true) => 2,
            _ => 0,
        };
        let mesh = NavMesh::build(&grid, Vector3::zeros(), road);
        let from = Vector3::new(2.0, 0.0, 2.0);
        let to = Vector3::new(2.0, 0.0, 13.0);
        let straight = mesh.route(from, to, f32::INFINITY, 0.25, |_| 0.0);
        assert_eq!(straight.len(), 1);
        let dear = |n: Poly| if mesh.polygon(n).area == 1 { 20.0 } else { 0.0 };
        let round = mesh.route(from, to, f32::INFINITY, 0.25, dear);
        assert!(round.iter().any(|p| p.x > 14.0), "by the crossing: {round:?}");
        // The crossing shut: no way over but across the road.
        let mut shut = mesh.clone();
        for n in 0..shut.len() as Poly {
            let p = *shut.polygon(n);
            if p.area == 2 {
                shut.set_blocked(n, true);
            }
        }
        let across = shut.route(from, Vector3::new(16.0, 0.0, 7.5), f32::INFINITY, 0.25, |_| 0.0);
        assert!(across.is_empty(), "into the shut crossing: {across:?}");
    }

    #[test]
    fn the_cost_to_everywhere_near_is_found_and_no_further() {
        let grid = grid(200, 4, |_, _| true);
        // Every 10 cells another area, to have polygons along it.
        let mesh = NavMesh::build(&grid, Vector3::zeros(), |x, _| (x / 10) as u8);
        let costs = mesh.costs_from(Vector3::new(1.0, 0.0, 1.0), 30.0, |_| 0.0);
        let reached = |x: f32| mesh.poly_at(Vector3::new(x, 0.0, 1.0)).is_some_and(|n| costs.contains_key(&n));
        assert!(reached(20.0) && reached(29.0));
        assert!(!reached(40.0));
    }

    #[test]
    fn the_nearest_polygon_to_somewhere_off_the_floor_is_found() {
        let grid = grid(100, 100, |x, z| x > 80 && z > 80);
        let mesh = NavMesh::build(&grid, Vector3::zeros(), |_, _| 0);
        assert_eq!(mesh.poly_at(Vector3::new(1.0, 0.0, 1.0)), None);
        assert!(mesh.nearest(Vector3::new(1.0, 0.0, 1.0)).is_some());
    }
}
