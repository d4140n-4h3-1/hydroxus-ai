//! Going about one's business in a city, over its navmesh: off the roads but to cross them, at
//! the crossings, and off the turf a gang keeps to - and never setting off for where there is no
//! getting to.

use crate::navigation::{
    grid::Rng,
    navmesh::{NavMesh, Poly},
    reach::Reach,
    walk::{plan, plan_avoiding},
};
use nalgebra::Vector3;

/// How much more a meter of road costs one going about its business than a meter of pavement:
/// enough that it walks to the nearest crossing rather than straight over, a block out of its way.
pub const ROAD_COST: f32 = 20.0;
/// How much more a meter across a group's turf costs the others than one along the pavement, so
/// that their routes go round it.
pub const TURF_COST: f32 = 3.0;

tuning!(
    /// How dear a wanderer counts the roads and the gangs' turf (see the module's constants, its
    /// defaults).
    {
        road_cost: f32 = ROAD_COST,
        turf_cost: f32 = TURF_COST,
    }
);

/// As [`plan`], a trip for an NPC going about its business from `feet`, `trip` meters away as the
/// crow flies - but keeping off the polygons of `mesh` that are `road` (crossings are not), if
/// there are roads, off the `turfs` of the groups that keep to one - each a spot and how many
/// meters round it is theirs - and off polygons it cannot get to (`reachable`): to somewhere off
/// them, the way that crosses them least (see [`plan_avoiding`]). One that finds itself on a road,
/// as it might after a chase, gets off it the shortest way. Empty, with nowhere found to go.
// Each argument is a separate thing the game already holds; bundling them would only move the
// count into a struct the game fills each call, and break the public API for nothing.
#[allow(clippy::too_many_arguments)]
pub fn wander(
    tuning: &Tuning,
    mesh: &NavMesh,
    feet: Vector3<f32>,
    trip: (f32, f32),
    road: Option<&dyn Fn(Poly) -> bool>,
    turfs: &[(Vector3<f32>, f32)],
    reachable: &Reach,
    rng: &mut Rng,
) -> Vec<Vector3<f32>> {
    let Some(road) = road else {
        return plan(mesh, feet, trip, rng);
    };
    let on_road = mesh.poly_at(feet).is_some_and(road);
    // On a group's turf: any of it within as far as the turf reaches of its spot.
    let on_turf = |n: Poly| {
        turfs.iter().any(|&(spot, reach)| {
            let near = mesh.closest_point(n, spot);
            (near.x - spot.x).hypot(near.z - spot.z) < reach
        })
    };
    plan_avoiding(
        mesh,
        feet,
        trip,
        |n| road(n) || on_turf(n) || !reachable.can(n as usize),
        |n| {
            let dear = if on_road || !road(n) { 0.0 } else { tuning.road_cost };
            dear + if on_turf(n) { tuning.turf_cost } else { 0.0 }
        },
        rng,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{flat, navigation::grid::WalkGrid};

    /// A navmesh of `grid`, with the cells `road` says as road, area 1.
    fn mesh(grid: &WalkGrid, road: impl Fn(usize, usize) -> bool) -> NavMesh {
        NavMesh::build(grid, Vector3::zeros(), |x, z| u8::from(road(x, z)))
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
        let mesh = mesh(&grid, |x, z| (16..24).contains(&x) && !(18..22).contains(&z));
        let road = |n: Poly| mesh.polygon(n).area == 1;
        let mut rng = Rng::new(11);
        let mut crossed = 0;
        for _ in 0..40 {
            let route = wander(&Tuning::DEFAULT, &mesh, Vector3::new(2.0, 0.12, 2.0), (5.0, 15.0), Some(&road), &[], &Reach::default(), &mut rng);
            assert!(!route.is_empty());
            // Its corners, and the way between them, off the road but at the crossing.
            let mut at = Vector3::new(2.0, 0.12, 2.0);
            for &next in route.iter().rev() {
                for k in 0..=10 {
                    let p = at + (next - at) * (k as f32 / 10.0);
                    assert!(!mesh.poly_at(p).is_some_and(road), "on the road at {p:?}: {route:?}");
                }
                at = next;
            }
            crossed += route.iter().any(|at| at.x > 12.0) as u32;
        }
        assert!(crossed > 0, "it does cross, at the crossing");
        // On the road, it gets off it.
        let route = wander(&Tuning::DEFAULT, &mesh, Vector3::new(10.0, 0.0, 2.0), (5.0, 15.0), Some(&road), &[], &Reach::default(), &mut rng);
        assert!(!mesh.poly_at(route[0]).is_some_and(road));
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
        let mesh = mesh(&grid, |x, _| x >= 38);
        let road = |n: Poly| mesh.polygon(n).area == 1;
        let spot = Vector3::new(10.0, 0.12, 10.0);
        let turfs = [(spot, 3.0)];
        let mut rng = Rng::new(5);
        for _ in 0..40 {
            let route = wander(&Tuning::DEFAULT, &mesh, Vector3::new(2.0, 0.12, 2.0), (5.0, 15.0), Some(&road), &turfs, &Reach::default(), &mut rng);
            if let Some(end) = route.first() {
                assert!(flat(end - spot).norm() >= 3.0, "makes for {end:?}, on the turf");
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
        let mesh = mesh(&grid, |_, _| false);
        let road = |_: Poly| false;
        let reach = Reach::of((0..mesh.len()).map(|n| mesh.polygon(n as Poly).x1 <= 25));
        let (mut rng, mut went) = (Rng::new(3), 0);
        for _ in 0..60 {
            let route = wander(&Tuning::DEFAULT, &mesh, Vector3::new(2.0, 0.0, 5.0), (10.0, 30.0), Some(&road), &[], &reach, &mut rng);
            assert!(route.iter().all(|at| at.x < 25.0), "set off for the far side: {route:?}");
            went += !route.is_empty() as u32;
        }
        assert!(went > 0, "it goes somewhere");
        // Not kept off it, it does go there now and then.
        let anywhere = (0..60).any(|_| {
            wander(&Tuning::DEFAULT, &mesh, Vector3::new(2.0, 0.0, 5.0), (10.0, 30.0), Some(&road), &[], &Reach::default(), &mut rng)
                .iter()
                .any(|at| at.x >= 25.0)
        });
        assert!(anywhere, "the far side is there to go to");
    }
}
