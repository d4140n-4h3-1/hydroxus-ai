//! Getting about over a [`NavMesh`]: routes to somewhere, trips a way off, spots near a point, and
//! the floor at a point's height - as [`crate::navigation::route`] has them over a grid, but in
//! meters rather than steps, and a route as its corners rather than a point a cell.
//!
//! A route is kept [`CLEARANCE`] in from the ends of the portals it goes through, so that it
//! keeps off the walls it rounds, and cuts every other corner. As the grid's are, its points are
//! the last first - the next to make for at its end - and the start is left out.

use crate::navigation::{
    grid::Rng,
    navmesh::{NavMesh, Poly},
    route::between,
};
use nalgebra::Vector3;

/// How far a route keeps from the walls it rounds, in meters: a droid's half width, and a little.
pub const CLEARANCE: f32 = 0.4;
/// How much further than as the crow flies an NPC going about its business will walk to get
/// somewhere, before giving it up for somewhere else (see [`plan_avoiding`]), and a little more.
pub const DETOUR: f32 = 2.5;
const DETOUR_MORE: f32 = 10.0;
/// How far from a point, at most, a floor at its height is looked for, in meters; and how far
/// above or below it a floor can be and still be at its height.
pub const NEAR: f32 = 6.0;
pub const LEVEL: f32 = 1.0;
/// How many spots are tried, picking somewhere at random.
const TRIES: usize = 24;

/// A route over `mesh` from `feet` to `to`, ending at `to` itself - empty if there is no way there
/// within `reach` meters of walking; `f32::INFINITY` for anywhere at all.
pub fn route_to(mesh: &NavMesh, feet: Vector3<f32>, to: Vector3<f32>, reach: f32) -> Vec<Vector3<f32>> {
    mesh.route(feet, to, reach, CLEARANCE, |_| 0.0)
}

/// As [`route_to`], but each meter across polygon `n` costing `extra(n)` more: a route that keeps
/// out of harm's way - off a road, out of sight - where it can.
pub fn route_to_weighted(
    mesh: &NavMesh,
    feet: Vector3<f32>,
    to: Vector3<f32>,
    reach: f32,
    extra: impl Fn(Poly) -> f32,
) -> Vec<Vector3<f32>> {
    mesh.route(feet, to, reach, CLEARANCE, extra)
}

/// A route over `mesh` from `feet` to somewhere picked by `rng` a `trip` away - from `trip.0` to
/// `trip.1` meters of walking - or failing that, in a small space, anywhere it can get to. Empty
/// if there is nowhere to go.
pub fn plan(mesh: &NavMesh, feet: Vector3<f32>, trip: (f32, f32), rng: &mut Rng) -> Vec<Vector3<f32>> {
    let costs = mesh.costs_from(feet, trip.1, |_| 0.0);
    let here = mesh.nearest(feet);
    let mut choices: Vec<Poly> = costs.iter().filter(|&(_, &c)| c >= trip.0).map(|(&n, _)| n).collect();
    if choices.is_empty() {
        choices = costs.keys().copied().filter(|&n| Some(n) != here).collect();
    }
    if choices.is_empty() {
        return Vec::new();
    }
    choices.sort_unstable();
    let goal = choices[rng.below(choices.len())];
    let to = mesh.random_point(goal, &mut |k| rng.below(k));
    route_to(mesh, feet, to, trip.1 * DETOUR + DETOUR_MORE)
}

/// As [`plan`], a trip from `feet` - but to somewhere not `kept_off`, about a `trip` away as the
/// crow flies, in meters, on any floor there, the way that costs least with each meter across
/// polygon `n` costing `extra(n)` more: off a city's roads, say, the way that crosses them least.
/// A spot it cannot get to within [`DETOUR`] times as far, it tries no further for. Empty, with
/// nowhere found to go.
pub fn plan_avoiding(
    mesh: &NavMesh,
    feet: Vector3<f32>,
    trip: (f32, f32),
    kept_off: impl Fn(Poly) -> bool,
    extra: impl Fn(Poly) -> f32,
    rng: &mut Rng,
) -> Vec<Vector3<f32>> {
    if mesh.nearest(feet).is_none() {
        return Vec::new();
    }
    let goal = (0..TRIES).find_map(|_| {
        let angle = between(rng, (-std::f32::consts::PI, std::f32::consts::PI));
        let away = between(rng, trip);
        let spot = feet + Vector3::new(angle.sin(), 0.0, angle.cos()) * away;
        let floors: Vec<Poly> = mesh.over_spot(spot).into_iter().filter(|&n| !kept_off(n)).collect();
        (!floors.is_empty()).then(|| {
            let n = floors[rng.below(floors.len())];
            (Vector3::new(spot.x, mesh.floor_of(n), spot.z), away)
        })
    });
    let Some((to, away)) = goal else {
        return Vec::new();
    };
    route_to_weighted(mesh, feet, to, away * DETOUR + DETOUR_MORE, extra)
}

/// Somewhere on the floor of `mesh` within `reach` of `at` as the crow flies, on the floor nearest
/// its height there, picked by `rng` - or, with nowhere there, the floor nearest `at`.
pub fn spot_near(mesh: &NavMesh, at: Vector3<f32>, reach: f32, rng: &mut Rng) -> Option<Vector3<f32>> {
    for _ in 0..TRIES {
        let angle = between(rng, (-std::f32::consts::PI, std::f32::consts::PI));
        let away = between(rng, (0.0, reach));
        let spot = at + Vector3::new(angle.sin(), 0.0, angle.cos()) * away;
        let nearest = mesh
            .over_spot(spot)
            .into_iter()
            .min_by(|&a, &b| (mesh.floor_of(a) - at.y).abs().total_cmp(&(mesh.floor_of(b) - at.y).abs()));
        if let Some(n) = nearest {
            return Some(Vector3::new(spot.x, mesh.floor_of(n), spot.z));
        }
    }
    mesh.nearest(at).map(|n| mesh.closest_point(n, at))
}

/// Somewhere on any floor of `mesh` within `reach` of `at` as the crow flies - in a building, up
/// on one of its floors as likely as on the ground - picked by `rng`; or, with nowhere there, the
/// floor nearest `at`.
pub fn spot_on_any_floor_near(mesh: &NavMesh, at: Vector3<f32>, reach: f32, rng: &mut Rng) -> Option<Vector3<f32>> {
    for _ in 0..TRIES {
        let angle = between(rng, (-std::f32::consts::PI, std::f32::consts::PI));
        let away = between(rng, (0.0, reach));
        let spot = at + Vector3::new(angle.sin(), 0.0, angle.cos()) * away;
        let floors = mesh.over_spot(spot);
        if !floors.is_empty() {
            let n = floors[rng.below(floors.len())];
            return Some(Vector3::new(spot.x, mesh.floor_of(n), spot.z));
        }
    }
    mesh.nearest(at).map(|n| mesh.closest_point(n, at))
}

/// The floor nearest `point`, within [`NEAR`] of it, at its height ([`LEVEL`]): where it is, if
/// there is floor there at its height, or else the nearest point that is. None with no floor at
/// its height near it.
pub fn floor_near(mesh: &NavMesh, point: Vector3<f32>) -> Option<Vector3<f32>> {
    mesh.floor_near(point, NEAR, LEVEL)
}

/// Whether there is floor at `spot`, within `level` of its height.
pub fn floor_at(mesh: &NavMesh, spot: Vector3<f32>, level: f32) -> bool {
    mesh.over_spot(spot).into_iter().any(|n| (mesh.floor_of(n) - spot.y).abs() <= level)
}

/// The height of the floor under `feet` - of those over where it stands, the nearest its height -
/// if there is one within [`LEVEL`] of it.
pub fn floor_under(mesh: &NavMesh, feet: Vector3<f32>) -> Option<f32> {
    mesh.poly_at(feet).map(|n| mesh.floor_of(n))
}

/// Somewhere on the floor of `mesh` at least `away` meters from `from` as the crow flies, picked
/// at random by `rng`, as likely anywhere as anywhere else of its floor; none if none turns up in
/// so many tries.
pub fn far_spot(mesh: &NavMesh, from: Vector3<f32>, away: f32, rng: &mut Rng) -> Option<Vector3<f32>> {
    let (min, max) = mesh.bounds();
    for _ in 0..2000 {
        let spot = Vector3::new(between(rng, (min.x, max.x)), 0.0, between(rng, (min.z, max.z)));
        if (spot.x - from.x).hypot(spot.z - from.z) < away {
            continue;
        }
        let floors = mesh.over_spot(spot);
        if floors.is_empty() {
            continue;
        }
        let n = floors[rng.below(floors.len())];
        return Some(Vector3::new(spot.x, mesh.floor_of(n), spot.z));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::navigation::grid::WalkGrid;

    /// A corridor 20 cells long, a meter across each, round a corner.
    fn corridor() -> NavMesh {
        let mut grid = WalkGrid::new(20, 20, 1.0);
        for i in 0..20 {
            grid.set(i, 0, true);
            grid.set(19, i, true);
        }
        NavMesh::build(&grid, Vector3::zeros(), |_, _| 0)
    }

    #[test]
    fn a_route_ends_where_it_was_asked_to_go_and_none_is_beyond_its_reach() {
        let mesh = corridor();
        let to = Vector3::new(19.3, 0.0, 15.2);
        let route = route_to(&mesh, Vector3::new(0.5, 0.0, 0.5), to, f32::INFINITY);
        assert_eq!(route.first(), Some(&to), "ending at `to` itself");
        assert!(route.len() >= 2, "round the corner: {route:?}");
        assert!(route.iter().all(|&p| mesh.poly_at(p).is_some()), "on the floor: {route:?}");
        assert!(route_to(&mesh, Vector3::new(0.5, 0.0, 0.5), Vector3::new(19.5, 0.0, 19.5), 10.0).is_empty());
    }

    #[test]
    fn a_trip_goes_a_trip_away_and_a_spot_near_is_on_the_floor_near() {
        let mut grid = WalkGrid::new(60, 60, 0.5);
        for z in 0..60 {
            for x in 0..60 {
                grid.set(x, z, true);
            }
        }
        // Areas every 5 cells, to have polygons to pick from.
        let mesh = NavMesh::build(&grid, Vector3::zeros(), |x, z| ((x / 5) * 13 + z / 5) as u8);
        let mut rng = Rng::new(5);
        let feet = Vector3::new(15.0, 0.0, 15.0);
        for _ in 0..20 {
            let route = plan(&mesh, feet, (5.0, 10.0), &mut rng);
            let end = route[0];
            assert!((end - feet).norm() > 2.0 && (end - feet).norm() < 12.0, "{end:?}");
            let spot = spot_near(&mesh, feet, 3.0, &mut rng).unwrap();
            assert!((spot - feet).norm() <= 3.01);
        }
        let off = spot_near(&mesh, Vector3::new(-20.0, 0.0, 15.0), 1.0, &mut rng).unwrap();
        assert!(off.x >= 0.0 && off.x < 1.0, "the floor nearest: {off:?}");
        let route = plan_avoiding(&mesh, feet, (5.0, 10.0), |n| mesh.centre(n).x < 15.0, |_| 0.0, &mut rng);
        assert!(route[0].x >= 14.0, "{:?}", route[0]);
        assert!(far_spot(&mesh, feet, 12.0, &mut rng).is_some_and(|s| (s.x - feet.x).hypot(s.z - feet.z) >= 12.0));
        assert!(floor_at(&mesh, feet, 0.5) && !floor_at(&mesh, feet + Vector3::y() * 2.0, 0.5));
        assert_eq!(floor_under(&mesh, feet), Some(0.0));
    }
}
