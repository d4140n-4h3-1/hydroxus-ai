//! Getting about over more than one floor: NPCs' ways to where they are going, at the height
//! it is at, and across by ferry where that is quicker than walking.
//!
//! A point an NPC is going to is put on the floor nearest it at its own height (see
//! [`floor_near`]): the sunken plaza for someone down in it, not the catwalk over it.
//!
//! Ferries - platforms that carry whoever stands on them between two places, waiting at each end
//! a while - are no part of the navmesh (see [`Crossing`]): a ferry is a way of its own from
//! one end to the other. Where walking there is the longer way round - or there is no way round -
//! an NPC walks to the end the ferry leaves from and waits for it there, gets on while it waits,
//! rides it across, gets off at the other end while it waits there, and walks on (see [`Trip`]).

use crate::navigation::{
    navmesh::NavMesh,
    walk::{self, route_to},
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

pub use walk::NEAR;
/// How fast an NPC gets about, for weighing a walk against a ferry, in meters per second.
pub const PACE: f32 = 2.0;
/// How much quicker the ferry has to be than walking for an NPC to take it, in seconds: waiting
/// about at the dock, it could be on its way.
pub const BETTER_BY: f32 = 5.0;
/// How near the dock an NPC has to be to wait there, and how near the middle of the deck to be
/// aboard, in meters.
pub const AT_DOCK: f32 = 1.2;
pub const ABOARD: f32 = 0.9;
/// How fast an NPC gets on and off a ferry, at least, in meters per second - it hurries - and how
/// long the ferry has to go on waiting for it, on top of the time that takes, for it to set off
/// to get on, in seconds.
pub const BOARDING_PACE: f32 = 1.0;
pub const TIME_TO_SPARE: f32 = 1.0;

tuning!(
    /// How an NPC weighs and takes a ferry (see the module's constants, its defaults).
    {
        /// How fast it gets about, in meters per second, and how much quicker, in seconds, the
        /// ferry has to be than walking.
        pace: f32 = PACE,
        better_by: f32 = BETTER_BY,
        /// How near the dock it waits, and how near the middle of the deck is aboard, in meters.
        at_dock: f32 = AT_DOCK,
        aboard: f32 = ABOARD,
        /// How fast it gets on and off, in meters per second, and how long the ferry has to go on
        /// waiting on top of that, in seconds.
        boarding_pace: f32 = BOARDING_PACE,
        time_to_spare: f32 = TIME_TO_SPARE,
    }
);

/// The walkable floor nearest `point`, within [`NEAR`], at its height: where it is if there is
/// floor there at its height, or else the nearest point that is. None with no floor at its height
/// near it.
pub fn floor_near(mesh: &NavMesh, point: Vector3<f32>) -> Option<Vector3<f32>> {
    walk::floor_near(mesh, point)
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
    /// How it goes about it.
    tuning: Tuning,
}

/// Where an NPC waits at each end of `crossing`: the floor beside where it waits, at its height.
fn docks(mesh: &NavMesh, crossing: &Crossing) -> Option<[Vector3<f32>; 2]> {
    Some([floor_near(mesh, crossing.ends[0])?, floor_near(mesh, crossing.ends[1])?])
}

/// The way from `feet` to `to`: over the navmesh, as [`route_to`] has it, within `reach` meters - put on the
/// floor at `to`'s height - or across on one of the `crossings`, when that is quicker: then the
/// route to where it leaves from, and the trip.
///
/// Every way is looked for heading for where it goes, never by searching out from `feet` to every
/// polygon: in a city that would cost the tenth part of a second each time, and a crowd works out
/// where it is going many times a second. A walk there about as short
/// as the way as the crow flies is taken without weighing the ferries at all, as none of them
/// could get there sooner by [`BETTER_BY`].
pub fn go_to(
    tuning: &Tuning,
    mesh: &NavMesh,
    feet: Vector3<f32>,
    to: Vector3<f32>,
    reach: f32,
    crossings: &[Crossing],
) -> (Vec<Vector3<f32>>, Option<Trip>) {
    let to = floor_near(mesh, to).map_or(to, |floor| Vector3::new(to.x, floor.y, to.z));
    let walk = route_to(mesh, feet, to, reach);
    if crossings.is_empty() {
        return (walk, None);
    }
    let (pace, better_by) = (tuning.pace, tuning.better_by);
    let walking = (!walk.is_empty()).then(|| length(feet, &walk) / pace);
    if walking.is_some_and(|walking| walking <= flat_distance(to - feet) / pace + better_by) {
        return (walk, None);
    }
    // How long a walk from `a` to `b` takes, in seconds, if there is one within `reach`.
    let leg = |a: Vector3<f32>, b: Vector3<f32>| {
        let route = route_to(mesh, a, b, reach);
        (!route.is_empty()).then(|| length(a, &route) / pace)
    };
    let mut best: Option<(f32, Trip)> = None;
    for (ferry, crossing) in crossings.iter().enumerate() {
        let Some(docks) = docks(mesh, crossing) else {
            continue;
        };
        for from in 0..2 {
            // Not worth the walks to find out, if the crossing alone is no quicker.
            let least = (flat_distance(docks[from] - feet) + flat_distance(to - docks[1 - from])) / pace + 0.5 * crossing.longest;
            if walking.is_some_and(|walking| least + better_by >= walking) || best.is_some_and(|(quickest, _)| least >= quickest) {
                continue;
            }
            let (Some(there), Some(on)) = (leg(feet, docks[from]), leg(docks[1 - from], to)) else {
                continue;
            };
            // Half the longest it could be, on average.
            let time = there + 0.5 * crossing.longest + on;
            if best.is_none_or(|(quickest, _)| time < quickest) {
                best = Some((time, Trip { ferry, from, stage: Stage::ToDock, docks, to, tuning: *tuning }));
            }
        }
    }
    match best {
        Some((time, trip)) if walking.is_none_or(|walking| time + better_by < walking) => {
            let route = route_to(mesh, feet, trip.docks[trip.from], reach);
            if route.is_empty() && flat_distance(feet - trip.docks[trip.from]) > tuning.at_dock {
                (walk, None)
            } else {
                (route, Some(trip))
            }
        }
        _ => (walk, None),
    }
}

/// How far it is from `feet` along `route`, kept the way it is followed - its first point last.
fn length(feet: Vector3<f32>, route: &[Vector3<f32>]) -> f32 {
    let mut at = feet;
    let mut walked = 0.0;
    for &next in route.iter().rev() {
        walked += flat_distance(next - at);
        at = next;
    }
    walked
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
                if flat_distance(feet - self.docks[self.from]) < self.tuning.at_dock {
                    self.stage = Stage::Waiting;
                    Doing::Go(Vec::new())
                } else if route.is_empty() {
                    Doing::GiveUp
                } else {
                    Doing::Go(route.to_vec())
                }
            }
            Stage::Waiting => {
                let boarding = flat_distance(crossing.deck - feet) / self.tuning.boarding_pace + self.tuning.time_to_spare;
                if waiting_at(self.from).is_some_and(|left| left > boarding) {
                    self.stage = Stage::Boarding;
                    Doing::Go(vec![crossing.deck])
                } else {
                    Doing::Go(Vec::new())
                }
            }
            Stage::Boarding => {
                if aboard && flat_distance(feet - crossing.deck) < self.tuning.aboard {
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
                if !aboard && flat_distance(feet - self.docks[other]) < self.tuning.at_dock {
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

/// Whether a step from `from` to `to` keeps to floor that can be walked: onto floor at its height,
/// a stair's step up or down at most from the floor it is leaving. Off the floor, any step back
/// onto it.
pub fn can_step(mesh: &NavMesh, from: Vector3<f32>, to: Vector3<f32>) -> bool {
    match (mesh.poly_at(from), mesh.poly_at(to)) {
        (_, None) => false,
        (None, Some(_)) => true,
        (Some(here), Some(there)) => here == there || (mesh.floor_of(there) - mesh.floor_of(here)).abs() <= mesh.ground.max_climb,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::navigation::grid::WalkGrid;

    fn mesh(grid: &WalkGrid) -> NavMesh {
        NavMesh::build(grid, Vector3::zeros(), |_, _| 0)
    }

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
        let under = floor_near(&mesh(&grid), Vector3::new(5.5, 0.0, 5.5)).unwrap();
        assert_eq!(under.y, 0.0);
        assert!((under.x - 5.5).abs() <= 1.0 && under.x != 5.5, "{under:?}");
        // On the walkway, the walkway.
        let on = floor_near(&mesh(&grid), Vector3::new(5.5, 3.0, 5.5)).unwrap();
        assert_eq!(on, Vector3::new(5.5, 3.0, 5.5));
        // Nothing at all at 10 m.
        assert!(floor_near(&mesh(&grid), Vector3::new(5.5, 10.0, 5.5)).is_none());
    }

    #[test]
    fn a_step_keeps_to_the_floor_it_can_walk() {
        let grid = two_floors();
        let at = |x: f32| Vector3::new(x, 0.0, 5.5);
        assert!(can_step(&mesh(&grid), at(3.5), at(4.5)), "along the ground");
        assert!(!can_step(&mesh(&grid), at(4.5), at(5.5)), "not up onto the walkway");
        assert!(!can_step(&mesh(&grid), at(9.5), at(10.5)), "not off the edge");
        assert!(can_step(&mesh(&grid), at(-0.5), at(0.5)), "back onto it");
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
        let (route, trip) = go_to(&Tuning::DEFAULT, &mesh(&grid), Vector3::new(0.5, 0.0, 1.5), Vector3::new(9.5, 0.0, 1.5), f32::INFINITY, &[crossing]);
        let trip = trip.expect("by ferry");
        assert_eq!((trip.from, trip.stage), (0, Stage::ToDock));
        assert!(route.first().is_some_and(|end| end.x < 2.0), "to the dock: {route:?}");
        // And not for anywhere it can walk to.
        let (_, trip) = go_to(&Tuning::DEFAULT, &mesh(&grid), Vector3::new(0.5, 0.0, 1.5), Vector3::new(1.5, 0.0, 0.5), f32::INFINITY, &[crossing]);
        assert!(trip.is_none());
    }

    #[test]
    fn the_long_way_round_or_the_ferry_whichever_is_quicker() {
        // Two banks of a long lake, joined round its far end; a ferry across its middle.
        let mut grid = WalkGrid::new(60, 21, 1.0);
        for z in 0..21 {
            for x in 0..60 {
                grid.set(x, z, z < 8 || z > 12 || x > 55);
            }
        }
        let crossing = Crossing {
            ends: [Vector3::new(10.5, 0.0, 8.5), Vector3::new(10.5, 0.0, 12.5)],
            deck: Vector3::new(10.5, 0.0, 8.5),
            velocity: Vector3::zeros(),
            half: 0.9,
            waiting: Some((0, 5.0)),
            longest: 10.0,
        };
        let at = |x: f32, z: f32| Vector3::new(x, 0.0, z);
        // Across from near the ferry: by ferry, rather than a hundred meters round the end.
        let (route, trip) = go_to(&Tuning::DEFAULT, &mesh(&grid), at(10.5, 3.5), at(10.5, 17.5), f32::INFINITY, &[crossing]);
        assert!(trip.is_some_and(|trip| trip.from == 0), "by ferry: {route:?}");
        // Along its own bank: walking, the ferries not weighed.
        let (route, trip) = go_to(&Tuning::DEFAULT, &mesh(&grid), at(10.5, 3.5), at(40.5, 3.5), f32::INFINITY, &[crossing]);
        assert!(trip.is_none() && !route.is_empty());
        // Across near the end, where the walk round is short: walking.
        let (route, trip) = go_to(&Tuning::DEFAULT, &mesh(&grid), at(52.5, 3.5), at(52.5, 17.5), f32::INFINITY, &[crossing]);
        assert!(trip.is_none() && !route.is_empty(), "round the end");
    }

    #[test]
    fn it_waits_gets_on_rides_and_gets_off() {
        let (grid, mut crossing) = islands();
        let (_, trip) = go_to(&Tuning::DEFAULT, &mesh(&grid), Vector3::new(0.5, 0.0, 1.5), Vector3::new(9.5, 0.0, 1.5), f32::INFINITY, &[crossing]);
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
