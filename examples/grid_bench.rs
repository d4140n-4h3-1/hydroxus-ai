//! Times surveying a level into a grid and making the navmesh from it: a 1,200 by 1,200 cell
//! grid (600 m across at 0.5 m cells, two storeys) of rooms and corridors, set cell by cell as a
//! game's survey does, its walkable cells walked from one corner, then built into a navmesh.
//! `cargo run --release --example grid_bench`.
use hydroxus_ai::navigation::{grid::WalkGrid, navmesh::NavMesh};
use nalgebra::Vector3;
use std::{hint::black_box, time::Instant};

fn main() {
    const SIDE: usize = 1200;
    let runs = 5;
    let (mut survey, mut cells, mut walk, mut build) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let mut polygons = 0;
    for _ in 0..runs {
        let start = Instant::now();
        let mut grid = WalkGrid::with_storeys(SIDE, SIDE, 2, 0.5);
        for storey in 0..2 {
            for z in 0..SIDE {
                for x in 0..SIDE {
                    // Rooms 40 cells square, walls 2 thick, doorways 6 wide in the middle of each.
                    let (rx, rz) = (x % 42, z % 42);
                    let wall_x = rx >= 40 && !(17..23).contains(&rz);
                    let wall_z = rz >= 40 && !(17..23).contains(&rx);
                    let row = storey * SIDE + z;
                    grid.set(x, row, !(wall_x || wall_z));
                    grid.set_floor(x, row, storey as f32 * 4.0);
                }
            }
        }
        survey.push(start.elapsed().as_secs_f64() * 1000.0);
        let start = Instant::now();
        black_box(grid.walkable_cells().count());
        cells.push(start.elapsed().as_secs_f64() * 1000.0);
        let start = Instant::now();
        let mut walkable = 0usize;
        for z in 0..grid.rows() {
            for x in 0..SIDE {
                walkable += grid.is_walkable(x, z) as usize + (grid.floor(x, z) > 100.0) as usize;
            }
        }
        black_box(walkable);
        walk.push(start.elapsed().as_secs_f64() * 1000.0);
        let start = Instant::now();
        let mesh = NavMesh::build(&grid, Vector3::zeros(), |_, _| 0);
        build.push(start.elapsed().as_secs_f64() * 1000.0);
        polygons = mesh.len();
    }
    let show = |name: &str, v: &[f64]| {
        let mean = v.iter().sum::<f64>() / v.len() as f64;
        let best = v.iter().cloned().fold(f64::MAX, f64::min);
        println!("{name}: mean {mean:.1} ms, best {best:.1} ms over {runs} runs");
    };
    println!("grid: {SIDE} x {SIDE} cells, 2 storeys; navmesh {polygons} polygons");
    show("survey (set + set_floor per cell)", &survey);
    show("walkable_cells", &cells);
    show("is_walkable + floor, every cell", &walk);
    show("navmesh build", &build);
}
