//! Routes over a [`WalkGrid`], as points on the floor for an NPC to walk through.
//!
//! A route is kept the way an NPC follows it: the first point last, so that the next one to make
//! for is `route.last()`, and reaching it is `route.pop()`.

use crate::grid::{Rng, WalkGrid};
use nalgebra::Vector3;

/// A number from `range.0` to `range.1`.
pub fn between(rng: &mut Rng, range: (f32, f32)) -> f32 {
    range.0 + (range.1 - range.0) * rng.below(1001) as f32 / 1000.0
}

/// The cells of `path` after the first, as points on the floor, the last one first: a route.
pub fn along(grid: &WalkGrid, origin: Vector3<f32>, path: Vec<(usize, usize)>) -> Vec<Vector3<f32>> {
    path.into_iter()
        .skip(1)
        .rev()
        .map(|cell| grid.on_floor(origin, cell))
        .collect()
}

/// A route over `grid`, whose corner is at `origin`, from `feet` to `to`, ending at `to` itself.
/// Empty if there is no way there within `reach` steps across the grid - `f32::INFINITY` for
/// anywhere at all.
pub fn route_to(
    (grid, origin): (&WalkGrid, Vector3<f32>),
    feet: Vector3<f32>,
    to: Vector3<f32>,
    reach: f32,
) -> Vec<Vector3<f32>> {
    route_to_weighted((grid, origin), feet, to, reach, |_, _| 0.0)
}

/// As [`route_to`], but each step into a cell `(x, z)` of the grid costs `extra(x, z)` more (see
/// [`WalkGrid::routes_from_weighted`]): a route that keeps out of harm's way where it can. Found
/// by heading for `to` ([`WalkGrid::route_between_weighted`]), so it costs about as much as the
/// route is long, however big the grid.
pub fn route_to_weighted(
    (grid, origin): (&WalkGrid, Vector3<f32>),
    feet: Vector3<f32>,
    to: Vector3<f32>,
    reach: f32,
    extra: impl Fn(usize, usize) -> f32,
) -> Vec<Vector3<f32>> {
    let (Some(from), Some(goal)) = (grid.walkable_cell(origin, feet), grid.walkable_cell(origin, to)) else {
        return Vec::new();
    };
    let Some(path) = grid.route_between_weighted(from, goal, reach, extra) else {
        return Vec::new();
    };
    let mut route = along(grid, origin, path);
    let end = Vector3::new(to.x, route.first().map_or(feet.y, |end| end.y), to.z);
    match route.first_mut() {
        Some(last) => *last = end,
        None => route.push(end),
    }
    route
}

/// A route over `grid`, whose corner is at `origin`, from `feet` to somewhere picked by `rng` a
/// `trip` away - from `trip.0` to `trip.1` steps across the grid - or failing that, in a small
/// space, anywhere else at all. Empty if there is nowhere to go.
pub fn plan(
    (grid, origin): (&WalkGrid, Vector3<f32>),
    feet: Vector3<f32>,
    trip: (f32, f32),
    rng: &mut Rng,
) -> Vec<Vector3<f32>> {
    let Some(from) = grid.cell_at(origin, feet).filter(|&(x, z)| grid.is_walkable(x, z)) else {
        return Vec::new();
    };
    let routes = grid.routes_from(from, trip.1);
    let reached = |range: (f32, f32)| -> Vec<usize> {
        routes
            .costs
            .iter()
            .enumerate()
            .filter(|(_, cost)| cost.is_some_and(|c| c >= range.0 && c <= range.1))
            .map(|(i, _)| i)
            .collect()
    };
    let mut choices = reached(trip);
    if choices.is_empty() {
        choices = reached((f32::MIN_POSITIVE, f32::INFINITY));
    }
    if choices.is_empty() {
        return Vec::new();
    }
    let goal = choices[rng.below(choices.len())];
    let Some(path) = routes.path_to((goal % grid.width, goal / grid.width)) else {
        return Vec::new();
    };
    along(grid, origin, path)
}

/// How much further than as the crow flies an NPC going about its business will walk to get
/// somewhere, before giving it up for somewhere else (see [`plan_avoiding`]).
pub const DETOUR: f32 = 2.5;

/// As [`plan`], a trip from `feet` - but to somewhere not `kept_off`, a `trip` away as the crow
/// flies, on any floor there, the way that costs least with `extra` added to stepping into each
/// cell: off a city's roads, say, the way that crosses them least. The way there is found by
/// heading for it, so it costs about as much as the route is long, however big the grid; a spot
/// it cannot get to within [`DETOUR`] times as far, it tries no further for. Empty, with nowhere
/// found to go.
pub fn plan_avoiding(
    (grid, origin): (&WalkGrid, Vector3<f32>),
    feet: Vector3<f32>,
    trip: (f32, f32),
    kept_off: impl Fn(usize, usize) -> bool,
    extra: impl Fn(usize, usize) -> f32,
    rng: &mut Rng,
) -> Vec<Vector3<f32>> {
    let Some(from) = grid.cell_at(origin, feet).filter(|&(x, z)| grid.is_walkable(x, z)) else {
        return Vec::new();
    };
    let goal = (0..24).find_map(|_| {
        let angle = between(rng, (-std::f32::consts::PI, std::f32::consts::PI));
        let away = between(rng, trip) * grid.cell_size;
        let spot = feet + Vector3::new(angle.sin(), 0.0, angle.cos()) * away;
        let plan = grid.plan(grid.cell_at(origin, spot)?);
        let floors: Vec<(usize, usize)> = grid.cells_over(plan).filter(|&(x, z)| !kept_off(x, z)).collect();
        (!floors.is_empty()).then(|| (floors[rng.below(floors.len())], away / grid.cell_size))
    });
    let Some((goal, steps)) = goal else {
        return Vec::new();
    };
    grid.route_between_weighted(from, goal, steps * DETOUR + 20.0, extra)
        .map_or_else(Vec::new, |path| along(grid, origin, path))
}

/// Somewhere on the floor of `grid`, whose corner is at `origin`, within `reach` of `at` as the
/// crow flies, picked by `rng` - or, with nowhere there, the floor nearest `at`.
pub fn spot_near(
    (grid, origin): (&WalkGrid, Vector3<f32>),
    at: Vector3<f32>,
    reach: f32,
    rng: &mut Rng,
) -> Option<Vector3<f32>> {
    for _ in 0..24 {
        let angle = between(rng, (-std::f32::consts::PI, std::f32::consts::PI));
        let away = between(rng, (0.0, reach));
        let spot = at + Vector3::new(angle.sin(), 0.0, angle.cos()) * away;
        if let Some(cell) = grid.cell_at(origin, spot).filter(|&(x, z)| grid.is_walkable(x, z)) {
            return Some(grid.on_floor(origin, cell));
        }
    }
    grid.nearest_walkable(origin, at).map(|cell| grid.on_floor(origin, cell))
}

/// Somewhere on any floor of `grid` within `reach` of `at` as the crow flies - in a building, up
/// on one of its floors as likely as on the ground - picked by `rng`; or, with nowhere there,
/// the floor nearest `at`.
pub fn spot_on_any_floor_near(
    (grid, origin): (&WalkGrid, Vector3<f32>),
    at: Vector3<f32>,
    reach: f32,
    rng: &mut Rng,
) -> Option<Vector3<f32>> {
    for _ in 0..24 {
        let angle = between(rng, (-std::f32::consts::PI, std::f32::consts::PI));
        let away = between(rng, (0.0, reach));
        let spot = at + Vector3::new(angle.sin(), 0.0, angle.cos()) * away;
        let Some(cell) = grid.cell_at(origin, spot) else {
            continue;
        };
        let floors: Vec<(usize, usize)> = grid.cells_over(grid.plan(cell)).collect();
        if !floors.is_empty() {
            return Some(grid.on_floor(origin, floors[rng.below(floors.len())]));
        }
    }
    grid.nearest_walkable(origin, at).map(|cell| grid.on_floor(origin, cell))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A corridor 20 cells long, a meter across each, round a corner.
    fn corridor() -> WalkGrid {
        let mut grid = WalkGrid::new(20, 20, 1.0);
        for i in 0..20 {
            grid.set(i, 0, true);
            grid.set(19, i, true);
        }
        grid
    }

    #[test]
    fn a_route_ends_where_it_was_asked_to_go_and_is_followed_from_its_end() {
        let grid = corridor();
        let to = Vector3::new(19.3, 0.0, 15.2);
        let route = route_to((&grid, Vector3::zeros()), Vector3::new(0.5, 0.0, 0.5), to, f32::INFINITY);
        assert_eq!(route.first(), Some(&to), "ending at `to` itself");
        let next = route.last().unwrap();
        assert!((next - Vector3::new(1.5, 0.0, 0.5)).norm() < 1.0e-5, "the next step first: {next}");
    }

    #[test]
    fn beyond_its_reach_there_is_no_route() {
        let grid = corridor();
        let (from, to) = (Vector3::new(0.5, 0.0, 0.5), Vector3::new(19.5, 0.0, 19.5));
        assert!(route_to((&grid, Vector3::zeros()), from, to, 10.0).is_empty());
        assert!(!route_to((&grid, Vector3::zeros()), from, to, f32::INFINITY).is_empty());
    }

    #[test]
    fn a_trip_goes_about_as_far_as_asked() {
        let grid = corridor();
        let mut rng = Rng::new(5);
        for _ in 0..20 {
            let route = plan((&grid, Vector3::zeros()), Vector3::new(0.5, 0.0, 0.5), (8.0, 12.0), &mut rng);
            // Every cell of a corridor a cell wide is next to a wall, and so dear: 2 a step.
            assert!((4..=6).contains(&route.len()), "{}", route.len());
        }
    }

    #[test]
    fn between_stays_between() {
        let mut rng = Rng::new(9);
        assert!((0..1000).map(|_| between(&mut rng, (2.0, 3.0))).all(|n| (2.0..=3.0).contains(&n)));
    }
}
