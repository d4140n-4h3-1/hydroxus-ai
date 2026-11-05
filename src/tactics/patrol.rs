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
//! The game says what it sees, finds its ways ([`Ways`]) and what is round it as it flies
//! ([`Air`]), and moves its body, plays its animations and fires; this decides where it goes and
//! what it has to say about it, by name: `spotted`, `searching`, `alarm` and `calm`.

use crate::flat;
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
        self.state = State::Search { left: SEARCH };
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
            self.resting = REST.0;
        }
    }

    /// The alarm sounded with the player at `at`, or a bolt flew by from there: calm or
    /// searching, it searches there. What it has to say about it if it was calm - `calm_says` -
    /// if anything.
    pub fn alerted(&mut self, at: Vector3<f32>, calm_says: &'static str) -> Option<&'static str> {
        match self.state {
            State::Down | State::Alert => None,
            was => {
                self.search(at);
                (was == State::Patrol).then_some(calm_says)
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
    /// looked (see [`Patrol::looks`]) - and decides where to go, finding its `ways`, for another
    /// `dt`. What it has to say, if anything.
    pub fn think(&mut self, sees: bool, player: Vector3<f32>, ways: &mut dyn Ways, dt: f32) -> Option<&'static str> {
        let under = self.under();
        match self.state {
            State::Down => None,
            State::Patrol | State::Search { .. } if sees => {
                self.state = State::Alert;
                self.fire_in = FIRST_SHOT;
                self.replan = 0.0;
                self.scanning = 0.0;
                Some("spotted")
            }
            State::Patrol => {
                if self.route.is_empty() {
                    self.resting -= dt;
                    if self.resting <= 0.0 {
                        self.route = match self.post {
                            // Keeping a post, round it.
                            Some(post) => ways
                                .spot_near(post, POST_REACH)
                                .map(|to| ways.route_to(to))
                                .unwrap_or_default(),
                            None => ways.trip(under, TRIP),
                        };
                        self.resting = ways.between(REST);
                    }
                }
                None
            }
            State::Alert if !sees => {
                let at = self.lost_at;
                self.search(at);
                Some("searching")
            }
            State::Alert => {
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
                if near <= ENGAGE {
                    self.route.clear();
                } else if self.replan <= 0.0 || self.route.is_empty() && !self.no_way {
                    self.replan = REPLAN;
                    self.route = ways.route_to(player);
                    self.no_way = self.route.is_empty();
                }
                // Out of range, it holds its fire till it is nearer.
                if to.norm() > FIRE_RANGE {
                    self.fire_in = self.fire_in.max(0.1);
                }
                None
            }
            State::Search { left } => {
                // Its time runs from when it gets to where it was sent, not while it is on its way.
                let on_its_way = self.searched <= 1 && self.scanning == 0.0 && !self.route.is_empty();
                let left = if on_its_way { left } else { left - dt };
                if left <= 0.0 {
                    self.state = State::Patrol;
                    self.route.clear();
                    self.scanning = 0.0;
                    self.resting = REST.0;
                    return Some("calm");
                }
                self.state = State::Search { left };
                let mut says = None;
                if self.scanning > 0.0 {
                    self.scanning -= dt;
                    // Done scanning: on to another spot nearby.
                    if self.scanning <= 0.0 {
                        self.scanning = 0.0;
                        self.searched += 1;
                        self.route = ways.trip(under, SEARCH_TRIP);
                    }
                } else if self.route.is_empty() {
                    if self.searched == 0 && flat(self.lost_at - under).norm() > REACHED {
                        // First where the player was.
                        self.searched = 1;
                        self.route = ways.route_to(self.lost_at);
                        says = Some("searching");
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
            let near = if self.route.len() == 1 { ARRIVED } else { REACHED };
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
            next.y += (height - next.y) * (1.0 - (-FLOOR_EASING * dt).exp());
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
        assert_eq!(patrol.think(true, player, &mut Open, 0.1), Some("spotted"));
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
        assert_eq!(patrol.think(false, player, &mut Open, dt), Some("searching"));
        assert_eq!(patrol.lost_at, player);
    }

    #[test]
    fn a_search_runs_out_and_it_is_calm_again() {
        let mut patrol = sentry();
        assert_eq!(patrol.alerted(Vector3::new(0.0, 0.0, 3.0), "alarm"), Some("alarm"));
        let (player, dt) = (Vector3::new(100.0, 0.0, 0.0), 0.1);
        let mut said = Vec::new();
        for _ in 0..((SEARCH + 60.0) / dt) as usize {
            said.extend(patrol.think(false, player, &mut Open, dt));
            patrol.fly(player, &Open, dt);
        }
        assert_eq!(said.last(), Some(&"calm"), "{said:?}");
        assert_eq!(patrol.state, State::Patrol);
        assert!(patrol.searched >= 2, "it searched more than one spot: {}", patrol.searched);
    }

    #[test]
    fn down_it_goes_nowhere_and_says_nothing() {
        let mut patrol = sentry();
        patrol.down();
        assert_eq!(patrol.think(true, Vector3::zeros(), &mut Open, 0.1), None);
        assert_eq!(patrol.alerted(Vector3::zeros(), "alarm"), None);
        assert!(!patrol.looks());
    }
}
