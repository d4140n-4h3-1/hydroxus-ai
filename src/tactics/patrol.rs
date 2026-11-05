//! Patrolling: a flying sentry - a security drone - going about a level, calm, until it sees the
//! player or is sent to look for them; then searching, and on Alert coming after them.
//!
//! - **Patrol**: it goes from one spot a trip away to the next, waiting a while at each - or, with
//!   a post to keep, round that - paying the player no heed unless it [`watches`](Patrol::watches)
//!   all the while.
//! - **Search**: it flies to where the player was and scans there, then to one spot after another
//!   nearby, scanning at each, for [`SEARCH`] seconds - its time running from when it gets there,
//!   not on its way - and then it is calm again.
//! - **Alert**: it can see them. It comes on to [`ENGAGE`] from them, keeps facing them, and is
//!   ready to fire ([`Patrol::fire_in`]) while they are within [`FIRE_RANGE`]. Losing them, it
//!   searches where it last saw them.
//! - **Down**: brought down, it thinks no more.
//!
//! Which of the three it is doing is chosen by utility scoring (see [`crate::tactics::utility`]):
//! its [`Patrol::reasoner`] weighs engaging, searching and patrolling against what it takes in
//! ([`Input`]) - whether it sees the player, how long it has left to search, how far off they
//! are. Its default ([`reasoner`]) chooses as the phases above say; a game can weigh them its own
//! way - a drone that leaves a player too far off alone, say - and see what each scored.
//!
//! The game says what it sees, finds its ways ([`Ways`]) and what is round it as it flies
//! ([`Air`]), and moves its body, plays its animations and fires; this decides where it goes, and
//! tells the game what came of it ([`Event`]) - for it to say, play or show as it likes. How far
//! and how long it does each is its [`Tuning`].

use crate::{
    flat,
    tactics::utility::{Action, Curve, Reasoner, Situation},
};
use nalgebra::Vector3;

/// How long it searches once it has lost the player, or been sent to look for them, in seconds.
pub const SEARCH: f32 = 30.0;
/// How far it goes on patrol each time it sets off, and how far between the spots it searches,
/// in meters of walking, from least to most; how far from its post it patrols, if it keeps one;
/// and how long it waits between trips, in seconds.
pub const TRIP: (f32, f32) = (15.0, 60.0);
pub const SEARCH_TRIP: (f32, f32) = (5.0, 20.0);
pub const POST_REACH: f32 = 9.0;
pub const REST: (f32, f32) = (3.0, 8.0);
/// On Alert: how near the player it comes, in meters, how far off it fires from, and how soon
/// after spotting them it first fires; and how often it works out its way to them again.
pub const ENGAGE: f32 = 6.0;
pub const FIRE_RANGE: f32 = 20.0;
pub const FIRST_SHOT: f32 = 0.8;
pub const REPLAN: f32 = 0.5;
/// How near the next point on its route it has to get, in meters, before making for the one after,
/// and how near the last counts as there.
pub const REACHED: f32 = 0.6;
pub const ARRIVED: f32 = 0.15;
/// How quickly it follows the floor up and down.
pub const FLOOR_EASING: f32 = 4.0;

tuning!(
    /// How a flying sentry goes about its patrol (see the module's constants, its defaults).
    {
        /// How long it searches, in seconds.
        search: f32 = SEARCH,
        /// How far each patrol trip goes, and how far between the spots it searches, in meters of
        /// walking, from least to most; how far from its post it patrols; and how long it rests
        /// between trips, in seconds.
        trip: (f32, f32) = TRIP,
        search_trip: (f32, f32) = SEARCH_TRIP,
        post_reach: f32 = POST_REACH,
        rest: (f32, f32) = REST,
        /// On Alert: how near it comes, how far off it fires from, in meters; how soon it first
        /// fires, and how often it works out its way again, in seconds.
        engage: f32 = ENGAGE,
        fire_range: f32 = FIRE_RANGE,
        first_shot: f32 = FIRST_SHOT,
        replan: f32 = REPLAN,
        /// How near a point of its route counts as reached, and the last as there, in meters.
        reached: f32 = REACHED,
        arrived: f32 = ARRIVED,
        /// How quickly it follows the floor up and down.
        floor_easing: f32 = FLOOR_EASING,
    }
);

/// What a sentry can choose to do: what [`State`] it goes about the player in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Mode {
    /// Come on to the player and fire: [`State::Alert`].
    Engage,
    /// Look for them: [`State::Search`].
    Search,
    /// Go about its patrol: [`State::Patrol`].
    Patrol,
}

/// What a sentry takes in, for its reasoner to weigh.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Input {
    /// 1 if it sees the player, 0 if not.
    Sees,
    /// How long it has left to search, in seconds: what it has left searching; all of it
    /// ([`Tuning::search`]), on Alert, were it to lose them; nothing, calm.
    SearchLeft,
    /// How far off the player is along the ground, in meters, if it sees them; 10 km if not.
    Distance,
    /// How many spots it has searched since it lost them.
    Searched,
    /// 1 if it keeps a post, 0 if not.
    Post,
}

/// A sentry's situation, as its reasoner reads it (see [`Input`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Reading {
    pub sees: bool,
    pub search_left: f32,
    pub distance: f32,
    pub searched: u32,
    pub post: bool,
}

impl Situation<Input> for Reading {
    fn input(&self, input: Input) -> f32 {
        match input {
            Input::Sees => f32::from(u8::from(self.sees)),
            Input::SearchLeft => self.search_left,
            Input::Distance => self.distance,
            Input::Searched => self.searched as f32,
            Input::Post => f32::from(u8::from(self.post)),
        }
    }
}

/// A sentry's reasoner as Metal Gear's phases have it: on Alert while it sees the player, searching
/// while it has time left to - having lost them, or been sent - and otherwise patrolling.
pub fn reasoner() -> Reasoner<Mode, Input> {
    Reasoner::new(
        vec![
            Action::new(Mode::Engage, 1.0).when(Input::Sees, Curve::Above { at: 0.5 }),
            Action::new(Mode::Search, 0.6).when(Input::SearchLeft, Curve::Above { at: 0.0 }),
            Action::new(Mode::Patrol, 0.2),
        ],
        0.1,
    )
}

/// What came of what a sentry did, for the game to say, play or show.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Event {
    /// It saw the player, and is on Alert.
    Spotted,
    /// It lost them and searches where it last saw them; or set off to where it was sent to
    /// search, or to where a shot came from.
    Searching,
    /// The alarm sent it, calm, to search.
    Alarmed,
    /// Its search ran out: it is calm again.
    Calm,
}

/// How it is going about the player.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum State {
    /// Going about its patrol, calm.
    Patrol,
    /// It can see the player, and fires at them.
    Alert,
    /// It is looking for the player, `left` seconds more.
    Search { left: f32 },
    /// Brought down.
    Down,
}

/// How fast it flies, in meters per second: patrolling, and hostile; how quickly it gets up to
/// speed, in meters per second per second; and how quickly it turns, in radians a second.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Speeds {
    pub patrol: f32,
    pub hurry: f32,
    pub acceleration: f32,
    pub turn_rate: f32,
}

/// Its ways about the level, as the game finds them: each a route, the next point last.
pub trait Ways {
    /// The way to its height over the floor at `to`, from where it is: through the air, flying
    /// free; otherwise along the floor. Empty if there is none.
    fn route_to(&mut self, to: Vector3<f32>) -> Vec<Vector3<f32>>;
    /// A trip from the floor `under` it to somewhere `reach` away, from least to most, in meters of
    /// walking. Empty, with nowhere found to go.
    fn trip(&mut self, under: Vector3<f32>, reach: (f32, f32)) -> Vec<Vector3<f32>>;
    /// Somewhere on the floor within `reach` of `post`, if anywhere.
    fn spot_near(&mut self, post: Vector3<f32>, reach: f32) -> Option<Vector3<f32>>;
    /// A number picked at random between `range`'s two.
    fn between(&mut self, range: (f32, f32)) -> f32;
}

/// What is round it as it flies, as the game finds it.
pub trait Air {
    /// Whether it could fly from `from` to `to` without touching anything solid.
    fn clear(&self, from: Vector3<f32>, to: Vector3<f32>) -> bool;
    /// The height of the floor under `at`, if there is floor there.
    fn floor_under(&self, at: Vector3<f32>) -> Option<f32>;
    /// Whether going `step` from `from` would take it into anyone: then it waits for them to move.
    fn blocked(&self, from: Vector3<f32>, step: Vector3<f32>) -> bool;
}

/// A flying sentry's mind: what it is doing, where it is, and where it is going.
#[derive(Debug, Clone, PartialEq)]
pub struct Patrol {
    pub state: State,
    /// Where it hovers, which way it faces, in radians, left positive from the world's +z, and how
    /// fast it is flying, in meters per second.
    pub at: Vector3<f32>,
    pub heading: f32,
    pub speed: f32,
    /// The rest of its route, the next point last: points on the floor, or in the air flying free.
    pub route: Vec<Vector3<f32>>,
    /// How long it has left to wait before its next trip, or to scan where it is, in seconds.
    pub resting: f32,
    pub scanning: f32,
    /// Where it last saw the player's feet, or was sent to look for them; and how many spots it has
    /// searched since.
    pub lost_at: Vector3<f32>,
    pub searched: u32,
    /// On Alert, how long until it fires, and until it works out its way to them again.
    pub fire_in: f32,
    pub replan: f32,
    /// Whether there was no way to the player the last time it worked one out: it waits for
    /// `replan` to try again, rather than trying every frame.
    pub no_way: bool,
    /// The post it patrols round, if it keeps one; and whether it watches for the player all the
    /// while, patrolling too, rather than only once it is hostile.
    pub post: Option<Vector3<f32>>,
    pub watches: bool,
    /// How high it hovers over the floor; whether it flies free, at any height, through the air,
    /// rather than along the floor; how fast it flies; and how long a scan takes, in seconds.
    pub hover: f32,
    pub flies_free: bool,
    pub speeds: Speeds,
    pub scan_for: f32,
    /// How far and how long it does each thing.
    pub tuning: Tuning,
    /// How it chooses what to do, and what each choice last scored (see [`reasoner`]).
    pub reasoner: Reasoner<Mode, Input>,
}

impl Patrol {
    /// A sentry hovering `hover` over the floor, flying `speeds`, free or not, a scan taking
    /// `scan_for` seconds: calm, nowhere yet.
    pub fn new(hover: f32, flies_free: bool, speeds: Speeds, scan_for: f32) -> Self {
        Self {
            state: State::Patrol,
            at: Vector3::zeros(),
            heading: 0.0,
            speed: 0.0,
            route: Vec::new(),
            resting: 0.0,
            scanning: 0.0,
            lost_at: Vector3::zeros(),
            searched: 0,
            fire_in: 0.0,
            replan: 0.0,
            no_way: false,
            post: None,
            watches: false,
            hover,
            flies_free,
            speeds,
            scan_for,
            tuning: Tuning::DEFAULT,
            reasoner: reasoner(),
        }
    }

    /// The floor under it.
    pub fn under(&self) -> Vector3<f32> {
        self.at - Vector3::new(0.0, self.hover, 0.0)
    }

    /// Put down afresh over `spot`, on the floor, facing `heading`: calm and still, resting
    /// `resting` seconds before it sets off.
    pub fn put_at(&mut self, spot: Vector3<f32>, heading: f32, resting: f32) {
        self.at = spot + Vector3::new(0.0, self.hover, 0.0);
        self.heading = heading;
        self.state = State::Patrol;
        self.speed = 0.0;
        self.route.clear();
        self.resting = resting;
        self.scanning = 0.0;
    }

    /// Calm, going nowhere: taken out of the round.
    pub fn calm(&mut self) {
        self.state = State::Patrol;
        self.route.clear();
    }

    /// Whether it is after the player: searching for them, or on Alert.
    pub fn hostile(&self) -> bool {
        matches!(self.state, State::Alert | State::Search { .. })
    }

    /// Searches from `at`, going there first.
    pub fn search(&mut self, at: Vector3<f32>) {
        self.state = State::Search { left: self.tuning.search };
        self.lost_at = at;
        self.searched = 0;
        self.route.clear();
        self.scanning = 0.0;
    }

    /// Gives up on the player: back to its patrol.
    pub fn stand_down(&mut self) {
        if self.hostile() {
            self.state = State::Patrol;
            self.route.clear();
            self.scanning = 0.0;
            self.resting = self.tuning.rest.0;
        }
    }

    /// The alarm sounded with the player at `at`: calm or searching, it searches there.
    /// [`Event::Alarmed`] if it was calm.
    pub fn alarm(&mut self, at: Vector3<f32>) -> Option<Event> {
        self.alerted(at, Event::Alarmed)
    }

    /// A bolt flew by from `at`: calm or searching, it searches there. [`Event::Searching`] if it
    /// was calm.
    pub fn shot_at(&mut self, at: Vector3<f32>) -> Option<Event> {
        self.alerted(at, Event::Searching)
    }

    /// Sent to search at `at`, if it is not after the player already or down: `calm` if it was
    /// calm.
    fn alerted(&mut self, at: Vector3<f32>, calm: Event) -> Option<Event> {
        match self.state {
            State::Down | State::Alert => None,
            was => {
                self.search(at);
                (was == State::Patrol).then_some(calm)
            }
        }
    }

    /// Hit by a bolt from `from`, and still up: it searches there, if it was not after the
    /// player already.
    pub fn hit(&mut self, from: Vector3<f32>) {
        if self.state != State::Alert && self.state != State::Down {
            self.search(from);
        }
    }

    /// Brought down: it stops where it is, and thinks no more.
    pub fn down(&mut self) {
        self.state = State::Down;
        self.route.clear();
        self.speed = 0.0;
    }

    /// Takes in what it `sees` - whether it can see the player, whose feet are at `player`, if it
    /// looked (see [`Patrol::looks`]) - chooses what to do about it (see [`Patrol::reasoner`]), and
    /// decides where to go, finding its `ways`, for another `dt`. What it has to say, if anything.
    pub fn think(&mut self, sees: bool, player: Vector3<f32>, ways: &mut dyn Ways, dt: f32) -> Option<Event> {
        let under = self.under();
        let tuning = self.tuning;
        let current = match self.state {
            State::Down => return None,
            State::Alert => Mode::Engage,
            State::Search { .. } => Mode::Search,
            State::Patrol => Mode::Patrol,
        };
        // How long it has left to search, before it chooses: its time runs from when it gets to
        // where it was sent, not while it is on its way.
        let search_left = match self.state {
            State::Search { left } => {
                let on_its_way = self.searched <= 1 && self.scanning == 0.0 && !self.route.is_empty();
                if on_its_way { left } else { left - dt }
            }
            State::Alert => tuning.search,
            _ => 0.0,
        };
        let reading = Reading {
            sees,
            search_left,
            distance: if sees { flat(player - under).norm() } else { 1.0e4 },
            searched: self.searched,
            post: self.post.is_some(),
        };
        let mode = self.reasoner.choose(&reading, Some(current)).unwrap_or(current);
        match (current, mode) {
            // Seeing the player, on to them.
            (Mode::Patrol | Mode::Search, Mode::Engage) => {
                self.state = State::Alert;
                self.fire_in = tuning.first_shot;
                self.replan = 0.0;
                self.scanning = 0.0;
                Some(Event::Spotted)
            }
            // Lost them: it searches where it last saw them.
            (Mode::Engage | Mode::Patrol, Mode::Search) => {
                let at = self.lost_at;
                self.search(at);
                Some(Event::Searching)
            }
            // Done searching, or giving up on them: calm again.
            (Mode::Search | Mode::Engage, Mode::Patrol) => {
                self.state = State::Patrol;
                self.route.clear();
                self.scanning = 0.0;
                self.resting = tuning.rest.0;
                Some(Event::Calm)
            }
            (_, Mode::Patrol) => {
                if self.route.is_empty() {
                    self.resting -= dt;
                    if self.resting <= 0.0 {
                        self.route = match self.post {
                            // Keeping a post, round it.
                            Some(post) => ways
                                .spot_near(post, tuning.post_reach)
                                .map(|to| ways.route_to(to))
                                .unwrap_or_default(),
                            None => ways.trip(under, tuning.trip),
                        };
                        self.resting = ways.between(tuning.rest);
                    }
                }
                None
            }
            (_, Mode::Engage) => {
                self.lost_at = player;
                self.fire_in -= dt;
                // Facing them, coming on to within its range of them, and no nearer.
                let to = flat(player - under);
                self.heading = to.x.atan2(to.z);
                self.replan -= dt;
                // Flying free, as near as the crow flies to its height over them.
                let near = match self.flies_free {
                    true => (player + Vector3::new(0.0, self.hover, 0.0) - self.at).norm(),
                    false => to.norm(),
                };
                if near <= tuning.engage {
                    self.route.clear();
                } else if self.replan <= 0.0 || self.route.is_empty() && !self.no_way {
                    self.replan = tuning.replan;
                    self.route = ways.route_to(player);
                    self.no_way = self.route.is_empty();
                }
                // Out of range, it holds its fire till it is nearer.
                if to.norm() > tuning.fire_range {
                    self.fire_in = self.fire_in.max(0.1);
                }
                None
            }
            (_, Mode::Search) => {
                self.state = State::Search { left: search_left };
                let mut says = None;
                if self.scanning > 0.0 {
                    self.scanning -= dt;
                    // Done scanning: on to another spot nearby.
                    if self.scanning <= 0.0 {
                        self.scanning = 0.0;
                        self.searched += 1;
                        self.route = ways.trip(under, tuning.search_trip);
                    }
                } else if self.route.is_empty() {
                    if self.searched == 0 && flat(self.lost_at - under).norm() > tuning.reached {
                        // First where the player was.
                        self.searched = 1;
                        self.route = ways.route_to(self.lost_at);
                        says = Some(Event::Searching);
                    } else {
                        // Got there, or has nowhere to go: it scans.
                        self.searched = self.searched.max(1);
                        self.scanning = self.scan_for;
                    }
                }
                says
            }
        }
    }

    /// Whether it looks for the player at all: while it is after them, or all the while if it
    /// [`watches`](Patrol::watches).
    pub fn looks(&self) -> bool {
        self.state != State::Down && (self.hostile() || self.watches)
    }

    /// Flies on along its route for another `dt` through the `air`, turning to face the way it
    /// goes - or the player, at `player`, on Alert - and hovering over the floor.
    pub fn fly(&mut self, player: Vector3<f32>, air: &dyn Air, dt: f32) {
        if self.state == State::Down {
            return;
        }
        let under = self.under();
        // Flying free, its route is through the air, up and down as well as along; otherwise
        // along the floor under it.
        let free = self.flies_free;
        let at = self.at;
        let toward = |next: Vector3<f32>| match free {
            true => next - at,
            false => flat(next - under),
        };
        // Past the points it has got near, the last one only once it is there.
        while let Some(&next) = self.route.last() {
            let near = if self.route.len() == 1 { self.tuning.arrived } else { self.tuning.reached };
            if toward(next).norm() > near {
                break;
            }
            self.route.pop();
        }
        let speeds = self.speeds;
        let top = if self.hostile() { speeds.hurry } else { speeds.patrol };
        let (way, wanted) = match self.route.last() {
            Some(&next) => {
                let to = toward(next);
                let distance = to.norm();
                // Slowing down for the end of its route.
                let wanted = match self.route.len() {
                    1 => top.min((2.0 * speeds.acceleration * distance).sqrt()),
                    _ => top,
                };
                (to / distance.max(1.0e-4), wanted)
            }
            None => (Vector3::zeros(), 0.0),
        };
        self.speed = match wanted > self.speed {
            true => (self.speed + speeds.acceleration * dt).min(wanted),
            false => (self.speed - 2.0 * speeds.acceleration * dt).max(wanted),
        };
        let mut next = self.at + way * self.speed * dt;
        // Facing the way it goes, or the player on Alert.
        let facing = match self.state {
            State::Alert => {
                let to = flat(player - under);
                (to.norm() > 1.0e-3).then(|| to.x.atan2(to.z))
            }
            // Not turning as it rises or drops straight up or down.
            _ => (self.speed > 0.1 && flat(way).norm() > 0.3).then(|| way.x.atan2(way.z)),
        };
        if let Some(facing) = facing {
            let turn = wrap(facing - self.heading);
            self.heading = wrap(self.heading + turn.clamp(-speeds.turn_rate * dt, speeds.turn_rate * dt));
        }
        // Hovering over the floor where it is, if there is floor there - unless it flies free, at
        // the height its route has. Flying free, it never flies into anything solid: something in
        // its way after all - something moved there, or a way along the floor too tight to fly -
        // it stops, and finds another way.
        if free {
            if !air.clear(self.at, next) {
                self.speed = 0.0;
                self.route.clear();
                self.replan = 0.0;
                return;
            }
        } else if let Some(floor) = air.floor_under(next - Vector3::new(0.0, self.hover, 0.0)) {
            let height = floor + self.hover;
            next.y += (height - next.y) * (1.0 - (-self.tuning.floor_easing * dt).exp());
        }
        // Never into anyone, sideways, up or down: it waits for them to move.
        if air.blocked(self.at, next - self.at) {
            self.speed = 0.0;
        } else {
            self.at = next;
        }
    }
}

/// `angle` brought round into -π..π.
pub fn wrap(angle: f32) -> f32 {
    let tau = std::f32::consts::TAU;
    (angle + std::f32::consts::PI).rem_euclid(tau) - std::f32::consts::PI
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPEEDS: Speeds = Speeds { patrol: 1.4, hurry: 3.5, acceleration: 3.0, turn_rate: 3.0 };

    /// Open, flat ground: straight to anywhere, a trip always 10 m on along +z.
    struct Open;

    impl Ways for Open {
        fn route_to(&mut self, to: Vector3<f32>) -> Vec<Vector3<f32>> {
            vec![to]
        }
        fn trip(&mut self, under: Vector3<f32>, _: (f32, f32)) -> Vec<Vector3<f32>> {
            vec![under + Vector3::new(0.0, 0.0, 10.0)]
        }
        fn spot_near(&mut self, post: Vector3<f32>, _: f32) -> Option<Vector3<f32>> {
            Some(post)
        }
        fn between(&mut self, range: (f32, f32)) -> f32 {
            range.0
        }
    }

    impl Air for Open {
        fn clear(&self, _: Vector3<f32>, _: Vector3<f32>) -> bool {
            true
        }
        fn floor_under(&self, _: Vector3<f32>) -> Option<f32> {
            Some(0.0)
        }
        fn blocked(&self, _: Vector3<f32>, _: Vector3<f32>) -> bool {
            false
        }
    }

    fn sentry() -> Patrol {
        let mut patrol = Patrol::new(1.7, false, SPEEDS, 4.0);
        patrol.put_at(Vector3::zeros(), 0.0, 1.0);
        patrol
    }

    #[test]
    fn calm_it_rests_then_sets_off_and_flies_its_trip_hovering() {
        let mut patrol = sentry();
        let player = Vector3::new(50.0, 0.0, 0.0);
        let dt = 0.1;
        for _ in 0..200 {
            assert_eq!(patrol.think(false, player, &mut Open, dt), None);
            patrol.fly(player, &Open, dt);
        }
        assert_eq!(patrol.state, State::Patrol);
        assert!(patrol.under().z > 5.0, "it went on its trip: {:?}", patrol.at);
        assert!((patrol.at.y - 1.7).abs() < 0.05, "hovering: {:?}", patrol.at);
    }

    #[test]
    fn seeing_the_player_it_comes_on_to_its_range_facing_them_and_searches_when_it_loses_them() {
        let mut patrol = sentry();
        patrol.search(Vector3::new(0.0, 0.0, 5.0));
        let player = Vector3::new(20.0, 0.0, 0.0);
        assert_eq!(patrol.think(true, player, &mut Open, 0.1), Some(Event::Spotted));
        assert_eq!(patrol.state, State::Alert);
        let dt = 0.1;
        for _ in 0..200 {
            patrol.think(true, player, &mut Open, dt);
            patrol.fly(player, &Open, dt);
        }
        let away = flat(player - patrol.under()).norm();
        assert!(away <= ENGAGE + 0.5 && away > 1.0, "{away}");
        assert!(wrap(patrol.heading - std::f32::consts::FRAC_PI_2).abs() < 0.05, "facing them: {}", patrol.heading);
        assert!(patrol.fire_in <= 0.0, "ready to fire");
        // Lost from sight: it searches where it last saw them.
        assert_eq!(patrol.think(false, player, &mut Open, dt), Some(Event::Searching));
        assert_eq!(patrol.lost_at, player);
    }

    #[test]
    fn a_search_runs_out_and_it_is_calm_again() {
        let mut patrol = sentry();
        assert_eq!(patrol.alarm(Vector3::new(0.0, 0.0, 3.0)), Some(Event::Alarmed));
        let (player, dt) = (Vector3::new(100.0, 0.0, 0.0), 0.1);
        let mut said = Vec::new();
        for _ in 0..((SEARCH + 60.0) / dt) as usize {
            said.extend(patrol.think(false, player, &mut Open, dt));
            patrol.fly(player, &Open, dt);
        }
        assert_eq!(said.last(), Some(&Event::Calm), "{said:?}");
        assert_eq!(patrol.state, State::Patrol);
        assert!(patrol.searched >= 2, "it searched more than one spot: {}", patrol.searched);
    }

    #[test]
    fn its_tuning_is_its_own() {
        // Searching 5 s rather than 30, it is calm again long before the default would be.
        let mut patrol = sentry();
        patrol.tuning.search = 5.0;
        assert_eq!(patrol.shot_at(Vector3::new(0.0, 0.0, 1.0)), Some(Event::Searching));
        let (player, dt) = (Vector3::new(100.0, 0.0, 0.0), 0.1);
        let calm = (0..(20.0 / dt) as usize).position(|_| {
            let said = patrol.think(false, player, &mut Open, dt);
            patrol.fly(player, &Open, dt);
            said == Some(Event::Calm)
        });
        assert!(calm.is_some_and(|step| (step as f32 * dt) < SEARCH), "{calm:?}");
        // Coming on to 15 m on Alert, not 6.
        let mut patrol = sentry();
        patrol.tuning.engage = 15.0;
        let player = Vector3::new(40.0, 0.0, 0.0);
        for _ in 0..400 {
            patrol.think(true, player, &mut Open, dt);
            patrol.fly(player, &Open, dt);
        }
        let away = flat(player - patrol.under()).norm();
        assert!(away > ENGAGE + 5.0 && away <= 15.5, "{away}");
    }

    #[test]
    fn its_reasoner_chooses_and_says_why() {
        let mut patrol = sentry();
        let player = Vector3::new(20.0, 0.0, 0.0);
        assert_eq!(patrol.think(true, player, &mut Open, 0.1), Some(Event::Spotted));
        let scores = &patrol.reasoner.scores;
        assert_eq!(scores.iter().map(|&(m, _)| m).collect::<Vec<_>>(), vec![Mode::Engage, Mode::Search, Mode::Patrol]);
        assert_eq!(scores[0].1, 1.0, "it saw them");
        assert_eq!(scores[1].1, 0.0, "nothing to search for, calm");
    }

    #[test]
    fn weighed_its_own_way_a_drone_leaves_a_player_too_far_off_alone() {
        // Engaging only within 15 m - fully at 10, not at all past 15.
        let mut patrol = sentry();
        patrol.reasoner = reasoner();
        patrol.reasoner.actions[0] = Action::new(Mode::Engage, 1.0)
            .when(Input::Sees, Curve::Above { at: 0.5 })
            .when(Input::Distance, Curve::Linear { from: 15.0, to: 10.0 });
        let far = Vector3::new(30.0, 0.0, 0.0);
        assert_eq!(patrol.think(true, far, &mut Open, 0.1), None);
        assert_eq!(patrol.state, State::Patrol, "seen, but too far to bother with");
        let near = Vector3::new(8.0, 0.0, 0.0);
        assert_eq!(patrol.think(true, near, &mut Open, 0.1), Some(Event::Spotted));
        // The default goes after them however far.
        let mut patrol = sentry();
        assert_eq!(patrol.think(true, far, &mut Open, 0.1), Some(Event::Spotted));
    }

    #[cfg(feature = "serde")]
    #[test]
    fn its_reasoner_can_be_read_from_a_file() {
        let read: Reasoner<Mode, Input> = serde_json::from_str(
            r#"{ "inertia": 0.2, "actions": [
                { "action": "Engage", "weight": 1.0, "considerations": [
                    { "input": "Sees", "curve": { "Above": { "at": 0.5 } } },
                    { "input": "Distance", "curve": { "Linear": { "from": 15.0, "to": 10.0 } } } ] },
                { "action": "Patrol", "weight": 0.2, "considerations": [] } ] }"#,
        )
        .unwrap();
        assert_eq!(read.actions.len(), 2);
        assert_eq!(read.actions[0].considerations[1].curve, Curve::Linear { from: 15.0, to: 10.0 });
        let back: Reasoner<Mode, Input> = serde_json::from_str(&serde_json::to_string(&reasoner()).unwrap()).unwrap();
        assert_eq!(back, reasoner());
    }

    #[test]
    fn down_it_goes_nowhere_and_says_nothing() {
        let mut patrol = sentry();
        patrol.down();
        assert_eq!(patrol.think(true, Vector3::zeros(), &mut Open, 0.1), None);
        assert_eq!(patrol.alarm(Vector3::zeros()), None);
        assert_eq!(patrol.shot_at(Vector3::zeros()), None);
        assert!(!patrol.looks());
    }
}
