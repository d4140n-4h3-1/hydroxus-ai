//! Times the navmesh against the grid, on a grid dumped by the game: run the game with
//! MAZE_NAVMESH_STATS=1 MAZE_NAVMESH_DUMP=<file> on a map, then
//! `cargo run --release --example navmesh_bench -- <file>`.
use hydroxus_ai::navigation::{grid::{Rng, WalkGrid}, navmesh::NavMesh, route::route_to};
use nalgebra::Vector3;
use std::time::Instant;

fn main() {
    let path = std::env::args().nth(1).expect("a dumped grid");
    let bytes = std::fs::read(path).unwrap();
    let u = |i: usize| u32::from_le_bytes(bytes[i..i + 4].try_into().unwrap()) as usize;
    let f = |i: usize| f32::from_le_bytes(bytes[i..i + 4].try_into().unwrap());
    let (width, depth, storeys) = (u(0), u(4), u(8));
    let (cell, origin) = (f(12), Vector3::new(f(16), f(20), f(24)));
    let mut grid = WalkGrid::with_storeys(width, depth, storeys, cell);
    let mut at = 28;
    for row in 0..depth * storeys {
        for x in 0..width {
            grid.set(x, row, bytes[at] != 0);
            grid.set_floor(x, row, f(at + 1));
            at += 5;
        }
    }
    let t = Instant::now();
    let mesh = NavMesh::build(&grid, origin, |_, _| 0);
    println!("built {} polygons, {} portals in {:.1} ms", mesh.len(), mesh.portal_count(), t.elapsed().as_secs_f64() * 1e3);
    let mut rng = Rng::new(7);
    let mut pairs = Vec::new();
    while pairs.len() < 200 {
        let (x, z) = (rng.below(grid.width), rng.below(grid.rows()));
        let (x2, z2) = (rng.below(grid.width), rng.below(grid.rows()));
        if !grid.is_walkable(x, z) || !grid.is_walkable(x2, z2) {
            continue;
        }
        let (a, b) = (grid.on_floor(origin, (x, z)), grid.on_floor(origin, (x2, z2)));
        let apart = (a - b).norm();
        if (20.0..200.0).contains(&apart) {
            pairs.push((a, b));
        } else if apart >= 200.0 {
            pairs.push((a, a + (b - a) * (rng.below(150) as f32 + 20.0) / apart));
        }
    }
    // Warm the grid's links.
    let _ = route_to((&grid, origin), pairs[0].0, pairs[0].1, f32::INFINITY);
    let t = Instant::now();
    for &(a, b) in &pairs {
        std::hint::black_box(route_to((&grid, origin), a, b, 200.0 / cell * 3.0));
    }
    println!("grid route {:.3} ms", t.elapsed().as_secs_f64() * 5.0);
    let t = Instant::now();
    for &(a, b) in &pairs {
        std::hint::black_box((mesh.nearest(a), mesh.nearest(b)));
    }
    println!("navmesh nearest x2 {:.3} ms", t.elapsed().as_secs_f64() * 5.0);
    let t = Instant::now();
    for &(a, b) in &pairs {
        std::hint::black_box(mesh.route(a, b, 600.0, 0.3, |_| 0.0));
    }
    println!("navmesh route {:.3} ms", t.elapsed().as_secs_f64() * 5.0);
    let t = Instant::now();
    for &(a, _) in &pairs {
        std::hint::black_box(mesh.costs_from(a, 600.0, |_| 0.0));
    }
    println!("navmesh costs to everywhere {:.3} ms", t.elapsed().as_secs_f64() * 5.0);
}
