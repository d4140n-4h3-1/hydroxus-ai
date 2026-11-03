//! Going about one's business in a city: off the roads but to cross them, at the crossings, and
//! off the turf a gang keeps to - and never setting off for where there is no getting to.

use crate::navigation::{
    grid::{Rng, WalkGrid},
    reach::Reach,
    route::{plan, plan_avoiding},
};
use nalgebra::Vector3;

/// Which cells of a walk grid are road, to keep off but to cross.
pub trait Roads {
    /// Whether cell `(x, row)` is road, and not a crossing.
    fn is_road(&self, x: usize, row: usize) -> bool;
    /// What a step into cell `(x, row)` costs over a step along the pavement.
    fn extra(&self, x: usize, row: usize) -> f32;
}

/// How much more a step onto a group's turf costs than one along the pavement, so that the
/// routes of the others go round it.
pub const TURF_COST: f32 = 3.0;

/// As [`plan`], a trip for an NPC going about its business from `feet` - but keeping off the
/// `roads`, if there are any, and off the `turfs` of the groups that keep to one, each a cell and
/// how many cells round it is theirs, and off cells it cannot get to (`reachable`): to somewhere
/// off them a `trip` away as the crow flies, the way that crosses them least (see
/// [`plan_avoiding`]). One that finds itself on a road, as it might after a chase, gets off it the
/// shortest way. Empty, with nowhere found to go.
pub fn wander(
    (grid, origin): (&WalkGrid, Vector3<f32>),
    feet: Vector3<f32>,
    trip: (f32, f32),
    roads: Option<&impl Roads>,
    turfs: &[((usize, usize), f32)],
    reachable: &Reach,
    rng: &mut Rng,
) -> Vec<Vector3<f32>> {
    let Some(roads) = roads else {
        return plan((grid, origin), feet, trip, rng);
    };
    let on_road = grid.cell_at(origin, feet).is_some_and(|(x, z)| roads.is_road(x, z));
    // On a group's turf: within as many cells as it reaches of the cell its spot is in.
    let on_turf = |x: usize, z: usize| {
        turfs.iter().any(|&((tx, tz), reach)| {
            let (dx, dz) = (x as f32 - tx as f32, z as f32 - tz as f32);
            dx * dx + dz * dz < reach * reach
        })
    };
    plan_avoiding(
        (grid, origin),
        feet,
        trip,
        |x, z| roads.is_road(x, z) || on_turf(x, z) || !reachable.can(z * grid.width + x),
        |x, z| {
            let road = if on_road { 0.0 } else { roads.extra(x, z) };
            road + if on_turf(x, z) { TURF_COST } else { 0.0 }
        },
        rng,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flat;

    /// Road over the cells of `roads` but not of `crossings`, each `[x0, z0, x1, z1)` in cells.
    struct TestRoads {
        width: usize,
        road: Vec<bool>,
    }

    impl TestRoads {
        fn new(grid: &WalkGrid, roads: &[[usize; 4]], crossings: &[[usize; 4]]) -> Self {
            let mut road = vec![false; grid.width * grid.rows()];
            for (rects, is) in [(roads, true), (crossings, false)] {
                for r in rects {
                    for z in r[1]..r[3].min(grid.depth) {
                        for x in r[0]..r[2].min(grid.width) {
                            road[z * grid.width + x] = is;
                        }
                    }
                }
            }
            Self { width: grid.width, road }
        }
    }

    impl Roads for TestRoads {
        fn is_road(&self, x: usize, row: usize) -> bool {
            self.road.get(row * self.width + x).copied().unwrap_or(false)
        }
        fn extra(&self, x: usize, row: usize) -> f32 {
            if self.is_road(x, row) {
                20.0
            } else {
                0.0
            }
        }
    }

    #[test]
    fn going_about_its_business_it_keeps_off_the_road_but_to_cross() {
        // A street 4 m wide down the middle of ground 20 m square, a crossing over it at z 10.
        let mut grid = WalkGrid::new(40, 40, 0.5);
        for x in 0..40 {
            for z in 0..40 {
                grid.set(x, z, true);
                grid.set_floor(x, z, if (16..24).contains(&x) { 0.0 } else { 0.12 });
            }
        }
        let roads = TestRoads::new(&grid, &[[16, 0, 24, 40]], &[[16, 18, 24, 22]]);
        let mut rng = Rng::new(11);
        let mut crossed = 0;
        for _ in 0..40 {
            let route = wander((&grid, Vector3::zeros()), Vector3::new(2.0, 0.12, 2.0), (10.0, 30.0), Some(&roads), &[], &Reach::default(), &mut rng);
            assert!(!route.is_empty());
            for at in &route {
                let (x, z) = grid.cell_at(Vector3::zeros(), *at).unwrap();
                assert!(!roads.is_road(x, z), "on the road at {at:?}");
            }
            crossed += route.iter().any(|at| at.x > 12.0) as u32;
        }
        assert!(crossed > 0, "it does cross, at the crossing");
        // On the road, it gets off it.
        let route = wander((&grid, Vector3::zeros()), Vector3::new(10.0, 0.0, 2.0), (10.0, 30.0), Some(&roads), &[], &Reach::default(), &mut rng);
        let end = route.first().unwrap();
        let (x, z) = grid.cell_at(Vector3::zeros(), *end).unwrap();
        assert!(!roads.is_road(x, z));
    }

    #[test]
    fn going_about_its_business_it_keeps_off_a_gangs_turf() {
        // Ground 20 m square, with a street at its edge, and a gang's turf 3 m round in the
        // middle of the pavement.
        let mut grid = WalkGrid::new(40, 40, 0.5);
        for x in 0..40 {
            for z in 0..40 {
                grid.set(x, z, true);
                grid.set_floor(x, z, 0.12);
            }
        }
        let roads = TestRoads::new(&grid, &[[38, 0, 40, 40]], &[]);
        let spot = Vector3::new(10.0, 0.12, 10.0);
        let turfs = [(grid.cell_at(Vector3::zeros(), spot).unwrap(), 3.0 / 0.5)];
        let mut rng = Rng::new(5);
        for _ in 0..40 {
            let route = wander((&grid, Vector3::zeros()), Vector3::new(2.0, 0.12, 2.0), (10.0, 30.0), Some(&roads), &turfs, &Reach::default(), &mut rng);
            if let Some(end) = route.first() {
                // Give or take the half-meter cells it is reckoned in.
                assert!(flat(end - spot).norm() >= 2.5, "makes for {end:?}, on the turf");
            }
            for at in &route {
                assert!(flat(at - spot).norm() > 1.5, "goes by {at:?}, through the middle of the turf");
            }
        }
    }

    #[test]
    fn going_about_its_business_it_does_not_set_off_for_where_it_cannot_get_to() {
        // Two pavements, a path between them; the far one, as if walled off, not to be got to.
        let mut grid = WalkGrid::new(40, 10, 1.0);
        for z in 0..10 {
            for x in (0..15).chain(25..40) {
                grid.set(x, z, true);
            }
        }
        for x in 15..25 {
            grid.set(x, 0, true);
        }
        let roads = TestRoads::new(&grid, &[], &[]);
        let reach = Reach::of((0..grid.width * grid.rows()).map(|i| i % grid.width < 25));
        let (mut rng, mut went) = (Rng::new(3), 0);
        for _ in 0..60 {
            let route = wander((&grid, Vector3::zeros()), Vector3::new(2.0, 0.0, 5.0), (10.0, 30.0), Some(&roads), &[], &reach, &mut rng);
            assert!(route.iter().all(|at| at.x < 25.0), "set off for the far side: {route:?}");
            went += !route.is_empty() as u32;
        }
        assert!(went > 0, "it goes somewhere");
        // Not kept off it, it does go there now and then.
        let anywhere = (0..60).any(|_| {
            wander((&grid, Vector3::zeros()), Vector3::new(2.0, 0.0, 5.0), (10.0, 30.0), Some(&roads), &[], &Reach::default(), &mut rng)
                .iter()
                .any(|at| at.x >= 25.0)
        });
        assert!(anywhere, "the far side is there to go to");
    }
}
