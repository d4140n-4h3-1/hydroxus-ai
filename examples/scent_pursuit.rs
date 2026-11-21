//! Run with `cargo run --locked --example scent_pursuit`.

use hydroxus_ai::{
    heading_of,
    navigation::{grid::WalkGrid, navmesh::NavMesh, walk::route_to},
    orchestrate::{Happened, Intent, Pace, Senses, Sentry, Squad},
    perception::{
        sight::Stance,
        smell::{Nose, Trail},
    },
    tactics::conduct::{Reasoners, Role},
    Tuning,
};
use nalgebra::Vector3;

const AI: Tuning = Tuning::DEFAULT;
const DT: f32 = 0.1;
type Point = Vector3<f32>;

fn point(x: f32, z: f32) -> Point {
    Point::new(x, 0.0, z)
}

fn corridor() -> NavMesh {
    let mut grid = WalkGrid::new(12, 10, 1.0);
    for z in 0..10 {
        for x in 0..12 {
            grid.set(x, z, z < 3 || x >= 9);
        }
    }
    NavMesh::build(&grid, Point::zeros(), |_, _| 0)
}

fn move_toward(feet: &mut Point, goal: Point, distance: f32) {
    let offset = goal - *feet;
    let remaining = offset.norm();
    if remaining <= distance {
        *feet = goal;
    } else {
        *feet += offset * (distance / remaining);
    }
}

fn main() {
    let mesh = corridor();
    let mut trail = Trail::new(64);
    // The convict crosses the hall, turns north, then hides. Each half-meter
    // step leaves a sample. Only the guards' sight and the capture check use
    // the convict's current position after this setup.
    let mut steps = (0..17)
        .map(|i| point(2.5 + i as f32 * 0.5, 1.5))
        .chain((1..=14).map(|i| point(10.5, 1.5 + i as f32 * 0.5)));
    let mut convict = steps.next().unwrap();
    trail.lay(convict, 0.0, 1.0);
    for (i, step) in steps.enumerate() {
        convict = step;
        trail.lay(convict, (i + 1) as f32 * 0.2, 1.0);
    }

    let mut dog = point(2.0, 1.5);
    let mut guards = [point(1.0, 0.5), point(1.0, 1.5), point(1.0, 2.5)];
    let mut headings = [std::f32::consts::FRAC_PI_2; 3];
    assert!(guards.iter().all(|&feet| !mesh.sees_across(feet, convict)));
    let mut sentries = guards.map(|_| Sentry::new(Role::Sentry));
    let mut squad = Squad::new(&AI);
    let mut reasoners = Reasoners::default();
    let nose = Nose::default();
    let mut reached = None;
    let mut tracking = false;
    let mut barked = false;
    let mut investigating = [false; 3];

    for frame in 0..400 {
        let now = 7.0 + frame as f32 * DT;
        if !barked {
            if let Some(scent) = trail.sniff(&nose, dog, now, reached, |sample, feet| {
                mesh.sees_across(sample, feet)
            }) {
                if !tracking {
                    println!("{now:4.1}s  The dog picks up the trail.");
                }
                tracking = true;
                move_toward(&mut dog, scent.at, 3.5 * DT);
                if dog == scent.at {
                    reached = Some(scent.laid_at);
                }
            } else if tracking {
                // On reaching the end of the trail, the dog signals the guards.
                // In this example the freshest trail ends at the hiding place.
                barked = true;
                println!("{now:4.1}s  The dog barks at the end of the trail.");
                squad.noise(&AI, &mesh, &mut sentries, dog, 60.0);
            }
        }

        let senses = std::array::from_fn::<_, 3, _>(|i| {
            let feet = guards[i];
            let heading = headings[i];
            let visible = if mesh.sees_across(feet, convict) {
                AI.sight.visibility(
                    sentries[i].alert(),
                    feet,
                    heading,
                    convict,
                    Stance::Standing,
                    false,
                )
            } else {
                0.0
            };
            Senses {
                feet,
                heading,
                visible,
                player: convict,
                threat: 0.0,
            }
        });
        let thoughts = squad.step_silent(
            &AI,
            &mesh,
            &mut reasoners,
            &mut sentries,
            &senses,
            false,
            |a, b| mesh.sees_across(guards[a], guards[b]),
            DT,
        );
        for (i, thought) in thoughts.into_iter().enumerate() {
            if thought.happened.contains(&Happened::Noticed) && !investigating[i] {
                investigating[i] = true;
                println!("{now:4.1}s  Guard {} investigates the bark.", i + 1);
            }
            match thought.intent {
                Intent::GoTo { at, pace } => {
                    let route = route_to(&mesh, guards[i], at, f32::INFINITY);
                    if let Some(&next) = route.last() {
                        headings[i] = heading_of(next - guards[i]).unwrap_or(headings[i]);
                        let speed = if pace == Pace::Run { 4.0 } else { 2.0 };
                        move_toward(&mut guards[i], next, speed * DT);
                    }
                }
                Intent::Face { heading } => headings[i] = heading,
                Intent::Routine => {}
            }
            if sentries[i].alert() == Some(hydroxus_ai::alert::Alert::Alert)
                && (guards[i] - convict).norm() < 1.0
            {
                assert!(barked && investigating.iter().all(|heard| *heard));
                println!("{now:4.1}s  Guard {} catches the convict.", i + 1);
                return;
            }
        }
    }

    panic!("the pursuit timed out");
}
