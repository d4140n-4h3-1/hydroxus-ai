//! Taking a train. Now and then an NPC near one of the railway's stations takes a train to
//! another, a few at a time: it climbs to the platform, waits beside a doorway no one else is
//! waiting at, and gets on once the doors are open - only with time to; stands inside, looking
//! out, as it rides; and gets off at the station it is going to. Too late for the doors, it waits
//! for the train to come round again; kept waiting too long, it gives up.
//!
//! The game says where its trains are and when their doors are open (see [`Train`]); this says
//! what a rider does about it.

use crate::{
    flat,
    navigation::{
        grid::Rng,
        navmesh::NavMesh,
        route::between,
        walk::route_to,
        trip,
    },
};
use nalgebra::Vector3;

/// A train as riders go by it: where it is, where its doorways are on it, and whether its doors
/// are open to get on and off by. Its positions are in the world's terms.
pub trait Train {
    /// The middle of the beam's top under it, or wherever its doorways are measured from.
    fn at(&self) -> Vector3<f32>;
    /// The middle of each doorway, on the floor in it, from [`Train::at`].
    fn doorways(&self) -> &[Vector3<f32>];
    /// Which way is across it, to its right-hand side; and which way along it.
    fn across(&self) -> Vector3<f32>;
    fn along(&self) -> Vector3<f32>;
    /// While its doors are wide open, at which station, and for how much longer, in seconds.
    fn open(&self) -> Option<(usize, f32)>;
    /// How many stations it stops at, and where it stands at the `n`th, its doors open.
    fn stations(&self) -> usize;
    fn station(&self, n: usize) -> Vector3<f32>;
    /// Whether `at` is inside it: aboard.
    fn within(&self, at: Vector3<f32>) -> bool;
    /// How far out to its right-hand side `at` is, in meters - to its left, less than nothing.
    fn out(&self, at: Vector3<f32>) -> f32;
    /// Where on the platform at `station`, `out` meters out from its `n`th doorway, a rider waits.
    fn platform(&self, station: usize, n: usize, out: f32) -> Vector3<f32>;
}

/// How near where it stands aboard a rider getting on has to be to be on, in meters.
const ON_SPOT: f32 = 0.2;

/// How near a station an NPC has to be, setting off somewhere, to think of it,
/// and how likely it is to then, one in so many; how many can be taking each train at once; and how
/// long one waits on the platform for it at most, in seconds.
pub const TRAIN_REACH: f32 = 150.0;
pub const TRAIN_CHANCE: usize = 3;
pub const RIDERS: usize = 8;
pub const TRAIN_WAIT: f32 = 200.0;
/// How far out on the platform from a doorway it waits, and how far in from it it stands aboard,
/// in meters; and how near where it waits it has to be to be there.
pub const PLATFORM_OUT: f32 = 1.1;
pub const STAND_IN: f32 = 0.8;
pub const AT_PLATFORM: f32 = 1.2;
/// How far out from the middle of the train one getting off has to be, in meters, to be clear of
/// its doorway: off the sill, its side past the car's.
pub const CLEAR_OF_DOORS: f32 = 3.0;
/// How fast it gets on and off, at least, in meters per second, and how long the doors have to
/// stay open for it to set off to, on top of the time that takes, in seconds.
pub const BOARDING_PACE: f32 = 1.0;
pub const TIME_TO_SPARE: f32 = 1.0;

/// A droid's ride on a train: from one station to another.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ride {
    /// Which train it takes, as the game counts them.
    pub train: usize,
    /// Which stations it gets on at and off at, as the train counts them.
    pub from: usize,
    pub to: usize,
    /// Which of the train's doorways it gets on and off by, and where it stands in the car, from
    /// the middle of the beam's top under the train.
    pub doorway: usize,
    pub spot: Vector3<f32>,
    /// Where it waits on the platform it gets on from, and where it goes on the one it gets off on.
    pub wait: Vector3<f32>,
    pub off: Vector3<f32>,
    pub stage: Riding,
    /// How long it has been waiting on the platform, in seconds.
    pub waited: f32,
}

/// Where along the way a droid taking the train is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Riding {
    /// On its way up to the platform.
    ToPlatform,
    /// Waiting there for the doors to open.
    Waiting,
    /// Getting on, the doors open.
    Boarding,
    /// On, standing where it stands.
    Aboard,
    /// Getting off, at the station it is going to.
    Leaving,
}

/// What a droid taking the train does this frame.
#[derive(Debug, Clone, PartialEq)]
pub enum Rides {
    /// Goes its own way, on to the platform.
    Own,
    /// Goes where the route says, the next point last: getting on or off, or nowhere.
    Go(Vec<Vector3<f32>>),
    /// Is done with the ride, or gives it up.
    Off,
}

impl Ride {
    /// Whether it is off the grid's floor for the train: getting on, on, or getting off.
    pub fn aboard(&self) -> bool {
        matches!(self.stage, Riding::Boarding | Riding::Aboard | Riding::Leaving)
    }

    /// Moves the ride on for a droid at `feet` with `route` ahead of it - on its way to the
    /// platform - the train as `train` has it, after `dt`.
    pub fn step(&mut self, feet: Vector3<f32>, route: &[Vector3<f32>], train: &impl Train, dt: f32) -> Rides {
        let doorway = train.at() + train.doorways()[self.doorway];
        let spot = train.at() + self.spot;
        let within = train.within(feet);
        // Whether the doors are open at `station`, and stay open long enough for it to get to `to`.
        let in_time = |station: usize, to: Vector3<f32>| {
            let takes = flat(to - feet).norm() / BOARDING_PACE + TIME_TO_SPARE;
            train.open().is_some_and(|(at, left)| at == station && left > takes)
        };
        let shut = train.open().is_none();
        let waiting = flat(self.wait - feet).norm() < AT_PLATFORM;
        // Kept aboard, the doors shut: it stands where it is till they open again.
        let stay = |ride: &mut Self| {
            ride.spot = Vector3::new(feet.x - train.at().x, train.doorways()[ride.doorway].y, feet.z - train.at().z);
            ride.stage = Riding::Aboard;
            Rides::Go(Vec::new())
        };
        match self.stage {
            Riding::ToPlatform if waiting => {
                self.stage = Riding::Waiting;
                Rides::Go(Vec::new())
            }
            Riding::ToPlatform if route.is_empty() => Rides::Off,
            Riding::ToPlatform => Rides::Own,
            Riding::Waiting => {
                self.waited += dt;
                if self.waited > TRAIN_WAIT {
                    Rides::Off
                } else if in_time(self.from, spot) {
                    self.stage = Riding::Boarding;
                    Rides::Go(vec![spot, doorway])
                } else {
                    Rides::Go(if waiting { Vec::new() } else { vec![self.wait] })
                }
            }
            Riding::Boarding if within && flat(spot - feet).norm() < ON_SPOT => {
                self.stage = Riding::Aboard;
                Rides::Go(Vec::new())
            }
            Riding::Boarding if shut && within => stay(self),
            Riding::Boarding if shut => {
                // Too late: back to waiting for it to come round again.
                self.stage = Riding::Waiting;
                Rides::Go(vec![self.wait])
            }
            Riding::Boarding => Rides::Go(if within { vec![spot] } else { vec![spot, doorway] }),
            Riding::Aboard if in_time(self.to, self.off) => {
                self.stage = Riding::Leaving;
                Rides::Go(vec![self.off, doorway])
            }
            Riding::Aboard => Rides::Go(Vec::new()),
            Riding::Leaving
                if train.out(feet).abs() > CLEAR_OF_DOORS && flat(self.off - feet).norm() < AT_PLATFORM =>
            {
                Rides::Off
            }
            // Not off in time: on round again.
            Riding::Leaving if within && shut => stay(self),
            Riding::Leaving => Rides::Go(if within { vec![self.off, doorway] } else { vec![self.off] }),
        }
    }
}

/// A ride on one of `trains` for an NPC at `feet`, over `mesh`: on the
/// train with a station nearest it, near enough to think of, that has room - fewer than [`RIDERS`]
/// and a doorway no one is taking it by, those `taken` as (train, doorway) - from that station to
/// another of its stations: the way up to the platform beside the doorway - looked for no further
/// than `reach` meters - and the ride.
pub fn take_train<T: Train>(
    mesh: &NavMesh,
    feet: Vector3<f32>,
    trains: &[T],
    taken: &[(usize, usize)],
    reach: f32,
    rng: &mut Rng,
) -> Option<(Vec<Vector3<f32>>, Ride)> {
    let away = |train: &T, n: usize| flat(train.station(n) - feet).norm();
    let room = |k: usize| {
        let riding = taken.iter().filter(|&&(t, _)| t == k).count();
        riding < RIDERS && (0..trains[k].doorways().len()).any(|n| !taken.contains(&(k, n))) && trains[k].stations() >= 2
    };
    // Each train's station nearest it, nearest first; the first of those with room.
    let mut near: Vec<(usize, usize, f32)> = (0..trains.len())
        .filter_map(|k| {
            let train = &trains[k];
            let n = (0..train.stations()).min_by(|&a, &b| away(train, a).total_cmp(&away(train, b)))?;
            Some((k, n, away(train, n)))
        })
        .filter(|&(_, _, d)| d <= TRAIN_REACH)
        .collect();
    near.sort_by(|a, b| a.2.total_cmp(&b.2));
    let (which, from, _) = near.into_iter().find(|&(k, _, _)| room(k))?;
    let train = &trains[which];
    let free: Vec<usize> = (0..train.doorways().len()).filter(|&n| !taken.contains(&(which, n))).collect();
    let to = (from + 1 + rng.below(train.stations() - 1)) % train.stations();
    let doorway = free[rng.below(free.len())];
    let wait = trip::floor_near(mesh, train.platform(from, doorway, PLATFORM_OUT))?;
    let off = trip::floor_near(mesh, train.platform(to, doorway, PLATFORM_OUT))?;
    let route = route_to(mesh, feet, wait, reach);
    if route.is_empty() {
        return None;
    }
    let inward = -train.doorways()[doorway].dot(&train.across()).signum();
    let spot = train.doorways()[doorway] + train.across() * inward * STAND_IN + train.along() * between(rng, (-0.5, 0.5));
    Some((route, Ride { train: which, from, to, doorway, spot, wait, off, stage: Riding::ToPlatform, waited: 0.0 }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// How near a point of its way one walking has to be to make for the next, and how fast it
    /// walks, as the game's droids do.
    const REACHED: f32 = 0.6;
    const FALLBACK_PACE: f32 = 1.4;

    /// A train along +z, stopping every 300 m, its platforms 1.5 m under where its doorways are
    /// measured from.
    struct TestTrain {
        at: Vector3<f32>,
        doorways: Vec<Vector3<f32>>,
        across: Vector3<f32>,
        along: Vector3<f32>,
        open: Option<(usize, f32)>,
    }

    impl Train for TestTrain {
        fn at(&self) -> Vector3<f32> {
            self.at
        }
        fn doorways(&self) -> &[Vector3<f32>] {
            &self.doorways
        }
        fn across(&self) -> Vector3<f32> {
            self.across
        }
        fn along(&self) -> Vector3<f32> {
            self.along
        }
        fn open(&self) -> Option<(usize, f32)> {
            self.open
        }
        fn stations(&self) -> usize {
            3
        }
        fn station(&self, n: usize) -> Vector3<f32> {
            Vector3::new(0.0, 9.5, 300.0 * n as f32)
        }
        fn within(&self, at: Vector3<f32>) -> bool {
            let off = at - self.at;
            off.dot(&self.across).abs() < 2.45 && off.dot(&self.along).abs() < 19.0
        }
        fn out(&self, at: Vector3<f32>) -> f32 {
            (at - self.at).dot(&self.across)
        }
        fn platform(&self, station: usize, n: usize, out: f32) -> Vector3<f32> {
            let doorway = self.doorways[n];
            let side = doorway.dot(&self.across).signum();
            let mut at = self.station(station) + doorway + self.across * side * out;
            at.y = self.station(station).y - 1.5;
            at
        }
    }

    /// The train at the first station with one doorway, on its right, its doors open there for as
    /// long as `open_for` says; and a ride by it to the last station, waiting on the platform.
    fn at_the_station(open_for: Option<f32>) -> (TestTrain, Ride) {
        let mut train = TestTrain {
            at: Vector3::zeros(),
            doorways: vec![Vector3::new(2.53, -1.25, 2.75)],
            across: Vector3::x(),
            along: Vector3::z(),
            open: None,
        };
        train.at = train.station(0);
        train.open = open_for.map(|left| (0, left));
        let to = train.stations() - 1;
        let (wait, off) = (train.platform(0, 0, PLATFORM_OUT), train.platform(to, 0, PLATFORM_OUT));
        let spot = train.doorways[0] + Vector3::new(-STAND_IN, 0.0, 0.0);
        (train, Ride { train: 0, from: 0, to, doorway: 0, spot, wait, off, stage: Riding::Waiting, waited: 0.0 })
    }

    /// Moves the ride on a step of `dt`, the droid walking where it says as it would: on to each
    /// point of the way, past it once near; aboard, where it stands.
    fn ride_a_step(ride: &mut Ride, train: &TestTrain, feet: &mut Vector3<f32>, dt: f32) -> Rides {
        let rides = ride.step(*feet, &[], train, dt);
        if let Rides::Go(route) = &rides {
            let mut route = route.clone();
            while route.len() > 1 && flat(route[route.len() - 1] - *feet).norm() < REACHED {
                route.pop();
            }
            if let Some(&next) = route.last() {
                let to = flat(next - *feet);
                if to.norm() > 1.0e-4 {
                    *feet += to.normalize() * (FALLBACK_PACE * dt).min(to.norm());
                }
            }
        }
        if ride.stage == Riding::Aboard {
            *feet = train.at + ride.spot;
        }
        rides
    }

    #[test]
    fn a_ride_on_the_train_gets_on_at_one_station_and_off_at_another() {
        let (mut train, mut ride) = at_the_station(None);
        let station = train.at;
        let mut feet = ride.wait;
        let dt = 0.1;
        // The doors shut, it waits.
        for _ in 0..20 {
            assert_eq!(ride_a_step(&mut ride, &train, &mut feet, dt), Rides::Go(Vec::new()));
        }
        assert_eq!(ride.stage, Riding::Waiting);
        // They open, it gets on.
        train.open = Some((0, 15.0));
        for _ in 0..100 {
            ride_a_step(&mut ride, &train, &mut feet, dt);
        }
        assert_eq!(ride.stage, Riding::Aboard);
        assert!(train.within(feet), "{feet:?}");
        // Off it goes, and it with it.
        train.open = None;
        let there = train.station(ride.to);
        for k in 0..=50 {
            train.at = station.lerp(&there, k as f32 / 50.0);
            ride_a_step(&mut ride, &train, &mut feet, dt);
            assert_eq!(feet, train.at + ride.spot);
        }
        // At a station on the way, it stays on.
        train.open = Some((1, 15.0));
        for _ in 0..20 {
            ride_a_step(&mut ride, &train, &mut feet, dt);
        }
        assert_eq!(ride.stage, Riding::Aboard);
        // At the one it is going to, the doors open: off it gets, onto its platform.
        train.open = Some((ride.to, 15.0));
        let mut off = false;
        for _ in 0..100 {
            if ride_a_step(&mut ride, &train, &mut feet, dt) == Rides::Off {
                off = true;
                break;
            }
        }
        assert!(off, "{:?} at {feet:?}", ride.stage);
        assert!((feet.x - train.at.x).abs() > CLEAR_OF_DOORS && flat(feet - ride.off).norm() < AT_PLATFORM);
    }

    #[test]
    fn a_ride_on_the_train_gets_on_only_with_time_to_and_waits_for_the_next_if_too_late() {
        // Too little time left to get on: it goes on waiting.
        let (train, mut ride) = at_the_station(Some(0.5));
        let mut feet = ride.wait;
        ride_a_step(&mut ride, &train, &mut feet, 0.1);
        assert_eq!(ride.stage, Riding::Waiting);
        // Shut on it before it is on: back to waiting.
        let (mut train, mut ride) = at_the_station(Some(10.0));
        let mut feet = ride.wait;
        ride_a_step(&mut ride, &train, &mut feet, 0.1);
        assert_eq!(ride.stage, Riding::Boarding);
        train.open = None;
        ride_a_step(&mut ride, &train, &mut feet, 0.1);
        assert_eq!(ride.stage, Riding::Waiting);
        // Kept waiting too long, it gives up.
        let (train, mut ride) = at_the_station(None);
        let mut feet = ride.wait;
        let steps = (TRAIN_WAIT / 1.0) as usize + 1;
        let gave_up = (0..steps).any(|_| ride_a_step(&mut ride, &train, &mut feet, 1.0) == Rides::Off);
        assert!(gave_up);
    }
}
