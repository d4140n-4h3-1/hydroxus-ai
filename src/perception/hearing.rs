//! How far a noise carries: along the corridors, round the corners, not through the walls. A
//! noise that carries `loudness` meters is heard by anyone that far along the floor from it.

use crate::navigation::navmesh::{NavMesh, Poly};
use nalgebra::Vector3;
use std::collections::HashMap;

/// A noise, and how far along the floor it carries.
#[derive(Debug, Clone, PartialEq)]
pub struct Heard {
    /// Each polygon it reaches, where it comes into it, and how far it has carried to there.
    reached: HashMap<Poly, ([f32; 2], f32)>,
    within: f32,
}

impl Heard {
    /// A noise made at `at`, over `mesh`, carrying `loudness` meters along the floor. None if it
    /// was made nowhere near the floor.
    pub fn at(mesh: &NavMesh, at: Vector3<f32>, loudness: f32) -> Option<Self> {
        let from = mesh.nearest(at)?;
        let mut reached = mesh.entries_from(at, loudness);
        reached.insert(from, ([at.x, at.z], 0.0));
        Some(Self { reached, within: loudness })
    }

    /// Whether it is heard by someone at `feet`: as far as it has carried to where the floor they
    /// stand on comes into it, and on from there to them.
    pub fn by(&self, mesh: &NavMesh, feet: Vector3<f32>) -> bool {
        mesh.poly_at(feet)
            .and_then(|n| self.reached.get(&n))
            .is_some_and(|&(entry, carried)| carried + (feet.x - entry[0]).hypot(feet.z - entry[1]) <= self.within)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::navigation::grid::WalkGrid;

    #[test]
    fn a_noise_carries_along_the_corridor_not_through_the_wall() {
        // Two corridors side by side, a wall between, joined at the far end.
        let mut grid = WalkGrid::new(12, 3, 1.0);
        for x in 0..12 {
            grid.set(x, 0, true);
            grid.set(x, 2, true);
        }
        grid.set(11, 1, true);
        let mesh = NavMesh::build(&grid, Vector3::zeros(), |_, _| 0);
        let heard = Heard::at(&mesh, Vector3::new(0.5, 0.0, 0.5), 6.0).unwrap();
        assert!(heard.by(&mesh, Vector3::new(3.5, 0.0, 0.5)), "along the corridor");
        assert!(!heard.by(&mesh, Vector3::new(7.5, 0.0, 0.5)), "not that far along it");
        assert!(!heard.by(&mesh, Vector3::new(0.5, 0.0, 2.5)), "through the wall, 2 m off");
        let loud = Heard::at(&mesh, Vector3::new(0.5, 0.0, 0.5), 60.0).unwrap();
        assert!(loud.by(&mesh, Vector3::new(0.5, 0.0, 2.5)), "round by the far end");
    }
}
