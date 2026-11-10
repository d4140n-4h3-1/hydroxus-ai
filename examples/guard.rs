//! Two guards run by the orchestration layer, printed as they go: the least a game has to do to
//! use `hydroxus_ai::orchestrate`. `cargo run --example guard`.
//!
//! A corridor with a room off it. One guard walks a beat along the corridor; the other stands at
//! the far end, facing it. The player stands in the corridor, is spotted, runs into the room and
//! out of sight, and is gone. Each frame the "game" here does its three parts: it senses (a
//! sight-line along the floor and `Sight::visibility`), steps the squad, and carries out each
//! guard's intent - routing with the navmesh and walking at the pace it says.
use hydroxus_ai::{
    flat, heading_of,
    navigation::{grid::WalkGrid, navmesh::NavMesh, walk::route_to},
    orchestrate::{Intent, Pace, Senses, Sentry, Squad},
    perception::sight::{Sight, Stance},
    tactics::conduct::{Reasoners, Role},
    Tuning,
};
use nalgebra::Vector3;

fn at(x: f32, z: f32) -> Vector3<f32> {
    Vector3::new(x, 0.0, z)
}

fn main() {
    let ai = Tuning::DEFAULT;
    if let Err(invalid) = ai.validate() {
        panic!("{invalid}");
    }
    // The level: a corridor 40 m by 4 along +x, a doorway north at x 29..31 into a room.
    let mut grid = WalkGrid::new(40, 18, 1.0);
    for z in 0..18 {
        for x in 0..40 {
            grid.set(x, z, z < 4 || ((4..8).contains(&z) && (29..31).contains(&x)) || ((8..18).contains(&z) && (24..36).contains(&x)));
        }
    }
    let mesh = NavMesh::build(&grid, Vector3::zeros(), |_, _| 0);

    let mut squad = Squad::new(&ai);
    let mut reasoners = Reasoners::default();
    let mut guards = vec![Sentry::new(Role::Sentry).with_beat(vec![at(3.0, 2.0), at(14.0, 2.0)]), Sentry::new(Role::Sentry)];
    // The game's part of each guard: where its body is, which way it faces, its route.
    let mut bodies = [(at(8.0, 2.0), std::f32::consts::FRAC_PI_2, Vec::new()), (at(38.0, 2.0), -std::f32::consts::FRAC_PI_2, Vec::new())];
    let mut player = at(20.0, 2.0);
    let mut path: Vec<Vector3<f32>> = Vec::new();

    let dt = 1.0 / 60.0;
    for frame in 0..(150 * 60) {
        let t = frame as f32 * dt;
        // The player, once seen, runs for the room, and four seconds after is gone for good.
        if t >= 3.0 && t < 3.0 + dt {
            path = vec![at(30.0, 2.0), at(30.0, 9.0), at(35.5, 17.5)];
        }
        if t >= 7.0 && t < 7.0 + dt {
            path.clear();
            player = at(500.0, 500.0);
        }
        if let Some(&next) = path.first() {
            let to = flat(next - player);
            if to.norm() <= 5.5 * dt {
                player = path.remove(0);
            } else {
                player += to / to.norm() * 5.5 * dt;
            }
        }

        // 1. Sense.
        let senses: Vec<Senses> = bodies
            .iter()
            .zip(&guards)
            .map(|((feet, heading, _), guard)| {
                let visible = if mesh.sees_across(*feet, player) {
                    Sight::DEFAULT.visibility(guard.alert(), *feet, *heading, player, Stance::Standing, false)
                } else {
                    0.0
                };
                Senses { feet: *feet, heading: *heading, visible, player, threat: 0.0 }
            })
            .collect();
        // 2. Think.
        let feet: Vec<_> = bodies.iter().map(|b| b.0).collect();
        let thoughts = squad.step(&ai, &mesh, &mut reasoners, &mut guards, &senses, false, |a, b| mesh.sees_across(feet[a], feet[b]), dt);
        // 3. Act.
        for (n, (thought, body)) in thoughts.iter().zip(bodies.iter_mut()).enumerate() {
            for happened in &thought.happened {
                println!("{t:6.2} s  guard {n}: {happened:?} - now {:?}, {:?}", guards[n].alert(), thought.conduct);
            }
            let (feet, heading, route) = body;
            match thought.intent {
                Intent::Routine => route.clear(),
                Intent::Face { heading: h } => {
                    route.clear();
                    *heading = h;
                }
                Intent::GoTo { at, pace } => {
                    if route.first().is_none_or(|&end: &Vector3<f32>| flat(end - at).norm() > 0.5) {
                        *route = route_to(&mesh, *feet, at, f32::INFINITY);
                        if route.is_empty() && flat(at - *feet).norm() > ai.orchestrate.arrived {
                            guards[n].no_way(&ai);
                        }
                    }
                    let mut left = match pace {
                        Pace::Walk => 1.5,
                        Pace::Search => 2.5,
                        Pace::Run => 4.0,
                    } * dt;
                    while let Some(&next) = route.last() {
                        let to = flat(next - *feet);
                        if let Some(h) = heading_of(to) {
                            *heading = h;
                        }
                        if to.norm() > left {
                            *feet += to / to.norm() * left;
                            break;
                        }
                        left -= to.norm();
                        *feet = next;
                        route.pop();
                    }
                }
            }
        }
    }
    for (n, guard) in guards.iter().enumerate() {
        println!(
            "after 150 s, guard {n}: {:?}, wariness {:.2}, at ({:.1}, {:.1}); learned {:?}",
            guard.alert(),
            guard.wariness.level,
            bodies[n].0.x,
            bodies[n].0.z,
            guard.experience.lessons.learned().collect::<Vec<_>>()
        );
    }
}
