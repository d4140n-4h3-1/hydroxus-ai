//! How far a noise carries: along the corridors, round the corners, not through the walls. A
//! noise that carries `loudness` meters is heard by anyone that far along the floor from it.

use crate::grid::{Routes, WalkGrid};
use nalgebra::Vector3;

/// A noise, and how far along the floor it carries.
#[derive(Debug, Clone, PartialEq)]
pub struct Heard {
    routes: Routes,
    within: f32,
}

impl Heard {
    /// A noise made at `at`, over `grid` whose corner is at `origin`, carrying `loudness` meters
    /// along the floor. None if it was made nowhere near the floor.
    pub fn at((grid, origin): (&WalkGrid, Vector3<f32>), at: Vector3<f32>, loudness: f32) -> Option<Self> {
        let from = grid.walkable_cell(origin, at)?;
        let within = loudness / grid.cell_size;
        Some(Self {
            routes: grid.routes_from(from, within),
            within,
        })
    }

    /// Whether it is heard by someone at `feet`.
    pub fn by(&self, (grid, origin): (&WalkGrid, Vector3<f32>), feet: Vector3<f32>) -> bool {
        grid.cell_at(origin, feet)
            .and_then(|(x, z)| self.routes.costs[z * grid.width + x])
            .is_some_and(|cost| cost <= self.within)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_noise_carries_along_the_corridor_not_through_the_wall() {
        // Two corridors side by side, a wall between, joined at the far end.
        let mut grid = WalkGrid::new(12, 3, 1.0);
        for x in 0..12 {
            grid.set(x, 0, true);
            grid.set(x, 2, true);
        }
        grid.set(11, 1, true);
        let ground = (&grid, Vector3::zeros());
        let heard = Heard::at(ground, Vector3::new(0.5, 0.0, 0.5), 6.0).unwrap();
        assert!(heard.by(ground, Vector3::new(3.5, 0.0, 0.5)), "along the corridor");
        assert!(!heard.by(ground, Vector3::new(0.5, 0.0, 2.5)), "through the wall, 2 m off");
        let loud = Heard::at(ground, Vector3::new(0.5, 0.0, 0.5), 60.0).unwrap();
        assert!(loud.by(ground, Vector3::new(0.5, 0.0, 2.5)), "round by the far end");
    }
}
