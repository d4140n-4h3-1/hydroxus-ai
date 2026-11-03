//! Which of a grid's cells can be got to from somewhere - where the player started, say - a bit
//! each by index. Kept so that NPCs going about their business never set off for where they
//! cannot get to: there, a search for the way would go over every cell it could within its bound
//! before it gave up - the tenth part of a second and more.

use crate::navigation::grid::WalkGrid;
use nalgebra::Vector3;

/// Which of a grid's cells can be got to, a bit each by index - or, empty, all of them.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Reach(Vec<u64>);

impl Reach {
    /// Those of `cells`, by index, that are true.
    pub fn of(cells: impl Iterator<Item = bool>) -> Self {
        let mut bits = Vec::new();
        for (i, can) in cells.enumerate() {
            if i % 64 == 0 {
                bits.push(0);
            }
            if can {
                *bits.last_mut().unwrap() |= 1 << (i % 64);
            }
        }
        Self(bits)
    }

    /// Those cells of `grid` that are reached from `from`, by any way there.
    pub fn from(grid: &WalkGrid, from: (usize, usize)) -> Self {
        let routes = grid.routes_from(from, f32::INFINITY);
        Self::of((0..grid.width * grid.rows()).map(|i| routes.cost(i).is_some()))
    }

    /// Whether the cell `index` can be got to.
    pub fn can(&self, index: usize) -> bool {
        self.0.is_empty() || self.0.get(index / 64).is_some_and(|bits| bits & (1 << (index % 64)) != 0)
    }

    /// Whether there is any way to `to` over `grid`, whose corner is at `origin`, at all.
    pub fn to(&self, (grid, origin): (&WalkGrid, Vector3<f32>), to: Vector3<f32>) -> bool {
        self.0.is_empty() || grid.walkable_cell(origin, to).is_some_and(|(x, z)| self.can(z * grid.width + x))
    }
}

impl Reach {
    /// Those polygons of `mesh` that can be got to from `from`, by any way there.
    pub fn on_mesh(mesh: &crate::navigation::navmesh::NavMesh, from: Vector3<f32>) -> Self {
        let costs = mesh.costs_from(from, f32::INFINITY, |_| 0.0);
        Self::of((0..mesh.len() as u32).map(|n| costs.contains_key(&n)))
    }

    /// Whether there is any way to `to` over `mesh` at all, as it was when this was worked out.
    pub fn to_on_mesh(&self, mesh: &crate::navigation::navmesh::NavMesh, to: Vector3<f32>) -> bool {
        self.0.is_empty() || mesh.nearest(to).is_some_and(|n| self.can(n as usize))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_can_be_got_to_is_kept_a_bit_a_cell_and_empty_is_anywhere() {
        let reach = Reach::of((0..130).map(|i| i % 3 == 0));
        assert!(reach.can(0) && reach.can(63) && reach.can(129));
        assert!(!reach.can(1) && !reach.can(64) && !reach.can(128));
        assert!(!reach.can(10_000), "past the end");
        assert!(Reach::default().can(10_000), "anywhere, with nothing known");
    }
}
