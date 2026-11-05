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
        self.nearness(mesh, feet).is_some()
    }

    /// How near it is to someone at `feet` who hears it, of as far as it carries: 1 right beside
    /// it, less the further along the floor, nothing at the very edge. None if they do not hear it.
    pub fn nearness(&self, mesh: &NavMesh, feet: Vector3<f32>) -> Option<f32> {
        let n = mesh.poly_at(feet)?;
        let &(entry, carried) = self.reached.get(&n)?;
        let along = carried + (feet.x - entry[0]).hypot(feet.z - entry[1]);
        (along <= self.within).then(|| (1.0 - along / self.within.max(1.0e-3)).clamp(0.0, 1.0))
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
        // Nearer along the corridor, the nearer it is.
        let (near, far) = (heard.nearness(&mesh, Vector3::new(1.5, 0.0, 0.5)), heard.nearness(&mesh, Vector3::new(4.5, 0.0, 0.5)));
        assert!(near.unwrap() > far.unwrap() && far.unwrap() > 0.0, "{near:?} {far:?}");
        assert_eq!(heard.nearness(&mesh, Vector3::new(7.5, 0.0, 0.5)), None);
    }
}
