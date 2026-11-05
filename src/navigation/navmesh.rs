//! A navmesh: a level's walkable floor as convex polygons, and the ways between them.
//!
//! It is built from a surveyed [`WalkGrid`] - which has already found where there is floor, how
//! high it is, on how many storeys, and where a step up or down a stair can be taken - by merging
//! its cells into as few rectangles as it can: each on one storey, its floor level to within
//! [`FLAT`], all of one area (a road, a doorway that can be shut - whatever the game tells
//! apart), and no more than [`MOST_CELLS`] along a side. Where a step can be taken from one rectangle into another, a portal joins them: the
//! stretch of their shared edge it can be taken along.
//!
//! A city's grid is millions of cells; its navmesh, a few hundred thousand polygons at most, and a
//! route over it a search over polygons, not cells - straightened out afterwards by pulling it
//! taut through the portals (the funnel), so that it cuts every corner it can and no more.
//!
//! Once built, the grid can go: everything a route needs is here.

use crate::navigation::{grid::{self, WalkGrid}, walk};
use nalgebra::Vector3;
use std::{
    cmp::Ordering,
    collections::{BinaryHeap, HashMap},
};

/// How far a polygon's floor may be above or below its first cell's, in meters: a floor level
/// enough to walk as one, so that each step of a stair is a polygon of its own.
pub const FLAT: f32 = 0.05;
/// How many cells along a side a polygon has at most: small enough that what is said of one -
/// that it is a gang's turf, that the enemy can see it - is true of all of it, near enough.
pub const MOST_CELLS: usize = 16;

tuning!(
    /// How a navmesh is made from a grid (see the module's constants, its defaults).
    {
        /// How far apart the floors of the cells of one polygon can be, in meters.
        flat: f32 = FLAT,
        /// How many cells along a side a polygon has at most.
        most_cells: usize = MOST_CELLS,
    }
);
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
    /// How many of `portals` no polygon has any more, since it was made afresh.
    garbage: usize,
    /// How many floors over one another its plan has room for.
    storeys: usize,
    /// The cells shut - a door closed, a firewall down - each with the floor and area it has
    /// open, by `row * width + x`, the row counting down every storey in turn.
    closed: HashMap<usize, (f32, u8)>,
    /// The cells opened or shut since it last caught up (see [`NavMesh::sync`]): open, with their
    /// floor and area, or shut.
    pending: HashMap<usize, Option<(f32, u8)>>,
    /// How it was made from its grid, and how the ground is taken, as the grid's was (see
    /// [`grid::Tuning`]); and how those that walk it go about it (see [`walk`]).
    pub made: Tuning,
    pub ground: grid::Tuning,
    pub walk: walk::Tuning,
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

/// What there is in a cell, by storey and plan: its floor and area, if it can be walked.
type Source<'a> = &'a dyn Fn(usize, usize, usize) -> Option<(f32, u8)>;

/// The walkable cells `cell` says there are within `[x0, z0, x1, z1)` of the plan, on each of
/// `storeys`, merged into rectangles each on one storey, its floor level to within [`FLAT`], and
/// all of one area - but those `kept(storey, x, z)` by others already.
fn merge(tuning: &Tuning, cell: Source, storeys: usize, [bx0, bz0, bx1, bz1]: [usize; 4], kept: &dyn Fn(usize, usize, usize) -> bool) -> Vec<Polygon> {
    let (w, d) = (bx1 - bx0, bz1 - bz0);
    let mut polygons = Vec::new();
    let mut taken = vec![0u64; (w * d).div_ceil(64)];
    let is_taken = |taken: &[u64], x: usize, z: usize| {
        let i = (z - bz0) * w + (x - bx0);
        taken[i / 64] & (1 << (i % 64)) != 0
    };
    for storey in 0..storeys {
        taken.iter_mut().for_each(|bits| *bits = 0);
        for z in bz0..bz1 {
            for x in bx0..bx1 {
                if is_taken(&taken, x, z) || kept(storey, x, z) {
                    continue;
                }
                let Some((floor, kind)) = cell(storey, x, z) else {
                    continue;
                };
                let fits = |taken: &[u64], cx: usize, cz: usize| {
                    !is_taken(taken, cx, cz)
                        && !kept(storey, cx, cz)
                        && cell(storey, cx, cz).is_some_and(|(f, k)| (f - floor).abs() <= tuning.flat && k == kind)
                };
                let mut x1 = x + 1;
                while x1 < bx1 && x1 - x < tuning.most_cells && fits(&taken, x1, z) {
                    x1 += 1;
                }
                let mut z1 = z + 1;
                while z1 < bz1 && z1 - z < tuning.most_cells && (x..x1).all(|cx| fits(&taken, cx, z1)) {
                    z1 += 1;
                }
                for cz in z..z1 {
                    for cx in x..x1 {
                        let i = (cz - bz0) * w + (cx - bx0);
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
    polygons
}

impl NavMesh {
    /// The navmesh of `grid`, whose corner is at `origin`: its walkable cells merged into
    /// rectangles of one storey, a level floor and one `area` - as `area` says of each cell, by
    /// `(x, row)` - and joined where a step can be taken from one into the next.
    pub fn build(grid: &WalkGrid, origin: Vector3<f32>, area: impl Fn(usize, usize) -> u8) -> Self {
        Self::build_by(grid, origin, &area, Tuning::DEFAULT)
    }

    /// [`NavMesh::build`], made as `made` says.
    pub fn build_with(grid: &WalkGrid, origin: Vector3<f32>, area: impl Fn(usize, usize) -> u8, made: Tuning) -> Self {
        Self::build_by(grid, origin, &area, made)
    }

    /// [`NavMesh::build`], not generic (see [`NavMesh::route_by`]).
    fn build_by(grid: &WalkGrid, origin: Vector3<f32>, area: &dyn Fn(usize, usize) -> u8, made: Tuning) -> Self {
        let (width, depth) = (grid.width, grid.depth);
        let cell = |storey: usize, x: usize, z: usize| {
            let row = z + storey * depth;
            grid.is_walkable(x, row).then(|| (grid.floor(x, row), area(x, row)))
        };
        let polygons = merge(&made, &cell, grid.storeys, [0, 0, width, depth], &|_, _, _| false);
        let mut mesh = Self {
            origin,
            cell_size: grid.cell_size,
            width,
            depth,
            blocked: vec![false; polygons.len()],
            polygons,
            portals: Vec::new(),
            buckets: vec![Vec::new(); width.div_ceil(BUCKET) * depth.div_ceil(BUCKET)],
            garbage: 0,
            storeys: grid.storeys,
            closed: HashMap::new(),
            pending: HashMap::new(),
            made,
            ground: grid.tuning,
            walk: walk::Tuning::DEFAULT,
        };
        for n in 0..mesh.polygons.len() {
            mesh.index(n as Poly);
        }
        for n in 0..mesh.polygons.len() as Poly {
            mesh.relink(n);
        }
        mesh
    }

    /// The squares of the index polygon `n` is over.
    fn squares(&self, n: Poly) -> impl Iterator<Item = usize> + '_ {
        let p = self.polygons[n as usize];
        let cols = self.width.div_ceil(BUCKET);
        let (xs, zs) = (p.x0 as usize / BUCKET..=(p.x1.max(p.x0 + 1) as usize - 1) / BUCKET, p.z0 as usize / BUCKET..=(p.z1.max(p.z0 + 1) as usize - 1) / BUCKET);
        zs.flat_map(move |bz| xs.clone().map(move |bx| bz * cols + bx))
    }

    /// Puts polygon `n` in the index, under each square it is over.
    fn index(&mut self, n: Poly) {
        let squares: Vec<usize> = self.squares(n).collect();
        for square in squares {
            self.buckets[square].push(n);
        }
    }

    /// Takes polygon `n` out of the index.
    fn unindex(&mut self, n: Poly) {
        let squares: Vec<usize> = self.squares(n).collect();
        for square in squares {
            self.buckets[square].retain(|&m| m != n);
        }
    }

    /// Works polygon `n`'s portals out afresh: along each edge of it, cell by cell, wherever a
    /// step can be taken across into another polygon - over the next cell, its floor a stair's
    /// step up or down at most, the nearest if more than one is - a stretch for each it steps
    /// into. Its old ones, if any, are left as garbage, cleared out once there is enough of it.
    fn relink(&mut self, n: Poly) {
        let portals = self.portals_of(n);
        let p = &mut self.polygons[n as usize];
        self.garbage += p.count as usize;
        p.first = self.portals.len() as u32;
        p.count = portals.len() as u16;
        self.portals.extend(portals);
    }

    fn portals_of(&self, n: Poly) -> Vec<Portal> {
        let cs = self.cell_size;
        let (ox, oz) = (self.origin.x, self.origin.z);
        let mut portals = Vec::new();
        let p = self.polygons[n as usize];
        if p.x0 == p.x1 {
            return portals;
        }
        // Each side: the cells along it inside, and the step out across it.
        let sides: [Side; 4] = [
            ((p.z0..p.z1).map(|z| (p.x1 - 1, z)).collect(), (1, 0)),
            ((p.z0..p.z1).map(|z| (p.x0, z)).collect(), (-1, 0)),
            ((p.x0..p.x1).map(|x| (x, p.z1 - 1)).collect(), (0, 1)),
            ((p.x0..p.x1).map(|x| (x, p.z0)).collect(), (0, -1)),
        ];
        for (cells, (dx, dz)) in sides {
            let mut run: Option<(Poly, [f32; 2], [f32; 2])> = None;
            for (x, z) in cells {
                let to = (x as usize)
                    .checked_add_signed(dx)
                    .zip((z as usize).checked_add_signed(dz))
                    .and_then(|plan| self.step_onto(plan, p.floor, n));
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
                        to.filter(|&t| t != n).map(|t| (t, a, b))
                    }
                };
            }
            if let Some((q, start, end)) = run {
                portals.push(Portal { to: q, a: start, b: end });
            }
        }
        portals
    }

    /// The polygon a step from a floor at `floor` - polygon `from`'s - lands on over cell `plan` of
    /// the plan: of those over it, the one whose floor is nearest, if it is a stair's step up or
    /// down at most.
    fn step_onto(&self, (x, z): (usize, usize), floor: f32, from: Poly) -> Option<Poly> {
        if x >= self.width || z >= self.depth {
            return None;
        }
        self.buckets[(z / BUCKET) * self.width.div_ceil(BUCKET) + x / BUCKET]
            .iter()
            .copied()
            .filter(|&m| m != from)
            .filter(|&m| {
                let q = &self.polygons[m as usize];
                (q.x0..q.x1).contains(&(x as u32)) && (q.z0..q.z1).contains(&(z as u32)) && (q.floor - floor).abs() <= self.ground.max_climb
            })
            .min_by(|&a, &b| (self.polygons[a as usize].floor - floor).abs().total_cmp(&(self.polygons[b as usize].floor - floor).abs()))
    }

    /// The cell `at` is in, as `(x, row)` - the row counting down every storey in turn - on the
    /// floor nearest its height, within a storey's height: none off the floor.
    pub fn cell_at(&self, at: Vector3<f32>) -> Option<(usize, usize)> {
        let n = self.poly_at(at)?;
        let x = ((at.x - self.origin.x) / self.cell_size).floor() as usize;
        let z = ((at.z - self.origin.z) / self.cell_size).floor() as usize;
        Some((x, z + self.polygons[n as usize].storey as usize * self.depth))
    }

    /// Opens `cells` - each `(x, row)`, as [`NavMesh::cell_at`] has them - or shuts them: a door, a
    /// barrier, a firewall. One shut can be opened again as it was; one that never was floor
    /// cannot be. It catches up with them all at once, as [`NavMesh::sync`] is next called.
    pub fn set_walkable(&mut self, cells: &[(usize, usize)], open: bool) {
        for &(x, row) in cells {
            let (z, storey) = (row % self.depth, row / self.depth);
            if x >= self.width || storey >= self.storeys {
                continue;
            }
            let index = row * self.width + x;
            match open {
                true => {
                    if let Some(was) = self.closed.remove(&index) {
                        self.pending.insert(index, Some(was));
                    }
                }
                false => {
                    let here = match self.pending.get(&index) {
                        Some(&pending) => pending,
                        None => self.over((x, z), storey).map(|n| (self.polygons[n as usize].floor, self.polygons[n as usize].area)),
                    };
                    if let Some(was) = here {
                        self.closed.insert(index, was);
                        self.pending.insert(index, None);
                    }
                }
            }
        }
    }

    /// Catches up with the cells opened and shut since it was built, or last caught up (see
    /// [`NavMesh::set_walkable`]): the polygons over or next to them made afresh, as
    /// [`NavMesh::build`] made them, and joined to those round them; the rest left as they are.
    /// Whether anything changed. Polygons made afresh are new numbers; those they replace are
    /// left empty.
    pub fn sync(&mut self) -> bool {
        if self.pending.is_empty() {
            return false;
        }
        let pending = std::mem::take(&mut self.pending);
        self.sync_by(&pending);
        true
    }

    fn sync_by(&mut self, changes: &HashMap<usize, Option<(f32, u8)>>) {
        // The changed cells on the plan, and a cell round, on every storey.
        let plan: Vec<(usize, usize)> = changes.keys().map(|&index| (index % self.width, (index / self.width) % self.depth)).collect();
        let mut bounds = [
            plan.iter().map(|c| c.0).min().unwrap_or(0).saturating_sub(1),
            plan.iter().map(|c| c.1).min().unwrap_or(0).saturating_sub(1),
            (plan.iter().map(|c| c.0).max().unwrap_or(0) + 2).min(self.width),
            (plan.iter().map(|c| c.1).max().unwrap_or(0) + 2).min(self.depth),
        ];
        // The polygons over any of it are made afresh, all of each.
        let overlaps = |p: &Polygon, b: &[usize; 4]| {
            p.x0 != p.x1 && (p.x0 as usize) < b[2] && (p.x1 as usize) > b[0] && (p.z0 as usize) < b[3] && (p.z1 as usize) > b[1]
        };
        let near = bounds;
        let gone: Vec<Poly> = (0..self.polygons.len() as Poly).filter(|&n| overlaps(&self.polygons[n as usize], &near)).collect();
        for &n in &gone {
            let p = self.polygons[n as usize];
            bounds = [bounds[0].min(p.x0 as usize), bounds[1].min(p.z0 as usize), bounds[2].max(p.x1 as usize), bounds[3].max(p.z1 as usize)];
        }
        // Those that stay, over the ground made afresh, keep theirs.
        let staying: Vec<Polygon> = (0..self.polygons.len())
            .filter(|n| !gone.contains(&(*n as Poly)))
            .map(|n| self.polygons[n])
            .filter(|p| overlaps(p, &bounds))
            .collect();
        let kept = |storey: usize, x: usize, z: usize| {
            staying.iter().any(|p| {
                p.storey as usize == storey && (p.x0..p.x1).contains(&(x as u32)) && (p.z0..p.z1).contains(&(z as u32))
            })
        };
        // What is in each cell of it, before anything is made afresh: as the polygons over it have
        // it, but as it has changed.
        let (w, d) = (bounds[2] - bounds[0], bounds[3] - bounds[1]);
        let mut cells: Vec<Option<(f32, u8)>> = vec![None; self.storeys * w * d];
        for storey in 0..self.storeys {
            for z in bounds[1]..bounds[3] {
                for x in bounds[0]..bounds[2] {
                    let index = (z + storey * self.depth) * self.width + x;
                    cells[(storey * d + (z - bounds[1])) * w + (x - bounds[0])] = match changes.get(&index) {
                        Some(&changed) => changed,
                        None => self.over((x, z), storey).map(|n| (self.polygons[n as usize].floor, self.polygons[n as usize].area)),
                    };
                }
            }
        }
        let cell = |storey: usize, x: usize, z: usize| cells[(storey * d + (z - bounds[1])) * w + (x - bounds[0])];
        // Their neighbours, whose portals lead into them.
        let mut neighbours: Vec<Poly> = gone.iter().flat_map(|&n| self.portals(n).iter().map(|p| p.to)).filter(|m| !gone.contains(m)).collect();
        for &n in &gone {
            self.unindex(n);
            let p = &mut self.polygons[n as usize];
            self.garbage += p.count as usize;
            *p = Polygon { x1: p.x0, z1: p.z0, count: 0, ..*p };
        }
        let made = merge(&self.made, &cell, self.storeys, bounds, &kept);
        let first = self.polygons.len() as Poly;
        for polygon in made {
            self.polygons.push(polygon);
            self.blocked.push(false);
            self.index(self.polygons.len() as Poly - 1);
        }
        let fresh: Vec<Poly> = (first..self.polygons.len() as Poly).collect();
        for &n in &fresh {
            self.relink(n);
        }
        // And any that now border the new ones.
        neighbours.extend(fresh.iter().flat_map(|&n| self.portals(n).iter().map(|p| p.to)).filter(|&m| m < first));
        neighbours.sort_unstable();
        neighbours.dedup();
        for n in neighbours {
            self.relink(n);
        }
        if self.garbage > self.portals.len() / 2 {
            self.sweep();
        }
    }

    /// Clears the portals no polygon has any more out of the way.
    fn sweep(&mut self) {
        let mut portals = Vec::with_capacity(self.portals.len() - self.garbage);
        for p in &mut self.polygons {
            let first = portals.len() as u32;
            portals.extend_from_slice(&self.portals[p.first as usize..p.first as usize + p.count as usize]);
            p.first = first;
        }
        self.portals = portals;
        self.garbage = 0;
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
            .filter(|&(_, off)| off <= self.ground.level)
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
            dx.hypot(dz) + (self.floor_of(n) - at.y).abs() / self.ground.level * self.ground.storey_away
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
        // Each also with where its ends were before they were drawn in: a wall's corner, if a turn
        // is made round one.
        let mut gates: Vec<([f32; 2], [f32; 2], Poly)> = vec![(start, start, from)];
        let mut walls: Vec<([f32; 2], [f32; 2])> = vec![(start, start)];
        let mut came = start;
        for &(poly, k) in &corridor {
            let portal = self.portals(poly)[k];
            let length = distance(portal.a, portal.b);
            let inset = clearance.min(length / 2.0) / length.max(1.0e-6);
            let a = [portal.a[0] + (portal.b[0] - portal.a[0]) * inset, portal.a[1] + (portal.b[1] - portal.a[1]) * inset];
            let b = [portal.b[0] + (portal.a[0] - portal.b[0]) * inset, portal.b[1] + (portal.a[1] - portal.b[1]) * inset];
            let middle = portal.middle();
            let ((left, right), wall) =
                if side(came, middle, a) > 0.0 { ((b, a), (portal.b, portal.a)) } else { ((a, b), (portal.a, portal.b)) };
            gates.push((left, right, portal.to));
            walls.push(wall);
            came = middle;
        }
        gates.push((end, end, goal));
        walls.push((end, end));
        // Pulled taut: the funnel. Each corner with the wall's corner it turns round.
        let mut points: Vec<([f32; 2], Poly, [f32; 2])> = Vec::new();
        let (mut apex, mut left, mut right) = (start, start, start);
        let (mut left_at, mut right_at) = (0, 0);
        let mut i = 1;
        while i < gates.len() {
            let (l, r, _) = gates[i];
            if side(apex, right, r) <= 0.0 {
                if apex == right || side(apex, left, r) > 0.0 {
                    (right, right_at) = (r, i);
                } else {
                    points.push((left, gates[left_at].2, walls[left_at].0));
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
                    points.push((right, gates[right_at].2, walls[right_at].1));
                    let at = right_at;
                    apex = right;
                    (left, right, left_at, right_at) = (apex, apex, at, at);
                    i = at + 1;
                    continue;
                }
            }
            i += 1;
        }
        points.retain(|&(at, _, _)| at != start);
        // Each corner set out from the wall's corner it turns round, on the outside of the turn,
        // so that the ways in and out both keep `clearance` from it - drawing the portal's end
        // in keeps them clear of it only along the portal - where there is floor to.
        let corners: Vec<[f32; 2]> = points.iter().map(|&(at, _, _)| at).collect();
        let mut route: Vec<Vector3<f32>> = Vec::with_capacity(points.len() + 1);
        for (k, &(at, poly, wall)) in points.iter().enumerate() {
            let before = if k == 0 { start } else { corners[k - 1] };
            let after = corners.get(k + 1).copied().unwrap_or(end);
            let unit = |v: [f32; 2]| {
                let n = v[0].hypot(v[1]);
                (n > 1.0e-5).then(|| [v[0] / n, v[1] / n])
            };
            let out = unit([at[0] - before[0], at[1] - before[1]])
                .zip(unit([after[0] - at[0], after[1] - at[1]]))
                .and_then(|(i, o)| unit([i[0] - o[0], i[1] - o[1]]));
            let floor = self.floor_of(poly);
            // Only round a corner jutting out into the way: floor on three sides of it. Along a
            // wall that runs straight on, there is nothing to go round.
            let e = self.cell_size * 0.1;
            let floored = [(-e, -e), (e, -e), (-e, e), (e, e)]
                .iter()
                .filter(|&&(dx, dz)| self.poly_at(Vector3::new(wall[0] + dx, floor, wall[1] + dz)).is_some())
                .count();
            let set_out = out
                .filter(|_| floored == 3)
                .map(|o| Vector3::new(wall[0] + o[0] * clearance * std::f32::consts::SQRT_2, floor, wall[1] + o[1] * clearance * std::f32::consts::SQRT_2))
                .filter(|&p| self.poly_at(p).is_some());
            route.push(set_out.unwrap_or(Vector3::new(at[0], floor, at[1])));
        }
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
        self.entries_by(feet, within, &extra).into_iter().map(|(n, (_, cost))| (n, cost)).collect()
    }

    /// As [`NavMesh::costs_from`], each polygon with where the way there comes into it, on the
    /// plan, and the cost to there: for how far it is to a point in it, the cost on from there.
    pub fn entries_from(&self, feet: Vector3<f32>, within: f32) -> HashMap<Poly, ([f32; 2], f32)> {
        self.entries_by(feet, within, &|_| 0.0)
    }

    /// [`NavMesh::entries_from`], weighted, and not generic (see [`NavMesh::route_by`]).
    fn entries_by(&self, feet: Vector3<f32>, within: f32, extra: &dyn Fn(Poly) -> f32) -> HashMap<Poly, ([f32; 2], f32)> {
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
        best
    }

    /// Whether `b` can be seen from `a` along the floor: floor all the way between them, at the
    /// height of the line between them - no wall, no drop, no other storey in the way. Whether
    /// anything stands in the way above the floor is the game's to find out.
    pub fn sees_across(&self, a: Vector3<f32>, b: Vector3<f32>) -> bool {
        let span = (b.x - a.x).hypot(b.z - a.z);
        // A few samples a cell, so that the line cannot slip past the corner of a wall.
        let samples = (span / self.cell_size * 3.0).ceil().max(1.0) as usize;
        (0..=samples).all(|i| {
            let p = a + (b - a) * (i as f32 / samples as f32);
            self.over_spot(p).into_iter().any(|n| (self.floor_of(n) - p.y).abs() <= self.ground.level)
        })
    }

    /// The corners of the ground it covers, on the plan, at the origin's height.
    pub fn bounds(&self) -> (Vector3<f32>, Vector3<f32>) {
        let far = Vector3::new(self.width as f32 * self.cell_size, 0.0, self.depth as f32 * self.cell_size);
        (self.origin, self.origin + far)
    }

    /// How many cells across and down its plan is, and how many floors over one another it has
    /// room for: its cells are `(x, row)`, the row counting down every storey in turn.
    pub fn width(&self) -> usize {
        self.width
    }

    pub fn depth(&self) -> usize {
        self.depth
    }

    pub fn storeys(&self) -> usize {
        self.storeys
    }

    /// Whether cell `(x, row)` is floor that can be walked.
    pub fn is_walkable(&self, x: usize, row: usize) -> bool {
        self.depth > 0 && self.over((x, row % self.depth), row / self.depth).is_some()
    }

    /// How high the floor is in cell `(x, row)` over the origin, if it is floor.
    pub fn floor(&self, x: usize, row: usize) -> f32 {
        self.over((x, row % self.depth), row / self.depth).map_or(0.0, |n| self.polygons[n as usize].floor)
    }

    /// The middle of cell `(x, row)` in the world, at the origin's height.
    pub fn center(&self, (x, row): (usize, usize)) -> Vector3<f32> {
        let z = row % self.depth.max(1);
        self.origin + Vector3::new((x as f32 + 0.5) * self.cell_size, 0.0, (z as f32 + 0.5) * self.cell_size)
    }

    /// The middle of cell `(x, row)`, on its floor.
    pub fn on_floor(&self, (x, row): (usize, usize)) -> Vector3<f32> {
        self.center((x, row)) + Vector3::new(0.0, self.floor(x, row), 0.0)
    }

    /// The walkable cells over `(x, z)` of the plan, the ground's first.
    pub fn cells_over(&self, (x, z): (usize, usize)) -> impl Iterator<Item = (usize, usize)> + '_ {
        (0..self.storeys).map(move |storey| (x, z + storey * self.depth)).filter(|&(x, row)| self.is_walkable(x, row))
    }

    /// Every walkable cell.
    pub fn walkable_cells(&self) -> impl Iterator<Item = (usize, usize)> + '_ {
        self.polygons.iter().filter(|p| p.x0 != p.x1).flat_map(move |p| {
            (p.z0..p.z1).flat_map(move |z| (p.x0..p.x1).map(move |x| (x as usize, z as usize + p.storey as usize * self.depth)))
        })
    }

    /// The walkable cell nearest `point`, a storey's height counting as far as
    /// [`crate::navigation::grid::STOREY_AWAY`] cells across.
    pub fn nearest_walkable(&self, point: Vector3<f32>) -> Option<(usize, usize)> {
        let n = self.nearest(point)?;
        self.cell_at(self.closest_point(n, point))
    }

    /// The walkable cell `point` is in, or failing that the nearest one.
    pub fn walkable_cell(&self, point: Vector3<f32>) -> Option<(usize, usize)> {
        self.cell_at(point).or_else(|| self.nearest_walkable(point))
    }

    /// The cells a step along the plan's four ways from cell `from` lands in: onto floor a stair's
    /// step up or down at most, the nearest if more than one is.
    pub fn steps(&self, (x, row): (usize, usize)) -> impl Iterator<Item = (usize, usize)> + '_ {
        let (z, storey) = (row % self.depth.max(1), row / self.depth.max(1));
        let here = self.over((x, z), storey);
        [(1isize, 0isize), (-1, 0), (0, 1), (0, -1)].into_iter().filter_map(move |(dx, dz)| {
            let n = here?;
            let plan = (x.checked_add_signed(dx)?, z.checked_add_signed(dz)?);
            let to = match self.over(plan, storey) {
                Some(same) if same == n || (self.polygons[same as usize].floor - self.polygons[n as usize].floor).abs() <= self.ground.max_climb => same,
                _ => self.step_onto(plan, self.polygons[n as usize].floor, n)?,
            };
            Some((plan.0, plan.1 + self.polygons[to as usize].storey as usize * self.depth))
        })
    }

    /// How many steps (see [`NavMesh::steps`]) each cell is from `start`, by `row * width + x`:
    /// none for those it cannot get to. A cell at a time - for a level's setting out, on a map
    /// no bigger than a heist's, not a city's.
    pub fn distances_from(&self, start: (usize, usize)) -> Vec<Option<u32>> {
        let mut distances = vec![None; self.width * self.depth * self.storeys];
        if !self.is_walkable(start.0, start.1) {
            return distances;
        }
        let mut queue = std::collections::VecDeque::from([start]);
        distances[start.1 * self.width + start.0] = Some(0);
        while let Some(cell) = queue.pop_front() {
            let d = distances[cell.1 * self.width + cell.0].unwrap_or(0);
            for next in self.steps(cell) {
                let i = next.1 * self.width + next.0;
                if distances[i].is_none() {
                    distances[i] = Some(d + 1);
                    queue.push_back(next);
                }
            }
        }
        distances
    }

    /// The cell furthest from `start` in steps, and how many.
    pub fn farthest_from(&self, start: (usize, usize)) -> Option<((usize, usize), u32)> {
        self.distances_from(start)
            .iter()
            .enumerate()
            .filter_map(|(i, d)| d.map(|d| ((i % self.width, i / self.width), d)))
            .max_by_key(|&(_, d)| d)
    }

    /// How much floor it has, in square meters.
    pub fn floor_area(&self) -> f32 {
        (0..self.polygons.len() as Poly).map(|n| self.area_of(n)).sum()
    }

    /// How many polygons there are that have not been made afresh as others.
    pub fn live(&self) -> usize {
        self.polygons.iter().filter(|p| p.x0 != p.x1).count()
    }

    /// The polygons over `at` on the plan, on every storey.
    pub fn over_spot(&self, at: Vector3<f32>) -> Vec<Poly> {
        let x = ((at.x - self.origin.x) / self.cell_size).floor();
        let z = ((at.z - self.origin.z) / self.cell_size).floor();
        if !(x >= 0.0 && z >= 0.0 && (x as usize) < self.width && (z as usize) < self.depth) {
            return Vec::new();
        }
        let (x, z) = (x as u32, z as u32);
        self.buckets[(z as usize / BUCKET) * self.width.div_ceil(BUCKET) + x as usize / BUCKET]
            .iter()
            .copied()
            .filter(|&n| {
                let p = &self.polygons[n as usize];
                (p.x0..p.x1).contains(&x) && (p.z0..p.z1).contains(&z)
            })
            .collect()
    }

    /// The point of polygon `n` nearest `at`, on its floor.
    pub fn closest_point(&self, n: Poly, at: Vector3<f32>) -> Vector3<f32> {
        let p = &self.polygons[n as usize];
        let cs = self.cell_size;
        // Kept a little in from its edges, to be inside it.
        let inset = cs * 0.25;
        let clamp = |v: f32, from: u32, to: u32, o: f32| v.clamp(o + from as f32 * cs + inset, o + to as f32 * cs - inset);
        Vector3::new(clamp(at.x, p.x0, p.x1, self.origin.x), self.floor_of(n), clamp(at.z, p.z0, p.z1, self.origin.z))
    }

    /// Somewhere in polygon `n`, on its floor, picked by `pick`, which gives a number under what
    /// it is given.
    pub fn random_point(&self, n: Poly, pick: &mut dyn FnMut(usize) -> usize) -> Vector3<f32> {
        let p = &self.polygons[n as usize];
        let cs = self.cell_size;
        let x = p.x0 as usize + pick((p.x1 - p.x0).max(1) as usize);
        let z = p.z0 as usize + pick((p.z1 - p.z0).max(1) as usize);
        self.origin + Vector3::new((x as f32 + 0.5) * cs, p.floor, (z as f32 + 0.5) * cs)
    }

    /// The floor nearest `point` within `near` meters of it on the plan, at its height - no more
    /// than `level` above or below it: where it is, if there is floor there, or else the nearest
    /// point of the nearest such polygon. None with no floor at its height near it.
    pub fn floor_near(&self, point: Vector3<f32>, near: f32, level: f32) -> Option<Vector3<f32>> {
        let at_height = |n: Poly| (self.floor_of(n) - point.y).abs() <= level;
        if let Some(n) = self.over_spot(point).into_iter().filter(|&n| at_height(n)).min_by(|&a, &b| {
            (self.floor_of(a) - point.y).abs().total_cmp(&(self.floor_of(b) - point.y).abs())
        }) {
            return Some(Vector3::new(point.x, self.floor_of(n), point.z));
        }
        let cols = self.width.div_ceil(BUCKET) as isize;
        let rows = self.depth.div_ceil(BUCKET) as isize;
        let reach = (near / self.cell_size / BUCKET as f32).ceil() as isize;
        let bx = ((point.x - self.origin.x) / self.cell_size) as isize / BUCKET as isize;
        let bz = ((point.z - self.origin.z) / self.cell_size) as isize / BUCKET as isize;
        let mut best: Option<(Vector3<f32>, f32)> = None;
        for cz in (bz - reach).max(0)..=(bz + reach).min(rows - 1) {
            for cx in (bx - reach).max(0)..=(bx + reach).min(cols - 1) {
                for &n in &self.buckets[(cz * cols + cx) as usize] {
                    if !at_height(n) {
                        continue;
                    }
                    let spot = self.closest_point(n, point);
                    let d = (spot.x - point.x).hypot(spot.z - point.z);
                    if d <= near && best.is_none_or(|(_, b)| d < b) {
                        best = Some((spot, d));
                    }
                }
            }
        }
        best.map(|(spot, _)| spot)
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
    fn an_open_floor_is_a_few_polygons_and_the_way_across_it_is_straight() {
        let grid = grid(40, 30, |_, _| true);
        let mesh = NavMesh::build(&grid, Vector3::zeros(), |_, _| 0);
        assert_eq!(mesh.len(), 3 * 2, "16 cells along a side at most");
        let to = Vector3::new(18.0, 0.0, 13.0);
        assert_eq!(mesh.route(Vector3::new(1.0, 0.0, 1.0), to, f32::INFINITY, 0.3, |_| 0.0), vec![to]);
    }

    #[test]
    fn round_a_corner_it_cuts_it_as_close_as_it_can_and_no_closer() {
        // An L: along the bottom, then up the right-hand side; a wall in the corner of the L.
        let grid = grid(40, 40, |x, z| z < 8 || x >= 32);
        let mesh = NavMesh::build(&grid, Vector3::zeros(), |_, _| 0);
        assert_eq!(mesh.len(), 3 + 2, "the bottom of the L in three, its side in two");
        let from = Vector3::new(1.0, 0.0, 2.0);
        let to = Vector3::new(18.0, 0.0, 19.0);
        let route = mesh.route(from, to, f32::INFINITY, 0.3, |_| 0.0);
        assert_eq!(route.len(), 2, "one turn, at the corner: {route:?}");
        // Its ways in and out keep the clearance from the wall's corner, and no more than a
        // little over it.
        let wall = Vector3::new(16.0, 0.0, 4.0);
        let gap = |a: Vector3<f32>, b: Vector3<f32>| {
            let t = ((wall - a).dot(&(b - a)) / (b - a).norm_squared()).clamp(0.0, 1.0);
            (a + (b - a) * t - wall).norm()
        };
        let closest = gap(from, route[1]).min(gap(route[1], route[0]));
        assert!((0.25..0.5).contains(&closest), "{closest} from the corner: {route:?}");
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
        assert_eq!(mesh.len(), 1 + 8 + 2, "the bottom, each step, and the landing in two");
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
        assert_eq!(mesh.len(), 4 + 2, "the ground in four, the floor over it in two");
        let ground = mesh.poly_at(Vector3::new(2.0, 0.1, 2.0)).unwrap();
        let upstairs = mesh.poly_at(Vector3::new(2.0, 3.9, 2.0)).unwrap();
        assert_ne!(ground, upstairs);
        assert_eq!(mesh.floor_of(upstairs), 4.0);
        let crosses = (0..mesh.len() as Poly)
            .flat_map(|n| mesh.portals(n).iter().map(move |p| (n, p.to)))
            .any(|(a, b)| mesh.polygon(a).storey != mesh.polygon(b).storey);
        assert!(!crosses, "no stair between them");
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

    #[test]
    fn a_door_opened_and_shut_is_caught_up_with_where_it_is_and_nowhere_else() {
        // Two rooms with a wall between, and a doorway in it, made open and shut at once; a floor
        // of its own beyond.
        let door = [(20, 9), (20, 10)];
        let open = grid(80, 20, |x, z| x != 20 || door.contains(&(x, z)));
        let shut = grid(80, 20, |x, _| x != 20);
        let area = |x: usize, _: usize| u8::from(x >= 40);
        let mesh_far = |mesh: &NavMesh| mesh.over_spot(Vector3::new(35.0, 0.0, 5.0))[0];
        let mut mesh = NavMesh::build(&open, Vector3::zeros(), area);
        let far = mesh_far(&mesh);
        mesh.set_walkable(&door, false);
        assert!(mesh.sync());
        let from = Vector3::new(2.0, 0.0, 1.0);
        let to = Vector3::new(18.0, 0.0, 1.0);
        assert!(mesh.route(from, to, f32::INFINITY, 0.25, |_| 0.0).is_empty(), "shut");
        // Opened again.
        mesh.set_walkable(&door, true);
        assert!(mesh.sync());
        let opened = mesh.route(from, to, f32::INFINITY, 0.25, |_| 0.0);
        assert!(opened.iter().any(|p| (p.x - 10.0).abs() < 0.6 && (4.4..5.6).contains(&p.z)), "through the doorway: {opened:?}");
        assert_eq!(mesh_far(&mesh), far, "the far floor left as it was");
        // As a navmesh made from scratch would have it.
        let fresh = NavMesh::build(&open, Vector3::zeros(), area);
        assert_eq!(fresh.route(from, to, f32::INFINITY, 0.25, |_| 0.0), opened);
        // Shut again.
        mesh.set_walkable(&door, false);
        assert!(mesh.sync());
        assert!(mesh.route(from, to, f32::INFINITY, 0.25, |_| 0.0).is_empty(), "shut again");
        assert!(!mesh.sync(), "nothing more to catch up with");
        // A cell that never was floor cannot be opened.
        mesh.set_walkable(&[(20, 3)], true);
        assert!(!mesh.sync());
        // Over and over, it stays as small as it should.
        for k in 0..50 {
            mesh.set_walkable(&door[..1], k % 2 == 0);
            mesh.sync();
        }
        mesh.set_walkable(&door[..1], false);
        mesh.sync();
        assert!(mesh.portal_count() < 4 * fresh.portal_count() + 64, "{} portals", mesh.portal_count());
        assert_eq!(mesh.live(), NavMesh::build(&shut, Vector3::zeros(), area).len());
        assert_eq!(mesh.cell_at(Vector3::new(10.25, 0.0, 5.25)), None, "shut, no floor");
        assert_eq!(mesh.cell_at(Vector3::new(1.25, 0.0, 5.25)), Some((2, 10)));
    }

    #[test]
    fn the_floor_near_a_point_is_found_at_its_height() {
        let mut grid = WalkGrid::with_storeys(40, 40, 2, 0.5);
        for z in 0..40 {
            for x in 0..40 {
                grid.set(x, z, x < 10);
                grid.set(x, z + 40, x >= 30);
                grid.set_floor(x, z + 40, 4.0);
            }
        }
        let mesh = NavMesh::build(&grid, Vector3::zeros(), |_, _| 0);
        let ground = mesh.floor_near(Vector3::new(7.0, 0.0, 5.0), 6.0, 1.0).unwrap();
        assert!((ground.x - 4.875).abs() < 0.01 && ground.y == 0.0, "{ground:?}");
        let up = mesh.floor_near(Vector3::new(13.0, 4.2, 5.0), 6.0, 1.0).unwrap();
        assert!(up.x >= 15.0 && up.y == 4.0, "{up:?}");
        assert_eq!(mesh.floor_near(Vector3::new(10.0, 2.0, 5.0), 6.0, 1.0), None, "nothing at that height");
    }

    #[test]
    fn its_cells_are_the_grids_and_steps_between_them_as_the_grids() {
        // A room with a pillar, and stairs up to a landing.
        let mut grid = WalkGrid::new(20, 10, 0.5);
        for z in 0..10 {
            for x in 0..20 {
                grid.set(x, z, !(5..8).contains(&x) || !(3..6).contains(&z));
                if x >= 12 {
                    grid.set_floor(x, z, ((x - 11) as f32 * 0.2).min(1.2));
                }
            }
        }
        let mesh = NavMesh::build(&grid, Vector3::zeros(), |_, _| 0);
        for z in 0..10 {
            for x in 0..20 {
                assert_eq!(mesh.is_walkable(x, z), grid.is_walkable(x, z), "({x}, {z})");
                if grid.is_walkable(x, z) {
                    assert!((mesh.floor(x, z) - grid.floor(x, z)).abs() <= FLAT);
                }
            }
        }
        assert_eq!(mesh.walkable_cells().count(), grid.walkable_cells().count());
        assert_eq!(mesh.distances_from((0, 0)), grid.distances_from((0, 0)));
        assert_eq!(mesh.farthest_from((0, 0)), grid.farthest_from((0, 0)));
        // In the pillar, the nearest floor beside it.
        assert!(!mesh.is_walkable(6, 4));
        let (x, z) = mesh.walkable_cell(Vector3::new(3.25, 0.0, 2.25)).unwrap();
        assert!(mesh.is_walkable(x, z) && (x as f32 - 6.0).abs() + (z as f32 - 4.0).abs() <= 3.0);
        assert!((mesh.floor_area() - grid.walkable_cells().count() as f32 * 0.25).abs() < 1e-3);
    }
}

