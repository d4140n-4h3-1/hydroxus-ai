//! Traffic: bikes going about a city's streets, keeping to their lanes, turning at the
//! junctions, stopping for the lights and for anyone in the road.
//!
//! The streets are straight strips 16 m wide, kerb to kerb, a junction wherever two cross
//! ([`Network::from_roads`]). Traffic keeps to the right, two lanes each way, [`LANE`] wide. What
//! can be driven is found by looking (see [`Network::survey`], which casts its rays with the
//! game's): a lane from one junction to the next with anything in it is not driven, nor a turn
//! through a junction that would meet something, nor a street with no road under it.
//!
//! A bike goes from junction to junction along its lane, and at each turns left or right or goes on
//! across, as it planned on the way there - in the right-hand lane to turn right and the left-hand
//! to turn left - round a curve into its lane on the way out. It keeps its distance from the bike
//! in front, and stops for anyone in the road before it.
//!
//! The junctions where two main streets meet ([`main_street`]: every other street) have stop
//! lights, if there is room for them all (see [`Network::survey`]; [`signal_place`] is where each
//! stands). Each junction lets one street's traffic through at a time: green, then amber, then a
//! moment of red both ways ([`CYCLE`]); a bike stops at the line on red, and on amber if it can.
//! At a junction with no lights a bike goes in only while nothing is crossing it the other way.
//!
//! The game draws the bikes where the [`Rider`]s are and the lights as [`Traffic::light_for`] has
//! them; this decides where the bikes go.

use nalgebra::Vector3;

/// A lane's width.
pub const LANE: f32 = 4.0;
/// How far from a junction's middle a lane starts, on the way out of it, and where its traffic
/// stops, on the way in: behind the crossing, which is 8.5 to 10.5 m out.
pub const EXIT: f32 = 8.0;
pub const STOP: f32 = 11.5;
/// Where a stop light's mast stands from the junction's middle: back along the street it holds,
/// behind the crossing and clear of the lamp on the corner, and out on the pavement beyond the
/// kerb.
pub const MAST_BACK: f32 = 12.5;
pub const MAST_OUT: f32 = 9.2;
pub const ARM: (f32, f32) = (9.4, 5.6);
/// The lights: how long a street's traffic has green, and amber, and how long both have red
/// after.
pub const GREEN: f32 = 12.0;
pub const AMBER: f32 = 3.0;
pub const ALL_RED: f32 = 1.5;
pub const CYCLE: f32 = 2.0 * (GREEN + AMBER + ALL_RED);

/// How near the player a bike is put down at the start, at least.
const CLEAR_OF_PLAYER: f32 = 40.0;
/// How fast a bike goes along a street, at the least and the most, and round a turn; how hard it
/// picks up speed, and brakes - as it means to, and at the most.
const CRUISE: (f32, f32) = (11.0, 15.0);
const TURN_SPEED: f32 = 6.0;
const ACCEL: f32 = 3.5;
const EASE: f32 = 4.0;
const BRAKE: f32 = 9.0;
/// How far ahead a bike looks for someone in its way, how far to either side of its line, and how
/// far short of them it stops: a bike, a body's length and more; anyone on foot, a little more.
const LOOK: f32 = 24.0;
const WIDE: f32 = 1.6;
const GAP_BIKE: f32 = 4.5;
const GAP_FOOT: f32 = 3.5;
/// How long a bike waits behind another before it goes on round it all the same: two waiting for
/// each other across a junction - one turning left, one coming the other way - would wait for
/// ever. It never goes on into anyone on foot.
const PATIENCE: f32 = 2.5;

/// The four ways along the streets, by number: +x, +z, -x, -z. Turning right from way `w` is way
/// `w + 1`, left `w + 3`, back `w + 2`, all round the four.
const WAYS: [[f32; 2]; 4] = [[1.0, 0.0], [0.0, 1.0], [-1.0, 0.0], [0.0, -1.0]];

pub fn way(w: usize) -> Vector3<f32> {
    Vector3::new(WAYS[w % 4][0], 0.0, WAYS[w % 4][1])
}

/// The right of going along `d`: as the player's is, facing +z it is -x.
fn right_of(d: Vector3<f32>) -> Vector3<f32> {
    Vector3::new(-d.z, 0.0, d.x)
}

/// How far out from the middle of the street lane `k` is: 0 the one by the middle, 1 by the kerb.
fn lane_out(k: usize) -> f32 {
    LANE * (k as f32 + 0.5)
}

/// Whether a street through `centre` (x for one running along z, z for one along x) is a main
/// street, with stop lights where it meets another: every other one, 128 m apart as the cities'
/// blocks go, the middle avenue and the middle street among them.
pub fn main_street(centre: f32) -> bool {
    (centre / 64.0).round().rem_euclid(2.0) == 0.0
}

/// A junction: where it is, and the link out of it each way, if there is one.
#[derive(Debug, Clone, PartialEq)]
pub struct Junction {
    pub at: Vector3<f32>,
    pub out: [Option<usize>; 4],
    /// Whether it has stop lights, and how far into their cycle it started.
    pub lit: bool,
    pub offset: f32,
    /// Whether its streets are both main streets, for it to be lit if there is room.
    main: bool,
    /// Which turns through it can be driven: from coming in going `[from]` to going out `[to]`.
    pub turns: [[bool; 4]; 4],
}

/// A street from one junction to the next, one way.
#[derive(Debug, Clone, PartialEq)]
pub struct Link {
    pub from: usize,
    pub to: usize,
    pub way: usize,
    /// Which of its lanes can be driven: by the middle, and by the kerb.
    pub lanes: [bool; 2],
}

/// The streets, as junctions and the links between them.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Network {
    pub junctions: Vec<Junction>,
    pub links: Vec<Link>,
}

/// Where a ray from a point along a way (of length 1) first meets something within a reach, if
/// it meets anything: the game casts it.
pub type Cast<'a> = dyn FnMut(Vector3<f32>, Vector3<f32>, f32) -> Option<Vector3<f32>> + 'a;

impl Network {
    /// The junctions and links of the streets `roads` lay out, each `[x0, z0, x1, z1]`, kerb to
    /// kerb: a junction where one along x crosses one along z, a link each way between each two
    /// next to each other along a street. All of it drivable until it is surveyed.
    pub fn from_roads(roads: &[[f32; 4]]) -> Self {
        // Each street: along x or along z, through which middle, from where to where.
        let streets: Vec<(bool, f32, f32, f32)> = roads
            .iter()
            .map(|r| {
                let along_x = r[2] - r[0] > r[3] - r[1];
                if along_x {
                    (true, (r[1] + r[3]) / 2.0, r[0], r[2])
                } else {
                    (false, (r[0] + r[2]) / 2.0, r[1], r[3])
                }
            })
            .collect();
        let mut net = Network::default();
        let mut on_street: Vec<Vec<(f32, usize)>> = vec![Vec::new(); streets.len()];
        for (a, &(ax, ac, a0, a1)) in streets.iter().enumerate() {
            for (b, &(bx, bc, b0, b1)) in streets.iter().enumerate() {
                if !ax || bx {
                    continue;
                }
                // `a` along x through z = ac; `b` along z through x = bc.
                if bc > a0 + 1.0 && bc < a1 - 1.0 && ac > b0 + 1.0 && ac < b1 - 1.0 {
                    let at = Vector3::new(bc, 0.0, ac);
                    let j = match net.junctions.iter().position(|j| (j.at - at).norm() < 1.0) {
                        Some(j) => j,
                        None => {
                            net.junctions.push(Junction {
                                at,
                                out: [None; 4],
                                lit: false,
                                offset: 0.0,
                                main: main_street(bc) && main_street(ac),
                                turns: [[true; 4]; 4],
                            });
                            net.junctions.len() - 1
                        }
                    };
                    on_street[a].push((bc, j));
                    on_street[b].push((ac, j));
                }
            }
        }
        for (s, junctions) in on_street.iter_mut().enumerate() {
            junctions.sort_by(|p, q| p.0.total_cmp(&q.0));
            junctions.dedup_by_key(|p| p.1);
            let along_x = streets[s].0;
            for pair in junctions.windows(2) {
                let (a, b) = (pair[0].1, pair[1].1);
                // Up the street, +x or +z, and back down it.
                let (up, down) = if along_x { (0, 2) } else { (1, 3) };
                for (from, to, w) in [(a, b, up), (b, a, down)] {
                    if net.junctions[from].out[w].is_none() {
                        net.links.push(Link { from, to, way: w, lanes: [true; 2] });
                        net.junctions[from].out[w] = Some(net.links.len() - 1);
                    }
                }
            }
        }
        net
    }

    /// Where lane `lane` of `link` starts and ends.
    pub fn lane_ends(&self, link: usize, lane: usize) -> (Vector3<f32>, Vector3<f32>) {
        let l = &self.links[link];
        let d = way(l.way);
        let off = right_of(d) * lane_out(lane);
        let (a, b) = (self.junctions[l.from].at, self.junctions[l.to].at);
        (a + d * EXIT + off, b - d * STOP + off)
    }

    /// The curve through junction `j` from lane `from_lane` coming in going `from` to lane
    /// `to_lane` going out `to`: its start (at the line), the corner it bends round, and its end.
    pub fn turn_curve(&self, j: usize, from: usize, to: usize, from_lane: usize, to_lane: usize) -> [Vector3<f32>; 3] {
        let at = self.junctions[j].at;
        let (din, dout) = (way(from), way(to));
        let p0 = at - din * STOP + right_of(din) * lane_out(from_lane);
        let p1 = at + dout * EXIT + right_of(dout) * lane_out(to_lane);
        let corner = if from == to {
            (p0 + p1) / 2.0
        } else if (from + 2) % 4 == to {
            // Round and back: out past the middle.
            at + din * 2.0 + (right_of(din) * lane_out(from_lane) + right_of(dout) * lane_out(to_lane)) / 2.0
        } else {
            // Where the lane in and the lane out would meet: they cross square on.
            p0 + din * (p1 - p0).dot(&din)
        };
        [p0, corner, p1]
    }

    /// Looks over the streets in the world `cast` sees: the height of each junction; which lanes
    /// and turns can be driven, with nothing in them; and which main junctions have room for their
    /// stop lights - and lights those, starting each a different way into its cycle (`offset`).
    pub fn survey(&mut self, cast: &mut Cast<'_>, offset: &mut dyn FnMut() -> f32) {
        // Each junction on the road under it, if there is one.
        for j in &mut self.junctions {
            if let Some(hit) = cast(j.at + Vector3::new(0.0, 6.0, 0.0), -Vector3::y(), 12.0) {
                j.at.y = hit.y;
            }
        }
        let rise = |p: Vector3<f32>, h: f32| p + Vector3::y() * h;
        // A way between two points clear of anything a bike would meet, at the heights a bike's
        // body is, a body's width across.
        // A turn's curve is looked along its middle only: there are a great many of them, and
        // nothing stands in a junction but in its middle, or not at all.
        let clear = |cast: &mut Cast<'_>, a: Vector3<f32>, b: Vector3<f32>, wide: bool| {
            let d = b - a;
            let length = d.norm();
            if length < 1e-3 {
                return true;
            }
            let way = d / length;
            let side = right_of(way) * 0.6;
            let across: &[f32] = if wide { &[-1.0, 0.0, 1.0] } else { &[0.0] };
            [0.5, 1.4].iter().all(|&h| across.iter().all(|&s| cast(rise(a, h) + side * s, way, length).is_none()))
        };
        let road_under = |cast: &mut Cast<'_>, p: Vector3<f32>| {
            cast(rise(p, 4.0), -Vector3::y(), 6.0).is_some_and(|hit| (hit.y - p.y).abs() < 0.5)
        };
        for l in 0..self.links.len() {
            for lane in 0..2 {
                let (a, b) = self.lane_ends(l, lane);
                let (a, b) = (rise(a, 0.0), rise(b, 0.0));
                let ok = road_under(cast, a) && road_under(cast, b) && road_under(cast, (a + b) / 2.0) && clear(cast, a, b, true);
                self.links[l].lanes[lane] = ok;
            }
        }
        for j in 0..self.junctions.len() {
            for from in 0..4 {
                // Only the turns there are: in along a street that reaches it, out along one.
                if self.junctions[j].out[(from + 2) % 4].is_none() {
                    continue;
                }
                for to in 0..4 {
                    if self.junctions[j].out[to].is_none() {
                        continue;
                    }
                    let (from_lane, to_lane) = turn_lanes(from, to);
                    let [p0, c, p1] = self.turn_curve(j, from, to, from_lane, to_lane);
                    let mut ok = true;
                    let mut last = p0;
                    for k in 1..=4 {
                        let p = bezier([p0, c, p1], k as f32 / 4.0);
                        ok = ok && clear(cast, last, p, false);
                        last = p;
                    }
                    self.junctions[j].turns[from][to] = ok;
                }
            }
            // Lights where two main streets meet, if each corner has room for its mast and its arm
            // reaches over the street clear of anything.
            let junction = &self.junctions[j];
            if !junction.main {
                continue;
            }
            let at = junction.at;
            // Only where traffic comes in: at the city's edge a junction has a street but three ways.
            let out = junction.out;
            let room = (0..4).filter(|&w| out[w].is_some()).all(|w| {
                let (mast, arm) = signal_place(at, w);
                let foot_clear = cast(rise(mast, 6.5), -Vector3::y(), 7.0).is_some_and(|hit| hit.y < at.y + 0.5);
                foot_clear && cast(rise(mast, ARM.1), arm, ARM.0).is_none()
            });
            let junction = &mut self.junctions[j];
            junction.lit = room;
            junction.offset = offset();
        }
    }

    /// The ways out of junction `j` a bike coming in going `from` can take: on, left or right
    /// along a street with a lane to drive, through a turn that is clear; back only if there is
    /// nothing else.
    pub fn ways_on(&self, j: usize, from: usize) -> Vec<usize> {
        let junction = &self.junctions[j];
        let open = |to: usize| {
            junction.out[to].is_some_and(|l| self.links[l].lanes.iter().any(|&ok| ok)) && junction.turns[from][to]
        };
        let ways: Vec<usize> = [from, (from + 1) % 4, (from + 3) % 4].into_iter().filter(|&to| open(to)).collect();
        if ways.is_empty() && open((from + 2) % 4) {
            vec![(from + 2) % 4]
        } else {
            ways
        }
    }
}

/// The lanes a turn from going `from` to going `to` is from and to: turning right, the kerb's lane
/// into the kerb's; anything else, the middle's into the middle's.
fn turn_lanes(from: usize, to: usize) -> (usize, usize) {
    if to == (from + 1) % 4 {
        (1, 1)
    } else {
        (0, 0)
    }
}

/// Where the mast of the stop light holding the traffic coming in to the junction at `at` from
/// way `w` stands - on the corner on its right, back behind the crossing - and which way its arm
/// reaches, over the street.
pub fn signal_place(at: Vector3<f32>, w: usize) -> (Vector3<f32>, Vector3<f32>) {
    let back = way(w);
    let going = -back;
    let right = right_of(going);
    (at + back * MAST_BACK + right * MAST_OUT, -right)
}

/// A point `t` of the way along the quadratic curve through `p`.
fn bezier(p: [Vector3<f32>; 3], t: f32) -> Vector3<f32> {
    let u = 1.0 - t;
    p[0] * (u * u) + p[1] * (2.0 * u * t) + p[2] * (t * t)
}

fn bezier_ahead(p: [Vector3<f32>; 3], t: f32) -> Vector3<f32> {
    let d = (p[1] - p[0]) * (2.0 * (1.0 - t)) + (p[2] - p[1]) * (2.0 * t);
    d.try_normalize(1e-5).unwrap_or_else(|| (p[2] - p[0]).normalize())
}

fn bezier_length(p: [Vector3<f32>; 3]) -> f32 {
    (1..=16).map(|k| (bezier(p, k as f32 / 16.0) - bezier(p, (k - 1) as f32 / 16.0)).norm()).sum()
}

/// What a junction's lights show one of its streets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Light {
    Green,
    Amber,
    Red,
}

/// What the lights of a junction show the street along `axis` (0 along x, 1 along z), `t` seconds
/// into their cycle: green, amber and then red for the x street while the z street has red, and
/// then the other way round, with red both ways between.
pub fn light(axis: usize, t: f32) -> Light {
    let t = (t.rem_euclid(CYCLE) - if axis == 0 { 0.0 } else { CYCLE / 2.0 }).rem_euclid(CYCLE);
    if t < GREEN {
        Light::Green
    } else if t < GREEN + AMBER {
        Light::Amber
    } else {
        Light::Red
    }
}

/// Where a bike is on its way: along a lane of a link, or round a turn through a junction.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Leg {
    Lane { link: usize, lane: usize },
    Turn { junction: usize, from: usize, to: usize, from_lane: usize, to_lane: usize },
}

/// A bike going about the streets.
#[derive(Debug, Clone, PartialEq)]
pub struct Rider {
    pub leg: Leg,
    /// How far along its leg it is, and how fast it is going, in meters and meters per second.
    pub along: f32,
    pub speed: f32,
    /// How fast it goes along a street when nothing holds it up.
    pub cruise: f32,
    /// Which way it will turn at the end of the lane it is on, or the one it is turning into.
    next: usize,
    /// Where it is and which way it faces, as of the last step.
    pub at: Vector3<f32>,
    pub ahead: Vector3<f32>,
    /// How far it leans into its turn: left, turning left, positive.
    pub lean: f32,
    /// How long it has been held up behind another bike (see [`PATIENCE`]).
    waited: f32,
}

/// Someone a bike stops for.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Obstacle {
    Bike(Vector3<f32>),
    Foot(Vector3<f32>),
}

/// The traffic: the streets, the bikes on them, and the clock their junctions' lights go by.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Traffic {
    pub network: Network,
    pub riders: Vec<Rider>,
    /// How long the traffic has gone, for the lights' cycles.
    pub time: f32,
}

impl Traffic {
    /// Traffic on `network`, `count` bikes put down at random on its lanes - `pick` giving numbers
    /// from 0 to 1 - away from `player`.
    pub fn new(network: Network, count: usize, player: Vector3<f32>, pick: &mut dyn FnMut() -> f32) -> Self {
        let mut traffic = Traffic { network, ..Default::default() };
        let lanes: Vec<(usize, usize)> = (0..traffic.network.links.len())
            .flat_map(|l| (0..2).map(move |k| (l, k)))
            .filter(|&(l, k)| traffic.network.links[l].lanes[k])
            .collect();
        if lanes.is_empty() {
            return traffic;
        }
        let mut tries = 0;
        while traffic.riders.len() < count && tries < count * 20 {
            tries += 1;
            let (link, lane) = lanes[((pick() * lanes.len() as f32) as usize).min(lanes.len() - 1)];
            let (a, b) = traffic.network.lane_ends(link, lane);
            let length = (b - a).norm();
            let along = pick() * length;
            let at = a + (b - a).normalize() * along;
            if (at - player).norm() < CLEAR_OF_PLAYER || traffic.riders.iter().any(|r| (r.at - at).norm() < 14.0) {
                continue;
            }
            let to = traffic.network.links[link].to;
            let ways = traffic.network.ways_on(to, traffic.network.links[link].way);
            if ways.is_empty() {
                continue;
            }
            let next = ways[((pick() * ways.len() as f32) as usize).min(ways.len() - 1)];
            let cruise = CRUISE.0 + (CRUISE.1 - CRUISE.0) * pick();
            traffic.riders.push(Rider {
                leg: Leg::Lane { link, lane },
                along,
                speed: cruise,
                cruise,
                next,
                at,
                ahead: way(traffic.network.links[link].way),
                lean: 0.0,
                waited: 0.0,
                });
        }
        traffic
    }

    /// What junction `j`'s lights show the traffic coming in going `from`, if it is lit.
    pub fn light_for(&self, j: usize, from: usize) -> Option<Light> {
        let junction = &self.network.junctions[j];
        junction.lit.then(|| light(from % 2, self.time + junction.offset))
    }

    /// Runs the traffic on `dt` seconds, the bikes stopping for `people` on foot.
    pub fn step(&mut self, dt: f32, people: &[Vector3<f32>]) {
        self.time += dt;
        let bikes: Vec<(Vector3<f32>, Vector3<f32>)> = self.riders.iter().map(|r| (r.at, r.ahead)).collect();
        // Who is in each junction without lights, coming in which way.
        let mut inside: Vec<Vec<usize>> = vec![Vec::new(); self.network.junctions.len()];
        for rider in &self.riders {
            if let Leg::Turn { junction, from, .. } = rider.leg {
                inside[junction].push(from);
            }
        }
        for n in 0..self.riders.len() {
            let (target, hold, blocked) = self.wanted_speed(n, &bikes, people, &inside);
            let rider = &mut self.riders[n];
            rider.waited = if blocked && rider.speed < 0.5 { rider.waited + dt } else { 0.0 };
            // Picking up speed gently; slowing as hard as it has to, up to the hardest it can.
            let change = target - rider.speed;
            rider.speed = (rider.speed + change.clamp(-BRAKE * dt, ACCEL * dt)).max(0.0);
            let length = self.leg_length(n);
            let mut along = self.riders[n].along + self.riders[n].speed * dt;
            if along >= length {
                if hold {
                    along = length;
                    self.riders[n].speed = 0.0;
                } else {
                    along -= length;
                    self.next_leg(n, &mut inside);
                }
            }
            self.riders[n].along = along;
            let (at, ahead) = self.point(n);
            let rider = &mut self.riders[n];
            let turning = rider.ahead.cross(&ahead).y / dt.max(1e-3);
            // Leaning into its turn: left, turning left (positive), as the race's bikes do.
            rider.lean += ((turning * rider.speed * 0.04).clamp(-0.35, 0.35) - rider.lean) * (1.0 - (-5.0 * dt).exp());
            rider.at = at;
            rider.ahead = ahead;
        }
    }

    /// How fast bike `n` wants to go, and whether it must hold at the end of the lane it is on.
    fn wanted_speed(
        &self,
        n: usize,
        bikes: &[(Vector3<f32>, Vector3<f32>)],
        people: &[Vector3<f32>],
        inside: &[Vec<usize>],
    ) -> (f32, bool, bool) {
        let rider = &self.riders[n];
        let mut blocked = false;
        let impatient = rider.waited > PATIENCE;
        let mut target = rider.cruise;
        let mut hold = false;
        let left = self.leg_length(n) - rider.along;
        match rider.leg {
            Leg::Lane { link, .. } => {
                let l = &self.network.links[link];
                let from = l.way;
                // Easing off for the turn ahead.
                if rider.next != from {
                    target = target.min((TURN_SPEED * TURN_SPEED + 2.0 * EASE * left).sqrt());
                }
                hold = match self.light_for(l.to, from) {
                    Some(Light::Green) => false,
                    // On amber, stopping if it can, without braking hard.
                    Some(Light::Amber) => rider.speed * rider.speed / (2.0 * EASE) < left + 0.5,
                    Some(Light::Red) => true,
                    // No lights: in while nothing is crossing the other way.
                    None => inside[l.to].iter().any(|&w| w % 2 != from % 2),
                };
                if hold {
                    target = target.min((2.0 * EASE * (left - 0.3).max(0.0)).sqrt());
                }
            }
            Leg::Turn { .. } => target = target.min(TURN_SPEED),
        }
        // Whoever is in the way before it: the bike in front, keeping its distance; anyone on
        // foot, stopping short of them.
        let right = right_of(rider.ahead);
        let obstacles = bikes
            .iter()
            .enumerate()
            .filter(|&(m, _)| m != n)
            .map(|(_, &(at, _))| Obstacle::Bike(at))
            .chain(people.iter().map(|&p| Obstacle::Foot(p)));
        for obstacle in obstacles {
            let (at, gap, bike) = match obstacle {
                Obstacle::Bike(at) => (at, GAP_BIKE, true),
                Obstacle::Foot(at) => (at, GAP_FOOT, false),
            };
            if bike && impatient {
                continue;
            }
            let to = at - rider.at;
            if to.y.abs() > 3.0 {
                continue;
            }
            let (front, side) = (to.dot(&rider.ahead), to.dot(&right));
            if front > 0.0 && front < LOOK && side.abs() < WIDE {
                let slow = (2.0 * BRAKE * (front - gap).max(0.0)).sqrt();
                if slow < target {
                    target = slow;
                    blocked |= bike;
                }
            }
        }
        (target, hold, blocked)
    }

    /// The length of the leg bike `n` is on.
    fn leg_length(&self, n: usize) -> f32 {
        match self.riders[n].leg {
            Leg::Lane { link, lane } => {
                let (a, b) = self.network.lane_ends(link, lane);
                (b - a).norm()
            }
            Leg::Turn { junction, from, to, from_lane, to_lane } => {
                bezier_length(self.network.turn_curve(junction, from, to, from_lane, to_lane))
            }
        }
    }

    /// Where bike `n` is along its leg, and which way it faces.
    pub fn point(&self, n: usize) -> (Vector3<f32>, Vector3<f32>) {
        let rider = &self.riders[n];
        match rider.leg {
            Leg::Lane { link, lane } => {
                let (a, b) = self.network.lane_ends(link, lane);
                let d = (b - a).try_normalize(1e-5).unwrap_or_else(|| way(self.network.links[link].way));
                (a + d * rider.along.min((b - a).norm()), d)
            }
            Leg::Turn { junction, from, to, from_lane, to_lane } => {
                let curve = self.network.turn_curve(junction, from, to, from_lane, to_lane);
                let t = (rider.along / bezier_length(curve)).clamp(0.0, 1.0);
                (bezier(curve, t), bezier_ahead(curve, t))
            }
        }
    }

    /// Bike `n` on to its next leg: from its lane into the turn it planned through the junction at
    /// its end, or out of a turn into the lane beyond it - planning, as it goes into it, the turn at
    /// that lane's end, and so which lane to keep to.
    fn next_leg(&mut self, n: usize, inside: &mut [Vec<usize>]) {
        let net = &self.network;
        let rider = &self.riders[n];
        match rider.leg {
            Leg::Lane { link, lane } => {
                let l = &net.links[link];
                let to = rider.next;
                let Some(out) = net.junctions[l.to].out[to] else {
                    return;
                };
                // From the lane it is in - the one it kept to for this turn, if it could - into the
                // one the turn goes into, or the other if that cannot be driven.
                let (_, want_to) = turn_lanes(l.way, to);
                let to_lane = if net.links[out].lanes[want_to] { want_to } else { 1 - want_to };
                inside[l.to].push(l.way);
                self.riders[n].leg = Leg::Turn { junction: l.to, from: l.way, to, from_lane: lane, to_lane };
            }
            Leg::Turn { junction, to, to_lane, .. } => {
                let Some(out) = net.junctions[junction].out[to] else {
                    return;
                };
                // The turn at the end of the lane out, chosen now; the lane to keep to for it.
                let ways = net.ways_on(net.links[out].to, to);
                let pick = (self.time * 7.31 + n as f32 * 3.17).sin().abs();
                let next = if ways.is_empty() {
                    to
                } else {
                    // On across twice as often as either turn.
                    let weighted: Vec<usize> = ways.iter().flat_map(|&w| if w == to { vec![w, w] } else { vec![w] }).collect();
                    weighted[((pick * weighted.len() as f32) as usize).min(weighted.len() - 1)]
                };
                let (want, _) = turn_lanes(to, next);
                let lanes = net.links[out].lanes;
                let lane = if lanes[want] { want } else if lanes[to_lane] { to_lane } else { 1 - want };
                // Coming out of the turn into the lane it planned for: straight into it, or over
                // into the other one alongside as it goes - close enough, two lanes side by side.
                let rider = &mut self.riders[n];
                rider.leg = Leg::Lane { link: out, lane };
                rider.next = next;
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    /// A grid of streets 16 m wide, `n` by `n`, 64 m apart, from 0.
    fn grid(n: usize) -> Vec<[f32; 4]> {
        let span = (n - 1) as f32 * 64.0;
        let mut roads = Vec::new();
        for k in 0..n {
            let c = k as f32 * 64.0;
            roads.push([c - 8.0, -8.0, c + 8.0, span + 8.0]);
            roads.push([-8.0, c - 8.0, span + 8.0, c + 8.0]);
        }
        roads
    }

    #[test]
    fn junctions_are_where_streets_cross_and_links_join_the_next_each_way() {
        let net = Network::from_roads(&grid(3));
        assert_eq!(net.junctions.len(), 9);
        // Three streets each way, two links along each, both ways.
        assert_eq!(net.links.len(), 2 * 3 * 2 * 2);
        let middle = net.junctions.iter().position(|j| j.at == Vector3::new(64.0, 0.0, 64.0)).unwrap();
        assert!(net.junctions[middle].out.iter().all(Option::is_some));
        let corner = net.junctions.iter().position(|j| j.at == Vector3::zeros()).unwrap();
        assert_eq!(net.junctions[corner].out.iter().filter(|o| o.is_some()).count(), 2);
        for link in &net.links {
            let (a, b) = (net.junctions[link.from].at, net.junctions[link.to].at);
            assert_eq!((b - a).normalize(), way(link.way));
        }
    }

    #[test]
    fn traffic_keeps_right_and_turns_the_right_way() {
        let net = Network::from_roads(&grid(2));
        let link = net.links.iter().position(|l| l.way == 1).unwrap();
        let (a, b) = net.lane_ends(link, 0);
        // Going +z, the right is -x: the lanes are on the -x side of the middle.
        let middle_x = net.junctions[net.links[link].from].at.x;
        assert!(a.x < middle_x && b.x < middle_x);
        let (outer, _) = net.lane_ends(link, 1);
        assert!(outer.x < a.x);
        // Turning right is the next way round: facing +z, the right is -x, way 2.
        assert_eq!(right_of(way(1)), way(2));
        assert_eq!(right_of(way(0)), way(1));
        assert_eq!(turn_lanes(1, 2), (1, 1));
        assert_eq!(turn_lanes(1, 0), (0, 0));
    }

    #[test]
    fn main_streets_are_every_other_one_128_m_apart() {
        let mains: Vec<i32> = (-7..=7).map(|k| k * 64).filter(|&c| main_street(c as f32)).collect();
        assert_eq!(mains, vec![-384, -256, -128, 0, 128, 256, 384]);
    }

    #[test]
    fn on_open_ground_every_lane_turn_and_main_junction_is_kept_the_edges_too() {
        // A 5 by 5 grid 128 m apart in its middle: main streets every other one, from 0.
        let mut net = Network::from_roads(&grid(5));
        // Flat ground at 0 under everything, and nothing standing on it.
        let mut cast = |from: Vector3<f32>, way: Vector3<f32>, reach: f32| {
            (way.y < -0.5 && from.y >= 0.0 && from.y <= reach).then(|| Vector3::new(from.x, 0.0, from.z))
        };
        net.survey(&mut cast, &mut || 0.0);
        assert!(net.links.iter().all(|l| l.lanes == [true, true]));
        // Streets at 0, 64, 128, 192, 256: main at 0, 128 and 256, so 9 junctions lit - 5 of them
        // on the edge, with three ways in (or two, in the corners).
        let lit: Vec<Vector3<f32>> = net.junctions.iter().filter(|j| j.lit).map(|j| j.at).collect();
        assert_eq!(lit.len(), 9, "{lit:?}");
        assert!(lit.contains(&Vector3::zeros()) && lit.contains(&Vector3::new(128.0, 0.0, 0.0)));
        // Something standing where a corner's mast goes keeps that junction dark.
        let mut net = Network::from_roads(&grid(5));
        let (mast, _) = signal_place(Vector3::new(128.0, 0.0, 128.0), 0);
        let mut cast = |from: Vector3<f32>, way: Vector3<f32>, reach: f32| {
            let top = if (Vector3::new(from.x, 0.0, from.z) - Vector3::new(mast.x, 0.0, mast.z)).norm() < 0.5 { 3.0 } else { 0.0 };
            (way.y < -0.5 && from.y >= top && from.y - top <= reach).then(|| Vector3::new(from.x, top, from.z))
        };
        net.survey(&mut cast, &mut || 0.0);
        let middle = net.junctions.iter().find(|j| j.at.x == 128.0 && j.at.z == 128.0).unwrap();
        assert!(!middle.lit);
        assert_eq!(net.junctions.iter().filter(|j| j.lit).count(), 8);
    }

    #[test]
    fn the_lights_give_each_street_its_turn_never_both_at_once() {
        let mut seen = [false; 3];
        for k in 0..(CYCLE * 10.0) as usize {
            let t = k as f32 / 10.0;
            let (x, z) = (light(0, t), light(1, t));
            assert!(x == Light::Red || z == Light::Red, "both go at {t}");
            seen[match x { Light::Green => 0, Light::Amber => 1, Light::Red => 2 }] = true;
        }
        assert_eq!(seen, [true; 3]);
        assert_eq!(light(0, 1.0), Light::Green);
        assert_eq!(light(1, 1.0), Light::Red);
        assert_eq!(light(1, CYCLE / 2.0 + 1.0), Light::Green);
    }

    /// The `n` by `n` grid with every junction lit, all starting their cycles together.
    fn lit_grid(n: usize) -> Network {
        let mut net = Network::from_roads(&grid(n));
        for j in &mut net.junctions {
            j.lit = true;
        }
        net
    }

    #[test]
    fn a_bike_stops_at_the_line_on_red_and_goes_on_green() {
        let mut net = lit_grid(2);
        // Coming up +z: its street (z) has red for the first half of the cycle.
        let link = net.links.iter().position(|l| l.way == 1).unwrap();
        net.junctions[net.links[link].to].offset = 0.0;
        let mut traffic = Traffic { network: net, ..Default::default() };
        traffic.riders.push(Rider {
            leg: Leg::Lane { link, lane: 0 },
            along: 0.0,
            speed: 12.0,
            cruise: 12.0,
            next: 1,
            at: Vector3::zeros(),
            ahead: way(1),
            lean: 0.0,
            waited: 0.0,
        });
        let length = traffic.leg_length(0);
        let dt = 1.0 / 30.0;
        for _ in 0..(10.0 / dt) as usize {
            traffic.step(dt, &[]);
            assert!(matches!(traffic.riders[0].leg, Leg::Lane { .. }), "ran the red light at {}", traffic.time);
        }
        assert!(traffic.riders[0].speed < 0.1 && (length - traffic.riders[0].along) < 1.0);
        // Its green comes at half the cycle: it goes.
        for _ in 0..((CYCLE / 2.0 - 10.0 + 4.0) / dt) as usize {
            traffic.step(dt, &[]);
        }
        assert!(matches!(traffic.riders[0].leg, Leg::Turn { .. }) || traffic.riders[0].along < length - 1.0);
    }

    #[test]
    fn a_bike_stops_short_of_anyone_in_the_road_and_of_the_bike_in_front() {
        let net = Network::from_roads(&grid(3));
        let link = net.links.iter().position(|l| l.way == 0 && net.junctions[l.from].at == Vector3::zeros()).unwrap();
        let mut traffic = Traffic { network: net, ..Default::default() };
        let (a, b) = traffic.network.lane_ends(link, 0);
        let ahead = (b - a).normalize();
        traffic.riders.push(Rider {
            leg: Leg::Lane { link, lane: 0 },
            along: 0.0,
            speed: 12.0,
            cruise: 12.0,
            next: 0,
            at: a,
            ahead,
            lean: 0.0,
            waited: 0.0,
        });
        // Someone standing in its lane 30 m on.
        let person = a + ahead * 30.0;
        let dt = 1.0 / 30.0;
        for _ in 0..(8.0 / dt) as usize {
            traffic.step(dt, &[person]);
        }
        let rider = &traffic.riders[0];
        let short = (person - rider.at).dot(&ahead);
        assert!(rider.speed < 0.1 && short > 2.0 && short < GAP_FOOT + 1.0, "{short} at {}", rider.speed);
        // Off the road, it goes on.
        for _ in 0..(2.0 / dt) as usize {
            traffic.step(dt, &[]);
        }
        assert!(traffic.riders[0].speed > 3.0);
    }

    #[test]
    fn bikes_drive_round_a_grid_without_running_into_each_other() {
        let net = Network::from_roads(&grid(4));
        let mut seed = 0.37_f32;
        let mut pick = || {
            seed = (seed * 997.0 + 0.123).fract();
            seed
        };
        let mut traffic = Traffic::new(net, 12, Vector3::new(-500.0, 0.0, -500.0), &mut pick);
        assert_eq!(traffic.riders.len(), 12);
        let dt = 1.0 / 30.0;
        let mut closest = f32::MAX;
        let mut turned = 0;
        for _ in 0..(120.0 / dt) as usize {
            traffic.step(dt, &[]);
            for (n, a) in traffic.riders.iter().enumerate() {
                if matches!(a.leg, Leg::Turn { from, to, .. } if from != to) {
                    turned += 1;
                }
                for b in &traffic.riders[n + 1..] {
                    // Side by side in the two lanes is fine; one into the other is not.
                    let to = b.at - a.at;
                    if to.dot(&a.ahead).abs() < 1.5 && to.dot(&right_of(a.ahead)).abs() < 1.0 {
                        closest = closest.min(to.norm());
                    }
                }
            }
        }
        assert!(closest > 1.0, "two bikes ran into each other: {closest}");
        assert!(turned > 0, "no bike ever turned");
        // Still going: nothing is stuck for good.
        assert!(traffic.riders.iter().filter(|r| r.speed > 1.0).count() >= 6);
    }
}
