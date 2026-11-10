//! Times the AI the game runs every frame: a city's traffic, drones on patrol, and their routes
//! through the air. `cargo run --release --example ai_bench`.
//!
//! - **traffic**: a city of streets every 64 m, 21 each way (1.3 km across, as Urbs is), 64
//!   bikes and 200 people on foot, stepped at 60 a second for a minute.
//! - **patrol**: 50 drones, thinking and flying at 60 a second for a minute over open ground, half
//!   of them sent to search.
//! - **flight**: 2,000 routes through the air across a block of 40 buildings, each looked for
//!   with a ray-box test - straight there, over a building, or along the floor.
//! - **orchestrated**: squads of 10, 50, 100 and 250 sentries run by `orchestrate::Squad` for a
//!   minute at 60 a second, on a 160 m square of rooms and doorways, the player walking a loop
//!   through them - spotted, chased, lost, searched for. Only `Squad::step` is timed: the senses
//!   (a sight-line along the floor and `Sight::visibility`) and moving the bodies straight to
//!   where they are going are the game's, done outside the timer.
use hydroxus_ai::{
    navigation::{
        flight,
        grid::Rng,
        traffic::{Network, Traffic},
    },
    tactics::patrol::{self, Patrol, Speeds},
};
use hydroxus_ai::{
    flat,
    navigation::{grid::WalkGrid, navmesh::NavMesh},
    orchestrate::{Intent, Pace, Senses, Sentry, Squad},
    perception::sight::{Sight, Stance},
    tactics::conduct::{Reasoners, Role},
    Tuning,
};
use nalgebra::Vector3;
use std::{hint::black_box, time::Instant};

fn main() {
    traffic();
    patrols();
    flights();
    orchestrated();
}

fn orchestrated() {
    const SIDE: usize = 160;
    let mut grid = WalkGrid::new(SIDE, SIDE, 1.0);
    for z in 0..SIDE {
        for x in 0..SIDE {
            let (rx, rz) = (x % 20, z % 20);
            let wall = (rx == 19 && !(8..12).contains(&rz)) || (rz == 19 && !(8..12).contains(&rx));
            grid.set(x, z, !wall);
        }
    }
    let mesh = NavMesh::build(&grid, Vector3::zeros(), |_, _| 0);
    let ai = Tuning::DEFAULT;
    // The player's loop: round the middle of the level, through the doorways.
    let lap = [(10.0, 10.0), (150.0, 10.0), (150.0, 150.0), (10.0, 150.0)].map(|(x, z)| Vector3::new(x, 0.0, z));
    for count in [10, 50, 100, 250] {
        let mut rng = Rng::new(count as u64);
        let mut squad = Squad::new(&ai);
        let mut reasoners = Reasoners::default();
        let mut sentries: Vec<Sentry> = (0..count).map(|_| Sentry::new(Role::Sentry)).collect();
        let mut bodies: Vec<(Vector3<f32>, f32)> = (0..count)
            .map(|_| {
                let room = |r: usize| (r * 20 + 10) as f32;
                (Vector3::new(room(rng.below(8)) + rng.below(9) as f32 - 4.0, 0.0, room(rng.below(8)) + rng.below(9) as f32 - 4.0), rng.below(628) as f32 / 100.0)
            })
            .collect();
        let (mut player, mut leg) = (lap[0], 1);
        let (mut total, mut worst, mut frames) = (0.0f64, 0.0f64, 0);
        let (mut spotted, mut searching) = (0, 0);
        let dt: f32 = 1.0 / 60.0;
        for _ in 0..3600 {
            // The player walks the loop.
            let to = flat(lap[leg] - player);
            if to.norm() < 0.1 {
                leg = (leg + 1) % lap.len();
            } else {
                player += to / to.norm() * (3.0 * dt).min(to.norm());
            }
            let senses: Vec<Senses> = bodies
                .iter()
                .zip(&sentries)
                .map(|(&(feet, heading), s)| {
                    let near = flat(player - feet).norm() < 35.0;
                    let visible = if near && mesh.sees_across(feet, player) {
                        Sight::DEFAULT.visibility(s.alert(), feet, heading, player, Stance::Standing, false)
                    } else {
                        0.0
                    };
                    Senses { feet, heading, visible, player }
                })
                .collect();
            let t = Instant::now();
            let thoughts = squad.step(&ai, &mesh, &mut reasoners, &mut sentries, black_box(&senses), false, |a, b| flat(bodies[a].0 - bodies[b].0).norm() < 30.0, dt);
            let took = t.elapsed().as_secs_f64() * 1e3;
            total += took;
            worst = worst.max(took);
            frames += 1;
            for (body, thought) in bodies.iter_mut().zip(&thoughts) {
                match thought.intent {
                    Intent::GoTo { at, pace } => {
                        let speed = match pace {
                            Pace::Walk => 1.5,
                            Pace::Search => 2.5,
                            Pace::Run => 4.0,
                        };
                        let to = flat(at - body.0);
                        if to.norm() > 1.0e-3 {
                            body.1 = to.x.atan2(to.z);
                            body.0 += to / to.norm() * (speed * dt).min(to.norm());
                        }
                    }
                    Intent::Face { heading } => body.1 = heading,
                    Intent::Routine => (),
                }
            }
            spotted += sentries.iter().filter(|s| s.alert() == Some(hydroxus_ai::alert::Alert::Alert)).count();
            searching += sentries.iter().filter(|s| s.alert() == Some(hydroxus_ai::alert::Alert::Evasion)).count();
        }
        println!(
            "orchestrated: {count} sentries, a frame {:.4} ms (worst {:.3} ms), {:.4} ms a sentry; on average {:.1} chasing and {:.1} searching",
            total / frames as f64,
            worst,
            total / frames as f64 / count as f64,
            spotted as f64 / frames as f64,
            searching as f64 / frames as f64
        );
    }
}

/// The time each of `runs` takes, as `(mean, worst)` in milliseconds.
fn time(runs: usize, mut run: impl FnMut()) -> (f64, f64) {
    let (mut total, mut worst) = (0.0f64, 0.0f64);
    for _ in 0..runs {
        let t = Instant::now();
        run();
        let took = t.elapsed().as_secs_f64() * 1e3;
        total += took;
        worst = worst.max(took);
    }
    (total / runs as f64, worst)
}

fn traffic() {
    let (n, apart) = (21, 64.0);
    let span = (n - 1) as f32 * apart;
    let mut roads = Vec::new();
    for k in 0..n {
        let c = k as f32 * apart;
        roads.push([c - 8.0, -8.0, c + 8.0, span + 8.0]);
        roads.push([-8.0, c - 8.0, span + 8.0, c + 8.0]);
    }
    let t = Instant::now();
    let mut net = Network::from_roads(&roads);
    let built = t.elapsed().as_secs_f64() * 1e3;
    for j in &mut net.junctions {
        j.lit = j.at.x % 128.0 == 0.0 && j.at.z % 128.0 == 0.0;
    }
    let mut rng = Rng::new(7);
    let mut pick = || rng.below(1 << 16) as f32 / (1 << 16) as f32;
    let mut traffic = Traffic::new(net, 64, Vector3::new(-500.0, 0.0, -500.0), &mut pick);
    let people: Vec<Vector3<f32>> =
        (0..200).map(|k| Vector3::new((k * 37 % 1280) as f32, 0.0, (k * 91 % 1280) as f32)).collect();
    let (mean, worst) = time(3600, || traffic.step(1.0 / 60.0, black_box(&people)));
    println!(
        "traffic: {} junctions, {} bikes; network built in {built:.2} ms; a step {:.4} ms (worst {:.3} ms)",
        traffic.network.junctions.len(),
        traffic.riders.len(),
        mean,
        worst
    );
}

/// Open, flat ground: routes straight there, trips 10 m on.
struct Open(Rng);

impl patrol::Ways for Open {
    fn route_to(&mut self, to: Vector3<f32>) -> Vec<Vector3<f32>> {
        vec![to]
    }
    fn trip(&mut self, under: Vector3<f32>, _: (f32, f32)) -> Vec<Vector3<f32>> {
        let a = self.0.below(628) as f32 / 100.0;
        vec![under + Vector3::new(a.sin(), 0.0, a.cos()) * 10.0]
    }
    fn spot_near(&mut self, post: Vector3<f32>, _: f32) -> Option<Vector3<f32>> {
        Some(post)
    }
    fn between(&mut self, range: (f32, f32)) -> f32 {
        range.0 + (range.1 - range.0) * self.0.below(1000) as f32 / 1000.0
    }
}

impl patrol::Air for Open {
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

fn patrols() {
    let speeds = Speeds { patrol: 1.4, hurry: 3.5, acceleration: 3.0, turn_rate: 3.0 };
    let mut drones: Vec<Patrol> = (0..50)
        .map(|k| {
            let mut drone = Patrol::new(1.7, k % 2 == 0, speeds, 4.0);
            drone.put_at(Vector3::new(k as f32 * 3.0, 0.0, 0.0), 0.0, 1.0);
            if k % 2 == 1 {
                drone.search(Vector3::new(k as f32 * 3.0, 0.0, 20.0));
            }
            drone
        })
        .collect();
    let mut ways = Open(Rng::new(3));
    let player = Vector3::new(500.0, 0.0, 500.0);
    let (mean, worst) = time(3600, || {
        for drone in &mut drones {
            black_box(drone.think(false, player, &mut ways, 1.0 / 60.0));
            drone.fly(player, &Open(Rng::new(0)), 1.0 / 60.0);
        }
    });
    println!("patrol: 50 drones, a frame {:.4} ms (worst {:.3} ms)", mean, worst);
}

fn flights() {
    // A block of 40 buildings, 10 m square and 8 to 30 m high, 20 m apart, and the ground.
    let mut boxes: Vec<(Vector3<f32>, Vector3<f32>)> = vec![(Vector3::new(100.0, -0.5, 80.0), Vector3::new(200.0, 0.5, 200.0))];
    for k in 0..40 {
        let (x, z) = ((k % 8) as f32 * 20.0 + 10.0, (k / 8) as f32 * 20.0 + 10.0);
        let h = 8.0 + (k * 7 % 23) as f32;
        boxes.push((Vector3::new(x, h / 2.0, z), Vector3::new(5.0, h / 2.0, 5.0)));
    }
    // Whether a ball `radius` across flies from `a` to `b` clear of every box: the segment
    // against each box grown by the radius (the slab test).
    let clear = |a: Vector3<f32>, b: Vector3<f32>, radius: f32| {
        let d = b - a;
        boxes.iter().all(|&(at, half)| {
            let (lo, hi) = (at - half.add_scalar(radius), at + half.add_scalar(radius));
            let (mut t0, mut t1) = (0.0f32, 1.0f32);
            for i in 0..3 {
                if d[i].abs() < 1e-6 {
                    if a[i] < lo[i] || a[i] > hi[i] {
                        return true;
                    }
                } else {
                    let (u, v) = ((lo[i] - a[i]) / d[i], (hi[i] - a[i]) / d[i]);
                    t0 = t0.max(u.min(v));
                    t1 = t1.min(u.max(v));
                    if t0 > t1 {
                        return true;
                    }
                }
            }
            false
        })
    };
    let mut rng = Rng::new(11);
    let mut spot = || {
        // In the streets between the buildings, at hover height.
        let (x, z) = (rng.below(8) as f32 * 20.0, rng.below(5) as f32 * 20.0);
        Vector3::new(x, 1.7, z)
    };
    let pairs: Vec<_> = (0..2000).map(|_| (spot(), spot())).collect();
    let mut legs = 0;
    let t = Instant::now();
    for &(from, to) in &pairs {
        let ground = || vec![to - Vector3::new(0.0, 1.7, 0.0), (from + to) / 2.0 - Vector3::new(0.0, 1.7, 0.0)];
        legs += black_box(flight::flight(&flight::Tuning::DEFAULT, &clear, from, to, 1.7, ground)).len();
    }
    let took = t.elapsed().as_secs_f64() * 1e3;
    println!(
        "flight: 2000 routes in {took:.2} ms, {:.4} ms each, {:.1} points each",
        took / 2000.0,
        legs as f64 / 2000.0
    );
}
