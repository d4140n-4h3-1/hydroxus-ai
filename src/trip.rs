//! Getting about over more than one floor: NPCs' ways to where they are going, at the height
//! it is at, and across by ferry where that is quicker than walking.
//!
//! A [`WalkGrid`] has one floor to a cell, and a point is on whichever floor its cell has - the
//! catwalk over the sunken plaza, say, for someone down in the plaza under it. A point an NPC is
//! going to is put on the floor nearest it at its own height instead (see [`floor_near`]).
//!
//! Ferries - platforms that carry whoever stands on them between two places, waiting at each end
//! a while - are no part of the grid (see [`Crossing`]): a ferry is a way of its own from
//! one end to the other. Where walking there is the longer way round - or there is no way round -
//! an NPC walks to the end the ferry leaves from and waits for it there, gets on while it waits,
//! rides it across, gets off at the other end while it waits there, and walks on (see [`Trip`]).

use crate::{
    grid::{WalkGrid, MAX_CLIMB},
    route::route_to,
};
use nalgebra::Vector3;

/// A ferry as NPCs go by it: the game tells them where each is, every frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Crossing {
    /// The middle of its deck where it waits at each end.
    pub ends: [Vector3<f32>; 2],
    /// The middle of its deck now, and how fast it is going.
    pub deck: Vector3<f32>,
    pub velocity: Vector3<f32>,
    /// Half as wide across as its deck is, the narrower way, in meters: within that of the
    /// middle, someone is on it.
    pub half: f32,
    /// The end it is waiting at, if it is, and for how much longer, in seconds.
    pub waiting: Option<(usize, f32)>,
    /// How long it takes to get from one end to the other, at worst: waiting for it to come back
    /// from the other end, and the crossing.
    pub longest: f32,
}

impl Crossing {
    /// Whether `at` is on its deck.
    pub fn carries(&self, at: Vector3<f32>) -> bool {
        let off = at - self.deck;
        off.x.abs() < self.half && off.z.abs() < self.half && off.y.abs() < 0.5
    }
}

/// How far from a point, at most, a floor at its height is looked for, in meters.
pub const NEAR: f32 = 6.0;
/// How far above or below a point a floor can be and still be at its height, in meters.
const LEVEL: f32 = 1.0;
/// How fast an NPC gets about, for weighing a walk against a ferry, in meters per second.
const PACE: f32 = 2.0;
/// How much quicker the ferry has to be than walking for an NPC to take it, in seconds: waiting
/// about at the dock, it could be on its way.
const BETTER_BY: f32 = 5.0;
/// How near the dock an NPC has to be to wait there, and how near the middle of the deck to be
/// aboard, in meters.
const AT_DOCK: f32 = 1.2;
const ABOARD: f32 = 0.9;
/// How fast an NPC gets on and off a ferry, at least, in meters per second - it hurries - and how
/// long the ferry has to go on waiting for it, on top of the time that takes, for it to set off
/// to get on, in seconds.
const BOARDING_PACE: f32 = 1.0;
const TIME_TO_SPARE: f32 = 1.0;

/// The walkable floor nearest `point`, within [`NEAR`], at its height: in its own cell if that
/// floor is near its height, or else another's. None with no floor at its height near it.
pub fn floor_near(grid: &WalkGrid, origin: Vector3<f32>, point: Vector3<f32>) -> Option<Vector3<f32>> {
    let at_height = |(x, z): (usize, usize)| grid.is_walkable(x, z) && (grid.floor(x, z) - point.y).abs() <= LEVEL;
    if let Some(cell) = grid.cell_at(origin, point).filter(|&cell| at_height(cell)) {
        return Some(Vector3::new(point.x, grid.floor(cell.0, cell.1), point.z));
    }
    // Over each spot of the plan near it, the floor at its height, on whichever storey.
    let at_height_over = |plan: (usize, usize)| grid.cells_over(plan).find(|&cell| at_height(cell));
    let reach = (NEAR / grid.cell_size).ceil() as isize;
    let (cx, cz) = cell_near(grid, origin, point);
    let mut best: Option<((usize, usize), f32)> = None;
    for dz in -reach..=reach {
        for dx in -reach..=reach {
            let (Some(x), Some(z)) = (cx.checked_add_signed(dx), cz.checked_add_signed(dz)) else {
                continue;
            };
            if x >= grid.width || z >= grid.depth {
                continue;
            }
            let Some(cell) = at_height_over((x, z)) else {
                continue;
            };
            let off = grid.center(origin, cell) - point;
            let distance = (off.x * off.x + off.z * off.z).sqrt();
            if distance <= NEAR && best.is_none_or(|(_, d)| distance < d) {
                best = Some((cell, distance));
            }
        }
    }
    best.map(|(cell, _)| grid.on_floor(origin, cell))
}

/// The cell `point` is over, or the one at the grid's edge nearest it.
fn cell_near(grid: &WalkGrid, origin: Vector3<f32>, point: Vector3<f32>) -> (usize, usize) {
    let index = |at: f32, from: f32, count: usize| ((at - from) / grid.cell_size).floor().clamp(0.0, (count - 1) as f32) as usize;
    (index(point.x, origin.x, grid.width), index(point.z, origin.z, grid.depth))
}

/// Where along the way an NPC taking a ferry is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    /// Walking to the end it leaves from.
    ToDock,
    /// Waiting there for it.
    Waiting,
    /// Getting on it, while it waits.
    Boarding,
    /// On it.
    Riding,
    /// Getting off it at the other end, while it waits.
    Leaving,
}

/// An NPC's way across by ferry.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Trip {
    /// Which ferry, and which end it gets on at.
    pub ferry: usize,
    pub from: usize,
    pub stage: Stage,
    /// Where it waits for the ferry at each end, on the floor beside where the ferry waits.
    docks: [Vector3<f32>; 2],
    /// Where it is going, once across.
    pub to: Vector3<f32>,
}

/// Where an NPC waits at each end of `crossing`: the floor beside where it waits, at its height.
fn docks(grid: &WalkGrid, origin: Vector3<f32>, crossing: &Crossing) -> Option<[Vector3<f32>; 2]> {
    Some([floor_near(grid, origin, crossing.ends[0])?, floor_near(grid, origin, crossing.ends[1])?])
}

/// The way from `feet` to `to`: over the grid, as [`route_to`] has it, within `reach` - put on the
/// floor at `to`'s height - or across on one of the `crossings`, when that is quicker: then the
/// route to where it leaves from, and the trip.
pub fn go_to(
    (grid, origin): (&WalkGrid, Vector3<f32>),
    feet: Vector3<f32>,
    to: Vector3<f32>,
    reach: f32,
    crossings: &[Crossing],
) -> (Vec<Vector3<f32>>, Option<Trip>) {
    let to = floor_near(grid, origin, to).map_or(to, |floor| Vector3::new(to.x, floor.y, to.z));
    let walk = route_to((grid, origin), feet, to, reach);
    if crossings.is_empty() {
        return (walk, None);
    }
    let (Some(start), Some(goal)) = (grid.walkable_cell(origin, feet), cell_of(grid, origin, to)) else {
        return (walk, None);
    };
    let (from_here, from_there) = (grid.distances_from(start), grid.distances_from(goal));
    let seconds = |distances: &[Option<u32>], at: Vector3<f32>| {
        let (x, z) = cell_of(grid, origin, at)?;
        distances[z * grid.width + x].map(|cells| cells as f32 * grid.cell_size / PACE)
    };
    let walking = from_here[goal.1 * grid.width + goal.0].map(|cells| cells as f32 * grid.cell_size / PACE);
    let mut best: Option<(f32, Trip)> = None;
    for (ferry, crossing) in crossings.iter().enumerate() {
        let Some(docks) = docks(grid, origin, crossing) else {
            continue;
        };
        for from in 0..2 {
            let (Some(there), Some(on)) = (seconds(&from_here, docks[from]), seconds(&from_there, docks[1 - from])) else {
                continue;
            };
            // Half the longest it could be, on average.
            let time = there + 0.5 * crossing.longest + on;
            if best.is_none_or(|(quickest, _)| time < quickest) {
                best = Some((time, Trip { ferry, from, stage: Stage::ToDock, docks, to }));
            }
        }
    }
    match best {
        Some((time, trip)) if walking.is_none_or(|walking| time + BETTER_BY < walking) => {
            let route = route_to((grid, origin), feet, trip.docks[trip.from], reach);
            if route.is_empty() && flat_distance(feet - trip.docks[trip.from]) > AT_DOCK {
                (walk, None)
            } else {
                (route, Some(trip))
            }
        }
        _ => (walk, None),
    }
}

/// The walkable cell at `at`, at its height.
fn cell_of(grid: &WalkGrid, origin: Vector3<f32>, at: Vector3<f32>) -> Option<(usize, usize)> {
    grid.walkable_cell(origin, floor_near(grid, origin, at).unwrap_or(at))
}

fn flat_distance(v: Vector3<f32>) -> f32 {
    (v.x * v.x + v.z * v.z).sqrt()
}

/// What an NPC on a trip does this frame.
#[derive(Debug, Clone, PartialEq)]
pub enum Doing {
    /// Goes where the route says, the next point last: walking on or off, or nowhere.
    Go(Vec<Vector3<f32>>),
    /// Is across, and goes on its way to [`Trip::to`].
    Across,
    /// Gives up on the ferry, which is not there any more.
    GiveUp,
}

impl Trip {
    /// Moves the trip on for an NPC at `feet` with the ferry as `crossings` has it: whether it is
    /// at the dock to wait, the ferry is waiting for it to get on, it is on, the ferry is waiting
    /// at the other end, it is off. `route` is the route it has, to the dock while it is on its
    /// way there.
    pub fn step(&mut self, feet: Vector3<f32>, route: &[Vector3<f32>], crossings: &[Crossing]) -> Doing {
        let Some(crossing) = crossings.get(self.ferry) else {
            return Doing::GiveUp;
        };
        let other = 1 - self.from;
        let waiting_at = |end: usize| crossing.waiting.filter(|&(at, _)| at == end).map(|(_, left)| left);
        let aboard = crossing.carries(feet);
        match self.stage {
            Stage::ToDock => {
                if flat_distance(feet - self.docks[self.from]) < AT_DOCK {
                    self.stage = Stage::Waiting;
                    Doing::Go(Vec::new())
                } else if route.is_empty() {
                    Doing::GiveUp
                } else {
                    Doing::Go(route.to_vec())
                }
            }
            Stage::Waiting => {
                let boarding = flat_distance(crossing.deck - feet) / BOARDING_PACE + TIME_TO_SPARE;
                if waiting_at(self.from).is_some_and(|left| left > boarding) {
                    self.stage = Stage::Boarding;
                    Doing::Go(vec![crossing.deck])
                } else {
                    Doing::Go(Vec::new())
                }
            }
            Stage::Boarding => {
                if aboard && flat_distance(feet - crossing.deck) < ABOARD {
                    self.stage = Stage::Riding;
                    Doing::Go(Vec::new())
                } else if waiting_at(self.from).is_none() && !aboard {
                    // It went without it: back to waiting for the next one.
                    self.stage = Stage::Waiting;
                    Doing::Go(vec![self.docks[self.from]])
                } else {
                    Doing::Go(vec![crossing.deck])
                }
            }
            Stage::Riding => {
                if waiting_at(other).is_some() {
                    self.stage = Stage::Leaving;
                    Doing::Go(vec![self.docks[other]])
                } else {
                    Doing::Go(Vec::new())
                }
            }
            Stage::Leaving => {
                if !aboard && flat_distance(feet - self.docks[other]) < AT_DOCK {
                    Doing::Across
                } else if aboard && waiting_at(other).is_none() {
                    // Not off in time: round again.
                    self.stage = Stage::Riding;
                    Doing::Go(Vec::new())
                } else {
                    Doing::Go(vec![self.docks[other]])
                }
            }
        }
    }

    /// Whether the NPC has to hurry: getting on the ferry or off it while it waits.
    pub fn hurrying(&self) -> bool {
        matches!(self.stage, Stage::Boarding | Stage::Leaving)
    }

    /// Whether the NPC is off the grid's floor for the ferry: getting on, on, or getting off.
    pub fn off_the_grid(&self) -> bool {
        matches!(self.stage, Stage::Boarding | Stage::Riding | Stage::Leaving)
    }
}

/// Whether a step from `from` to `to` keeps to floor that can be walked: onto a walkable cell, a
/// stair's step up or down at most from the one it is leaving. Off the grid, any step back onto it.
pub fn can_step(grid: &WalkGrid, origin: Vector3<f32>, from: Vector3<f32>, to: Vector3<f32>) -> bool {
    let walkable = |at: Vector3<f32>| grid.cell_at(origin, at).filter(|&(x, z)| grid.is_walkable(x, z));
    match (walkable(from), walkable(to)) {
        (_, None) => false,
        (None, Some(_)) => true,
        (Some(here), Some(there)) => here == there || (grid.floor(there.0, there.1) - grid.floor(here.0, here.1)).abs() <= MAX_CLIMB,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two floors: the ground everywhere, and a walkway 3 m up along the middle, over it.
    fn two_floors() -> WalkGrid {
        let mut grid = WalkGrid::new(10, 10, 1.0);
        for z in 0..10 {
            for x in 0..10 {
                grid.set(x, z, true);
                grid.set_floor(x, z, if x == 5 { 3.0 } else { 0.0 });
            }
        }
        grid
    }

    #[test]
    fn a_point_goes_on_the_floor_at_its_height() {
        let grid = two_floors();
        // Under the walkway, on the ground: the ground beside it, not the walkway over it.
        let under = floor_near(&grid, Vector3::zeros(), Vector3::new(5.5, 0.0, 5.5)).unwrap();
        assert_eq!(under.y, 0.0);
        assert!((under.x - 5.5).abs() <= 1.0 && under.x != 5.5, "{under:?}");
        // On the walkway, the walkway.
        let on = floor_near(&grid, Vector3::zeros(), Vector3::new(5.5, 3.0, 5.5)).unwrap();
        assert_eq!(on, Vector3::new(5.5, 3.0, 5.5));
        // Nothing at all at 10 m.
        assert!(floor_near(&grid, Vector3::zeros(), Vector3::new(5.5, 10.0, 5.5)).is_none());
    }

    #[test]
    fn a_step_keeps_to_the_floor_it_can_walk() {
        let grid = two_floors();
        let at = |x: f32| Vector3::new(x, 0.0, 5.5);
        assert!(can_step(&grid, Vector3::zeros(), at(3.5), at(4.5)), "along the ground");
        assert!(!can_step(&grid, Vector3::zeros(), at(4.5), at(5.5)), "not up onto the walkway");
        assert!(!can_step(&grid, Vector3::zeros(), at(9.5), at(10.5)), "not off the edge");
        assert!(can_step(&grid, Vector3::zeros(), at(-0.5), at(0.5)), "back onto it");
    }

    /// Two platforms, 3 m apart across the void, both on the ground, with a ferry between their
    /// ends: one end at x 2.5 off the first, the other at x 7.5 off the second.
    fn islands() -> (WalkGrid, Crossing) {
        let mut grid = WalkGrid::new(10, 3, 1.0);
        for z in 0..3 {
            for x in (0..2).chain(8..10) {
                grid.set(x, z, true);
            }
        }
        let crossing = Crossing {
            ends: [Vector3::new(3.0, 0.0, 1.5), Vector3::new(7.0, 0.0, 1.5)],
            deck: Vector3::new(3.0, 0.0, 1.5),
            velocity: Vector3::zeros(),
            half: 0.9,
            waiting: Some((0, 5.0)),
            longest: 20.0,
        };
        (grid, crossing)
    }

    #[test]
    fn with_no_way_round_it_takes_the_ferry() {
        let (grid, crossing) = islands();
        let (route, trip) = go_to((&grid, Vector3::zeros()), Vector3::new(0.5, 0.0, 1.5), Vector3::new(9.5, 0.0, 1.5), f32::INFINITY, &[crossing]);
        let trip = trip.expect("by ferry");
        assert_eq!((trip.from, trip.stage), (0, Stage::ToDock));
        assert!(route.first().is_some_and(|end| end.x < 2.0), "to the dock: {route:?}");
        // And not for anywhere it can walk to.
        let (_, trip) = go_to((&grid, Vector3::zeros()), Vector3::new(0.5, 0.0, 1.5), Vector3::new(1.5, 0.0, 0.5), f32::INFINITY, &[crossing]);
        assert!(trip.is_none());
    }

    #[test]
    fn it_waits_gets_on_rides_and_gets_off() {
        let (grid, mut crossing) = islands();
        let (_, trip) = go_to((&grid, Vector3::zeros()), Vector3::new(0.5, 0.0, 1.5), Vector3::new(9.5, 0.0, 1.5), f32::INFINITY, &[crossing]);
        let mut trip = trip.unwrap();
        // At the dock, the ferry about to go: it waits.
        let dock = Vector3::new(1.5, 0.0, 1.5);
        crossing.waiting = Some((0, 1.0));
        assert_eq!(trip.step(dock, &[], &[crossing]), Doing::Go(Vec::new()));
        assert_eq!(trip.stage, Stage::Waiting);
        // The ferry waiting a while: on it goes.
        crossing.waiting = Some((0, 4.0));
        assert_eq!(trip.step(dock, &[], &[crossing]), Doing::Go(vec![crossing.deck]));
        assert_eq!(trip.step(crossing.deck, &[], &[crossing]), Doing::Go(Vec::new()));
        assert_eq!(trip.stage, Stage::Riding);
        // Under way, it stays on; at the other end, it gets off.
        crossing.waiting = None;
        crossing.deck = Vector3::new(5.0, 0.0, 1.5);
        assert_eq!(trip.step(crossing.deck, &[], &[crossing]), Doing::Go(Vec::new()));
        crossing.waiting = Some((1, 4.0));
        crossing.deck = crossing.ends[1];
        assert!(matches!(trip.step(crossing.deck, &[], &[crossing]), Doing::Go(route) if route.len() == 1 && route[0].x > 8.0));
        assert_eq!(trip.stage, Stage::Leaving);
        assert_eq!(trip.step(Vector3::new(8.5, 0.0, 1.5), &[], &[crossing]), Doing::Across);
    }
}
