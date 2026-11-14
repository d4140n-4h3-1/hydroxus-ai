//! Navigation validated (ROADMAP 6.1) on generated worlds, through the public API: a large one, floors
//! over floors joined by stairs, doors shutting and opening under the routes, narrow passages, a crowd
//! through one doorway, and the same queries asked again and again. The crowd's bodies slide along
//! walls they walk into, as a game's colliders do.
//!
//! Each route is checked against the grid the navmesh was made from - its own steps from cell to
//! cell (`NavMesh::steps`), which climb stairs and go from floor to floor as the droids' feet do:
//! - on the floor: every point of it, and the way between each and the next, over a polygon;
//! - found exactly when there is a way: a route where the grid has a way, none where it has not;
//! - not far round: no more than a little longer than the grid's own shortest way, eight ways from
//!   each cell (corners not cut), which is itself a little longer than the shortest there is.

use hydroxus_ai::{
    flat,
    navigation::{
        grid::{Rng, WalkGrid},
        navmesh::NavMesh,
        steer,
        walk::route_to,
    },
};
use nalgebra::Vector3;
use std::collections::{BinaryHeap, VecDeque};

type V = Vector3<f32>;
const CELL: f32 = 0.5;

/// How long `route`, from `from`, is along the ground (the route's last point first, as given).
fn length(from: V, route: &[V]) -> f32 {
    let mut at = from;
    let mut total = 0.0;
    for &next in route.iter().rev() {
        total += flat(next - at).norm();
        at = next;
    }
    total
}

/// Whether every point of `route` from `from`, and the way between each and the next, is over a
/// polygon at about its height.
fn on_floor(mesh: &NavMesh, from: V, route: &[V]) -> bool {
    off_floor(mesh, from, route).is_none()
}

/// The first stretch of `route` from `from` that leaves the floor, and where: (from, to, the spot).
fn off_floor(mesh: &NavMesh, from: V, route: &[V]) -> Option<(V, V, V)> {
    let mut at = from;
    for &next in route.iter().rev() {
        let steps = ((next - at).norm() / (CELL * 0.25)).ceil().max(1.0) as usize;
        for k in 0..=steps {
            let p = at + (next - at) * (k as f32 / steps as f32);
            // Within a centimetre of floor is on it: a way along a wall's face touches it, and the
            // cell a point exactly on the line between two cells is taken to be in is the wall's.
            let near = [(0.0, 0.0), (0.01, 0.0), (-0.01, 0.0), (0.0, 0.01), (0.0, -0.01)];
            if !near.iter().any(|&(dx, dz)| mesh.poly_at(p + V::new(dx, 0.0, dz)).is_some()) {
                return Some((at, next, p));
            }
        }
        at = next;
    }
    None
}

/// Which part of the grid each walkable cell is in, by the grid's own steps: cells in the same part
/// have a way between them.
fn parts(mesh: &NavMesh) -> Vec<Option<u32>> {
    let (w, rows) = (mesh.width(), mesh.depth() * mesh.storeys());
    let mut label = vec![None; w * rows];
    let mut next = 0;
    for start in mesh.walkable_cells() {
        if label[start.1 * w + start.0].is_some() {
            continue;
        }
        let mut queue = VecDeque::from([start]);
        label[start.1 * w + start.0] = Some(next);
        while let Some(at) = queue.pop_front() {
            for to in mesh.steps(at) {
                if label[to.1 * w + to.0].is_none() {
                    label[to.1 * w + to.0] = Some(next);
                    queue.push_back(to);
                }
            }
        }
        next += 1;
    }
    label
}

/// The grid's shortest way from `start` to every cell, in meters, eight ways from each cell, a
/// diagonal only where both cells it passes between are walkable on the same storey.
fn shortest_from(mesh: &NavMesh, start: (usize, usize)) -> Vec<f32> {
    let w = mesh.width();
    let rows = mesh.depth() * mesh.storeys();
    let mut best = vec![f32::INFINITY; w * rows];
    #[derive(PartialEq)]
    struct Q(f32, usize, usize);
    impl Eq for Q {}
    impl PartialOrd for Q {
        fn partial_cmp(&self, o: &Self) -> Option<std::cmp::Ordering> {
            Some(self.cmp(o))
        }
    }
    impl Ord for Q {
        fn cmp(&self, o: &Self) -> std::cmp::Ordering {
            o.0.total_cmp(&self.0)
        }
    }
    let mut heap = BinaryHeap::from([Q(0.0, start.0, start.1)]);
    best[start.1 * w + start.0] = 0.0;
    while let Some(Q(d, x, row)) = heap.pop() {
        if d > best[row * w + x] {
            continue;
        }
        let here = (x, row);
        let mut go = |to: (usize, usize), cost: f32, heap: &mut BinaryHeap<Q>| {
            let nd = d + cost;
            if nd < best[to.1 * w + to.0] {
                best[to.1 * w + to.0] = nd;
                heap.push(Q(nd, to.0, to.1));
            }
        };
        let steps: Vec<(usize, usize)> = mesh.steps(here).collect();
        for &to in &steps {
            let rise = mesh.floor(to.0, to.1) - mesh.floor(x, row);
            go(to, CELL.hypot(rise), &mut heap);
        }
        // Diagonals, on one storey, between two steps that are both open.
        for &a in &steps {
            for &b in &steps {
                if a.0 == x || b.1 == row || a.1 != row || b.0 != x {
                    continue;
                }
                let to = (a.0, b.1);
                if to.1 / mesh.depth() == row / mesh.depth()
                    && mesh.is_walkable(to.0, to.1)
                    && mesh.steps(a).any(|s| s == to)
                    && mesh.steps(b).any(|s| s == to)
                {
                    go(to, CELL * std::f32::consts::SQRT_2, &mut heap);
                }
            }
        }
    }
    best
}

/// A world of rooms `room` cells square, `rooms` along each side: walls a cell thick, each wall with
/// a doorway or not, 1 to 6 cells wide, at random; some rooms with pillars; and walled-off rooms
/// with no way in.
fn rooms(seed: u64, rooms: usize, room: usize) -> WalkGrid {
    let mut rng = Rng::new(seed);
    let side = rooms * room;
    let mut grid = WalkGrid::new(side, side, CELL);
    for z in 0..side {
        for x in 0..side {
            grid.set(x, z, x % room != room - 1 && z % room != room - 1);
        }
    }
    for ri in 0..rooms {
        for rj in 0..rooms {
            let (x0, z0) = (ri * room, rj * room);
            // A doorway in its east and north walls, most of the time; a narrow one now and then.
            for east in [true, false] {
                if rng.below(10) < 2 {
                    continue;
                }
                let wide = if rng.below(5) == 0 { 1 } else { 2 + rng.below(5) };
                let at = 1 + rng.below(room - 2 - wide);
                for k in at..at + wide {
                    let (x, z) = if east { (x0 + room - 1, z0 + k) } else { (x0 + k, z0 + room - 1) };
                    if x < side - 1 && z < side - 1 {
                        grid.set(x, z, true);
                    }
                }
            }
            // Pillars.
            for _ in 0..rng.below(4) {
                let (px, pz) = (x0 + 2 + rng.below(room - 5), z0 + 2 + rng.below(room - 5));
                for (dx, dz) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    grid.set(px + dx, pz + dz, false);
                }
            }
        }
    }
    grid
}

/// A walkable cell to route from or to, picked by `rng`.
fn somewhere(rng: &mut Rng, cells: &[(usize, usize)]) -> (usize, usize) {
    cells[rng.below(cells.len())]
}

#[test]
fn in_a_large_world_a_route_is_on_the_floor_found_exactly_when_there_is_a_way_and_near_the_shortest() {
    // 300 m square: 20 by 20 rooms of 15 m.
    let grid = rooms(11, 20, 30);
    let mesh = NavMesh::build(&grid, V::zeros(), |_, _| 0);
    let part = parts(&mesh);
    let w = mesh.width();
    let cells: Vec<(usize, usize)> = mesh.walkable_cells().collect();
    let mut rng = Rng::new(5);
    let (mut found, mut none, mut worst) = (0, 0, 1.0f32);
    let mut ratios: Vec<(f32, f32)> = Vec::new();
    for _ in 0..12 {
        let start = somewhere(&mut rng, &cells);
        let shortest = shortest_from(&mesh, start);
        let from = mesh.on_floor(start);
        for _ in 0..60 {
            let goal = somewhere(&mut rng, &cells);
            let to = mesh.on_floor(goal);
            let route = route_to(&mesh, from, to, f32::INFINITY);
            let joined = part[start.1 * w + start.0] == part[goal.1 * w + goal.0];
            if flat(to - from).norm() < CELL {
                continue;
            }
            assert_eq!(!route.is_empty(), joined, "a route from {start:?} to {goal:?} exactly when there is a way");
            if route.is_empty() {
                none += 1;
                continue;
            }
            found += 1;
            if let Some((a, b, p)) = off_floor(&mesh, from, &route) {
                let cell = |v: V| ((v.x / CELL) as usize, (v.z / CELL) as usize);
                let around: Vec<String> = (0..7).map(|dz| (0..7).map(|dx| {
                    let (cx, cz) = (cell(p).0 + dx - 3, cell(p).1 + dz - 3);
                    if (cx, cz) == cell(p) { 'X' } else if mesh.is_walkable(cx, cz) { '.' } else { '#' }
                }).collect()).collect();
                panic!("off the floor from {start:?} to {goal:?}: between {a:?} and {b:?}, at {p:?}, cell {:?}\n{}", cell(p), around.join("\n"));
            }
            assert!((route[0] - to).norm() < 0.01, "it ends where it was asked to");
            let grid_way = shortest[goal.1 * w + goal.0];
            let ratio = length(from, &route) / grid_way.max(CELL);
            worst = worst.max(ratio);
            ratios.push((ratio, grid_way));
        }
    }
    ratios.sort_by(|a, b| a.0.total_cmp(&b.0));
    let at = |q: f64| ratios[((ratios.len() - 1) as f64 * q) as usize].0;
    let over = |r: f32| ratios.iter().filter(|x| x.0 > r).count();
    println!(
        "large world: {} polygons; {found} routes, {none} with no way; as long as the grid's own shortest way: median {:.3}, 95th {:.3}, 99th {:.3}, worst {worst:.3}; over 1.10: {}, over 1.20: {}",
        mesh.len(), at(0.5), at(0.95), at(0.99), over(1.10), over(1.20)
    );
    for (r, d) in ratios.iter().rev().take(5) {
        println!("  {r:.3} on a way of {d:.1} m");
    }
    // Measured (seed 11, 20 by 20 rooms): median 0.997, 95th 1.041, worst 1.304 - one short way of
    // 18 m by another doorway. The corridor is found over polygons, not cells, so a little over
    // now and then is to be expected.
    assert!(at(0.5) < 1.02 && at(0.95) < 1.08 && worst < 1.4, "longer than measured");
    assert!(found > 500 && none > 0, "both kinds tried: {found} found, {none} not");
}

#[test]
fn floors_over_floors_are_gone_between_by_their_stairs_and_not_without_them() {
    // Three floors, 3 m apart, 40 m square, each over the last; a stair of 15 steps of 0.2 m up from
    // each to the next, at alternate ends.
    let (w, d, floors) = (80, 80, 3);
    let mut grid = WalkGrid::with_storeys(w, d, floors, CELL);
    for s in 0..floors {
        for z in 0..d {
            for x in 0..w {
                let (row, cell) = (z + s * d, (x, z));
                // Each floor over the ground has a stairwell cut out, where the stair below comes up.
                let well = s > 0 && (if s % 2 == 1 { (60..76).contains(&x) } else { (4..20).contains(&x) }) && (2..6).contains(&z);
                grid.set(cell.0, row, !well);
                grid.set_floor(cell.0, row, 3.0 * s as f32);
            }
        }
    }
    // The stairs: on the floor below, rising one step a cell, its top a step below the floor above.
    let stair = |s: usize| -> Vec<usize> { if s % 2 == 0 { (60..75).collect() } else { (5..20).rev().collect() } };
    for s in 0..floors - 1 {
        for (k, x) in stair(s).into_iter().enumerate() {
            for z in 2..6 {
                grid.set_floor(x, z + s * d, 3.0 * s as f32 + 0.2 * (k + 1) as f32);
            }
        }
    }
    let mut mesh = NavMesh::build(&grid, V::zeros(), |_, _| 0);
    let ground = V::new(20.0, 0.0, 20.0);
    let top = V::new(20.0, 6.0, 20.0);
    let route = route_to(&mesh, ground, top, f32::INFINITY);
    assert!(!route.is_empty(), "up two floors");
    assert!((route[0].y - 6.0).abs() < 0.01, "ending on the top floor: {:?}", route[0]);
    assert!(on_floor(&mesh, ground, &route));
    // It goes by both stairs: past each one's foot.
    assert!(route.iter().any(|p| p.x > 28.0 && p.y < 3.0 + 0.5), "by the first stair, at the east end");
    assert!(route.iter().any(|p| p.x < 12.0 && p.y > 2.5), "by the second, at the west end");
    let climbed = length(ground, &route);
    assert!(climbed > 40.0 && climbed < 90.0, "{climbed} m");
    // Down again is as far.
    let down = route_to(&mesh, top, ground, f32::INFINITY);
    assert!((length(top, &down) - climbed).abs() < 1.0, "{} against {climbed}", length(top, &down));
    // The second stair shut: no way to the top floor, the first floor still reached.
    let second: Vec<(usize, usize)> = stair(1).into_iter().flat_map(|x| (2..6).map(move |z| (x, z + d))).collect();
    mesh.set_walkable(&second, false);
    assert!(mesh.sync());
    assert!(route_to(&mesh, ground, top, f32::INFINITY).is_empty(), "the top floor out of reach");
    assert!(!route_to(&mesh, ground, V::new(20.0, 3.0, 20.0), f32::INFINITY).is_empty(), "the first floor still in reach");
}

#[test]
fn doors_shut_and_opened_under_the_routes_are_caught_up_with_as_a_fresh_navmesh_would_have_them() {
    let mut grid = rooms(3, 6, 30);
    let mut mesh = NavMesh::build(&grid, V::zeros(), |_, _| 0);
    // Every doorway's cells: wall cells left walkable.
    let side = grid.width;
    let doors: Vec<Vec<(usize, usize)>> = {
        let mut doors = Vec::new();
        for line in (1..6).map(|k| k * 30 - 1) {
            for east in [true, false] {
                let mut run = Vec::new();
                for k in 0..side {
                    let (x, z) = if east { (line, k) } else { (k, line) };
                    if grid.is_walkable(x, z) && k % 30 != 29 {
                        run.push((x, z));
                    } else if !run.is_empty() {
                        doors.push(std::mem::take(&mut run));
                    }
                }
                if !run.is_empty() {
                    doors.push(run);
                }
            }
        }
        doors
    };
    assert!(doors.len() > 20, "{} doorways", doors.len());
    let mut rng = Rng::new(9);
    let cells: Vec<(usize, usize)> = mesh.walkable_cells().collect();
    let mut shut = vec![false; doors.len()];
    let mut against_fresh: Vec<f32> = Vec::new();
    for round in 0..120 {
        // A few doors change; the navmesh catches up; routes are as a navmesh made afresh has them.
        for _ in 0..3 {
            let k = rng.below(doors.len());
            shut[k] = !shut[k];
            mesh.set_walkable(&doors[k], !shut[k]);
            for &(x, z) in &doors[k] {
                grid.set(x, z, !shut[k]);
            }
        }
        mesh.sync();
        if round % 10 != 0 {
            continue;
        }
        let fresh = NavMesh::build(&grid, V::zeros(), |_, _| 0);
        let part = parts(&fresh);
        for _ in 0..20 {
            let (a, b) = (cells[rng.below(cells.len())], cells[rng.below(cells.len())]);
            if !mesh.is_walkable(a.0, a.1) || !mesh.is_walkable(b.0, b.1) {
                continue;
            }
            let (from, to) = (mesh.on_floor(a), mesh.on_floor(b));
            let route = route_to(&mesh, from, to, f32::INFINITY);
            let joined = part[a.1 * side + a.0] == part[b.1 * side + b.0];
            assert_eq!(!route.is_empty(), joined || flat(to - from).norm() < CELL, "round {round}: a way exactly when there is one");
            assert!(on_floor(&mesh, from, &route), "round {round}: on the floor");
            let afresh = route_to(&fresh, from, to, f32::INFINITY);
            assert_eq!(route.is_empty(), afresh.is_empty(), "round {round}: a way as a fresh navmesh has one");
            if !route.is_empty() && length(from, &afresh) > 1.0 {
                against_fresh.push(length(from, &route) / length(from, &afresh));
            }
        }
    }
    // Caught up with by the cells changed, the floor is cut into polygons another way than a navmesh
    // made afresh cuts it, and the way over them can differ a little: by how much.
    against_fresh.sort_by(f32::total_cmp);
    let (lo, hi) = (against_fresh[0], against_fresh[against_fresh.len() - 1]);
    let mid = against_fresh[against_fresh.len() / 2];
    println!("doors: {} routes over a navmesh caught up with, against a fresh one: {lo:.3} to {hi:.3}, median {mid:.3}", against_fresh.len());
    assert!(lo > 0.9 && hi < 1.1 && (mid - 1.0).abs() < 0.01, "{lo} to {hi}, median {mid}");
    // A route taken, its door shut half way along it: from where the walker stands, another way or none.
    let (from, to) = (V::new(5.0, 0.0, 5.0), V::new(80.0, 0.0, 80.0));
    for k in 0..doors.len() {
        mesh.set_walkable(&doors[k], true);
    }
    mesh.sync();
    let before = route_to(&mesh, from, to, f32::INFINITY);
    assert!(!before.is_empty());
    let middle = before[before.len() / 2];
    let through = doors.iter().position(|door| {
        before.iter().any(|p| door.iter().any(|&(x, z)| (flat(*p - mesh.on_floor((x, z))).norm()) < 1.5))
    });
    let through = through.expect("it goes by a doorway");
    mesh.set_walkable(&doors[through], false);
    mesh.sync();
    let after = route_to(&mesh, middle, to, f32::INFINITY);
    assert!(on_floor(&mesh, middle, &after));
    assert!(!after.iter().any(|p| doors[through].iter().any(|&(x, z)| flat(*p - mesh.on_floor((x, z))).norm() < 0.3)), "not through the shut door");
}

#[test]
fn a_passage_one_cell_wide_is_found_and_gone_through_on_the_floor() {
    // Two rooms joined by a slit half a meter wide and 10 m long, with a turn in it.
    let mut grid = WalkGrid::new(100, 60, CELL);
    for z in 0..60 {
        for x in 0..100 {
            let room = (x < 30 || x >= 70) && (5..55).contains(&z);
            let slit = ((30..50).contains(&x) && z == 20) || (x == 50 && (20..41).contains(&z)) || ((50..70).contains(&x) && z == 40);
            grid.set(x, z, room || slit);
        }
    }
    let mesh = NavMesh::build(&grid, V::zeros(), |_, _| 0);
    let (from, to) = (V::new(5.0, 0.0, 15.0), V::new(45.0, 0.0, 15.0));
    let route = route_to(&mesh, from, to, f32::INFINITY);
    assert!(!route.is_empty(), "through the slit");
    assert!(on_floor(&mesh, from, &route), "{route:?}");
    let through = length(from, &route);
    assert!(through > 40.0 + 10.0 && through < 40.0 + 25.0, "round by the slit: {through} m");
}

#[test]
fn a_crowd_through_one_doorway_all_get_through_and_never_off_the_floor() {
    // Two halls joined by a doorway 2 m wide; 40 walkers from one to the far side of the other.
    let mut grid = WalkGrid::new(120, 60, CELL);
    for z in 0..60 {
        for x in 0..120 {
            grid.set(x, z, x != 60 || (28..32).contains(&z));
        }
    }
    let mesh = NavMesh::build(&grid, V::zeros(), |_, _| 0);
    let tuning = steer::Tuning::DEFAULT;
    let floor_at = |p: V| mesh.poly_at(p).is_some();
    let mut rng = Rng::new(17);
    let mut walkers: Vec<(V, Vec<V>, V)> = (0..40)
        .map(|k| {
            let at = V::new(5.0 + (k % 8) as f32 * 2.5, 0.0, 5.0 + (k / 8) as f32 * 4.0);
            let goal = V::new(40.0 + rng.below(16) as f32, 0.0, 6.0 + rng.below(18) as f32);
            (at, route_to(&mesh, at, goal, f32::INFINITY), goal)
        })
        .collect();
    let dt = 1.0 / 30.0;
    let speed = 1.4;
    let mut arrived_at = vec![None; walkers.len()];
    for step in 0..(120.0 / dt) as usize {
        let placed: Vec<(V, Option<V>)> = walkers.iter().map(|(at, route, _)| (*at, route.last().map(|n| flat(*n - *at).try_normalize(1e-3).unwrap_or(V::zeros())))).collect();
        for (k, (at, route, goal)) in walkers.iter_mut().enumerate() {
            if arrived_at[k].is_some() {
                continue;
            }
            if flat(*goal - *at).norm() < 0.5 {
                arrived_at[k] = Some(step);
                continue;
            }
            if route.is_empty() {
                *route = route_to(&mesh, *at, *goal, f32::INFINITY);
            }
            let Some(&next) = route.last() else { continue };
            let to = flat(next - *at);
            if to.norm() < 0.3 {
                route.pop();
                continue;
            }
            let others = placed.iter().enumerate().filter(|&(j, _)| j != k && arrived_at[j].is_none()).map(|(_, o)| *o);
            let (way, _) = steer::make_way(&tuning, *at, to / to.norm(), others, floor_at);
            // As the game's collider would: the move, or failing that, sliding along the wall -
            // its part along x or along z, whichever is over floor.
            let step = way * speed * dt;
            for tried in [step, V::new(step.x, 0.0, 0.0), V::new(0.0, 0.0, step.z)] {
                if mesh.poly_at(*at + tried).is_some() {
                    *at += tried;
                    break;
                }
            }
            assert!(mesh.poly_at(*at).is_some(), "walker {k} off the floor at {at:?}");
            // Pushed off its way, it finds it again.
            if flat(next - *at).norm() > 6.0 {
                route.clear();
            }
        }
        if arrived_at.iter().all(Option::is_some) {
            break;
        }
    }
    for (k, (at, route, goal)) in walkers.iter().enumerate() {
        if arrived_at[k].is_none() {
            println!("  stuck: walker {k} at {:.2},{:.2} for {:.1},{:.1}, {} points left, next {:?}", at.x, at.z, goal.x, goal.z, route.len(), route.last());
        }
    }
    let got = arrived_at.iter().filter(|a| a.is_some()).count();
    let last = arrived_at.iter().flatten().max().map_or(f32::INFINITY, |&s| s as f32 * dt);
    println!("crowd: {got} of {} through the doorway, the last after {last:.1} s", walkers.len());
    assert_eq!(got, walkers.len(), "all through within two minutes");
}

#[test]
fn the_same_query_asked_again_gets_the_same_route_and_thousands_of_them_cost_little() {
    let grid = rooms(23, 12, 30);
    let mesh = NavMesh::build(&grid, V::zeros(), |_, _| 0);
    let cells: Vec<(usize, usize)> = mesh.walkable_cells().collect();
    let mut rng = Rng::new(31);
    let queries: Vec<(V, V)> = (0..3000).map(|_| (mesh.on_floor(cells[rng.below(cells.len())]), mesh.on_floor(cells[rng.below(cells.len())]))).collect();
    let first: Vec<Vec<V>> = queries.iter().map(|&(a, b)| route_to(&mesh, a, b, f32::INFINITY)).collect();
    let started = std::time::Instant::now();
    let again: Vec<Vec<V>> = queries.iter().map(|&(a, b)| route_to(&mesh, a, b, f32::INFINITY)).collect();
    let took = started.elapsed();
    assert_eq!(first, again, "the same every time");
    // Nonsense asked: nothing, and no panic.
    let nan = V::new(f32::NAN, 0.0, 0.0);
    assert!(route_to(&mesh, nan, queries[0].1, f32::INFINITY).is_empty());
    assert!(route_to(&mesh, queries[0].0, nan, f32::INFINITY).is_empty());
    // From off the floor altogether, from the nearest floor: on it from there.
    let off = V::new(-5.0, 0.0, -5.0);
    let from_off = route_to(&mesh, off, queries[0].1, f32::INFINITY);
    assert!(from_off.iter().all(|p| mesh.poly_at(*p).is_some()), "{from_off:?}");
    println!("{} routes over {} polygons: {:.3} ms each", queries.len(), mesh.len(), took.as_secs_f64() * 1e3 / queries.len() as f64);
}
