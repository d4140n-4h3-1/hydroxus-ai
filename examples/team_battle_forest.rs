//! A fixed-step forest match with five AI droids per side and no renderer.
//! The flat map is a schematic of the forest arena, with a broken hangar and
//! mirrored ruins and cover. Run with `cargo run --release --example team_battle_forest`.

use hydroxus_ai::{
    alert::Alert,
    heading_of,
    navigation::{
        grid::{Rng, WalkGrid},
        ground::{self, Step},
        navmesh::NavMesh,
        walk::route_to,
    },
    perception::{
        detection::Detection,
        hearing::Heard,
        sight::{Fog, Seeing, Stance},
    },
    tactics::{
        fire,
        team::{self, Intel},
    },
    Tuning,
};
use nalgebra::Vector3;
use std::{env, error::Error};

type Point = Vector3<f32>;
const PER_TEAM: usize = 5;
const COUNT: usize = 2 * PER_TEAM;
const TEAMS: [&str; 2] = ["red", "cyan"];
const HZ: u32 = 20;
const DT: f32 = 1.0 / HZ as f32;
const LIMIT: u32 = 30;
const RESPAWN: u32 = 5 * HZ;
const SIGHT: f32 = 45.0;
const AI: Tuning = Tuning::DEFAULT;
const FOREST_AIR: Seeing = Seeing {
    dark: false,
    fog: Some(Fog {
        start: 8.0,
        density: 0.045,
        base_height: 0.0,
        height_falloff: 0.0,
    }),
    looking_up: false,
};

fn point(x: f32, z: f32) -> Point {
    Point::new(x, 0.0, z)
}

fn arena() -> (NavMesh, Vec<Point>) {
    let mut grid = WalkGrid::new(112, 64, 1.0);
    let mut cover = Vec::new();
    for z in 0..64 {
        for x in 0..112 {
            let side = x.min(111 - x);
            let blocked = match z {
                9..=16 => (22..=30).contains(&side),
                24..=28 | 37..=40 => (48..=55).contains(&side),
                31..=34 => (35..=37).contains(&side),
                45..=51 => (24..=25).contains(&side) || (40..=43).contains(&side),
                53..=54 => (33..=37).contains(&side),
                _ => false,
            };
            grid.set(x, z, !blocked);
        }
    }
    for z in (6..60).step_by(3) {
        for x in (8..105).step_by(3) {
            if grid.is_walkable(x, z) {
                cover.push(point(x as f32 + 0.5, z as f32 + 0.5));
            }
        }
    }
    (NavMesh::build(&grid, Point::zeros(), |_, _| 0), cover)
}

#[derive(Clone, Copy)]
struct Body {
    at: Point,
    heading: f32,
    health: u8,
    target: Option<usize>,
}

struct Droid {
    body: Body,
    awareness: [Detection; COUNT],
    route: Vec<Point>,
    ready: u32,
    respawn: u32,
}

impl Droid {
    fn spawn(id: usize) -> Self {
        let side = id / PER_TEAM;
        Self {
            body: Body {
                at: point(if side == 0 { 6.5 } else { 105.5 }, lane(id)),
                heading: if side == 0 { 1.57 } else { -1.57 },
                health: 100,
                target: None,
            },
            awareness: [Detection::default(); COUNT],
            route: Vec::new(),
            ready: 0,
            respawn: 0,
        }
    }
}

fn lane(id: usize) -> f32 {
    12.5 + (id % PER_TEAM) as f32 * 10.0
}

fn covering_point(mesh: &NavMesh, cover: &[Point], body: Body, threat: Point) -> Option<Point> {
    let mut candidates: Vec<_> = cover
        .iter()
        .copied()
        .enumerate()
        .filter_map(|(index, at)| {
            let distance = (at - body.at).norm();
            if distance > 15.0 {
                return None;
            }
            team::cover_rank(distance, (at - threat).norm(), true).map(|rank| (rank, index, at))
        })
        .collect();
    // Keep the original scan's tie order. Once a spot blocks sight, the rest rank lower.
    candidates.sort_unstable_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    candidates
        .into_iter()
        .map(|(_, _, at)| at)
        .find(|&at| !mesh.sees_across(at, threat))
}

struct Shot {
    shooter: usize,
    target: usize,
    damage: u8,
}

fn tick(
    frame: u32,
    mesh: &NavMesh,
    cover: &[Point],
    droids: &mut [Droid; COUNT],
    intel: &mut [Intel; 2],
    rng: &mut Rng,
    score: &mut [u32; 2],
) -> u32 {
    for id in 0..COUNT {
        if droids[id].body.health == 0 && frame >= droids[id].respawn {
            let fresh = Droid::spawn(id);
            if droids
                .iter()
                .filter(|other| other.body.health > 0)
                .all(|other| (other.body.at - fresh.body.at).norm() >= ground::CLEARANCE)
            {
                droids[id] = fresh;
            }
        }
    }
    for knowledge in intel.iter_mut() {
        knowledge.age(DT);
    }
    let bodies: [Body; COUNT] = std::array::from_fn(|id| droids[id].body);
    let mut positions = bodies.map(|body| body.at);
    let mut shots = Vec::new();
    for (id, droid) in droids.iter_mut().enumerate() {
        let body = bodies[id];
        if body.health == 0 {
            continue;
        }
        let side = id / PER_TEAM;
        let mut visible = Vec::new();
        for (enemy, other) in bodies.iter().enumerate() {
            if enemy / PER_TEAM == side || other.health == 0 {
                continue;
            }
            let clear = (other.at - body.at).norm() <= SIGHT && mesh.sees_across(body.at, other.at);
            let visibility = if clear {
                AI.sight.visibility(
                    body.target.map(|_| Alert::Alert),
                    body.at,
                    body.heading,
                    other.at,
                    Stance::Standing,
                    FOREST_AIR,
                )
            } else {
                0.0
            };
            let awareness = &mut droid.awareness[enemy];
            awareness.step(&AI.detection, visibility, 1.0, DT);
            if clear && awareness.spotted() {
                visible.push(enemy);
                intel[side].saw(enemy, other.at);
            }
        }
        droid.body.target = team::target(&visible, body.target, |enemy| {
            let other = bodies[enemy];
            team::target_worth(
                (other.at - body.at).norm(),
                other.health as f32 / 100.0,
                other.target == Some(id),
                true,
            )
        });

        if let Some(enemy) = droid.body.target {
            let distance = (bodies[enemy].at - body.at).norm();
            if frame >= droid.ready
                && fire::facing(&AI.fire, body.at, body.heading, bodies[enemy].at)
            {
                droid.ready = frame + HZ / 2 + rng.below(6) as u32;
                let hit_chance = (88.0 - distance * 1.5).clamp(15.0, 88.0) as usize;
                let damage = if rng.below(100) < hit_chance {
                    9 + rng.below(8) as u8
                } else {
                    0
                };
                shots.push(Shot {
                    shooter: id,
                    target: enemy,
                    damage,
                });
            }
            if body.health > 35 && distance < 23.0 {
                droid.route.clear();
                if let Some(heading) = heading_of(bodies[enemy].at - body.at) {
                    droid.body.heading = heading;
                }
                continue;
            }
        }

        if frame % HZ == id as u32 {
            let alive = |enemy: usize| bodies[enemy].health > 0;
            let threat = intel[side].nearest(&AI.team, body.at, alive);
            let advance = point(if side == 0 { 96.5 } else { 15.5 }, lane(id));
            let goal = match threat {
                Some(at) if body.health <= 35 => covering_point(mesh, cover, body, at)
                    .unwrap_or_else(|| Droid::spawn(id).body.at),
                Some(at) if id % PER_TEAM >= 3 => {
                    let mates: Vec<_> = bodies
                        .iter()
                        .enumerate()
                        .filter(|&(i, mate)| i / PER_TEAM == side && mate.health > 0)
                        .map(|(_, mate)| mate.at)
                        .collect();
                    let flank = if id % PER_TEAM == 3 { -1.0 } else { 1.0 };
                    team::flank_point(&AI.team, mesh, body.at, &mates, at, flank).unwrap_or(at)
                }
                Some(at) => at,
                None => advance,
            };
            droid.route = route_to(mesh, body.at, goal, f32::INFINITY);
        }
        let mut neighbors = [Point::zeros(); COUNT - 1];
        let mut count = 0;
        for (other, &at) in positions.iter().enumerate() {
            if other != id && bodies[other].health > 0 {
                neighbors[count] = at;
                count += 1;
            }
        }
        match ground::step(
            mesh,
            &AI,
            body.at,
            &mut droid.route,
            3.5 * DT,
            &neighbors[..count],
            id as u32,
        ) {
            Step::Move(at) => {
                droid.body.at = at;
                positions[id] = at;
                if let Some(heading) = heading_of(at - body.at) {
                    droid.body.heading = heading;
                }
            }
            Step::Replan => droid.route.clear(),
            Step::Waiting | Step::Arrived => {}
        }
    }

    // Apply shots after all droids have acted. A droid downed this tick still fires its shot.
    for shot in &shots {
        let side = shot.shooter / PER_TEAM;
        let target = &mut droids[shot.target];
        if shot.damage > 0 && target.body.health > 0 {
            target.body.health = target.body.health.saturating_sub(shot.damage);
            if target.body.health == 0 {
                target.respawn = frame + RESPAWN;
                score[side] += 1;
                intel[side].forget(shot.target);
                for mate in droids.iter_mut().skip(side * PER_TEAM).take(PER_TEAM) {
                    mate.awareness[shot.target].reset();
                }
            }
        }
        // A shot already made still lands, but it cannot refresh intel on a downed shooter.
        if droids[shot.shooter].body.health == 0 {
            continue;
        }
        let origin = bodies[shot.shooter].at;
        if let Some(noise) = Heard::at(mesh, origin, 35.0) {
            let other_side = 1 - side;
            for listener in droids.iter_mut().skip(other_side * PER_TEAM).take(PER_TEAM) {
                if listener.body.health > 0 {
                    if let Some(nearness) = noise.nearness(mesh, listener.body.at) {
                        listener.awareness[shot.shooter].hear(&AI.detection, nearness, 1.0);
                        intel[other_side].saw(shot.shooter, origin);
                    }
                }
            }
        }
    }
    shots.len() as u32
}

fn main() -> Result<(), Box<dyn Error>> {
    let seed = env::var("BATTLE_SEED")
        .unwrap_or_else(|_| "1".into())
        .parse::<u64>()?;
    let seconds = env::var("BATTLE_BENCH")
        .unwrap_or_else(|_| "240".into())
        .parse::<u32>()?;
    if !(1..=3600).contains(&seconds) {
        return Err("BATTLE_BENCH must be between 1 and 3600 seconds".into());
    }
    let (mesh, cover) = arena();
    let mut rng = Rng::new(seed);
    let mut droids = std::array::from_fn(Droid::spawn);
    let mut intel = [Intel::new(COUNT), Intel::new(COUNT)];
    let mut score = [0, 0];
    let mut shots = 0;
    let mut ticks = 0;
    println!("Forest team battle | seed {seed} | {PER_TEAM} vs {PER_TEAM} | {HZ} Hz");
    for frame in 0..seconds * HZ {
        let previous_score = score;
        shots += tick(
            frame,
            &mesh,
            &cover,
            &mut droids,
            &mut intel,
            &mut rng,
            &mut score,
        );
        ticks = frame + 1;
        if score != previous_score {
            println!(
                "{:6.2} s | red {} - {} cyan",
                ticks as f32 * DT,
                score[0],
                score[1]
            );
        }
        if score.iter().any(|&kills| kills >= LIMIT) {
            break;
        }
    }
    let result = match score[0].cmp(&score[1]) {
        std::cmp::Ordering::Greater => TEAMS[0],
        std::cmp::Ordering::Less => TEAMS[1],
        std::cmp::Ordering::Equal => "draw",
    };
    let ended = if score.iter().any(|&kills| kills >= LIMIT) {
        "kill limit"
    } else {
        "time limit"
    };
    println!(
        "Result: {result} | red {} - {} cyan | {shots} shots | {:.2} s | {ended}",
        score[0],
        score[1],
        ticks as f32 * DT
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walkers_stay_on_the_floor_and_keep_clear_of_one_another() {
        let (mesh, cover) = arena();
        let mut droids = std::array::from_fn(Droid::spawn);
        let mut intel = [Intel::new(COUNT), Intel::new(COUNT)];
        let mut rng = Rng::new(1);
        let mut score = [0, 0];
        for frame in 0..240 * HZ {
            tick(
                frame,
                &mesh,
                &cover,
                &mut droids,
                &mut intel,
                &mut rng,
                &mut score,
            );
            for (id, droid) in droids
                .iter()
                .enumerate()
                .filter(|(_, droid)| droid.body.health > 0)
            {
                assert!(
                    mesh.poly_at(droid.body.at).is_some(),
                    "droid {id} off floor at tick {frame}"
                );
                for other in droids
                    .iter()
                    .skip(id + 1)
                    .filter(|other| other.body.health > 0)
                {
                    assert!(
                        (droid.body.at - other.body.at).norm() >= ground::CLEARANCE - 1.0e-5,
                        "droid {id} too close to another at tick {frame}"
                    );
                }
            }
        }
        assert!(score.iter().all(|&kills| kills > 0), "{score:?}");
    }
}
