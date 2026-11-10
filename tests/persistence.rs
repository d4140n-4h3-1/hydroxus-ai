//! A guard saved in the middle of an incident, and loaded again, carries on as if it never was:
//! its awareness, its wariness, its alert phase, the investigation it is on, the witness on its
//! way to it, and the search map it is searching by.
#![cfg(feature = "serde")]

use hydroxus_ai::{
    alert::{Alert, Incident, Wariness},
    navigation::{grid::WalkGrid, navmesh::NavMesh},
    perception::detection::Detection,
    social::report::Witness,
    tactics::{investigate::Investigation, look::LookAbout, search::SearchMap, threat::Unease},
    think::experience::{Experience, Outcome, Venture},
    Tuning,
};
use nalgebra::Vector3;
use serde::{Deserialize, Serialize};

const AI: Tuning = Tuning::DEFAULT;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Guard {
    feet: Vector3<f32>,
    heading: f32,
    detection: Detection,
    wariness: Wariness,
    alert: Option<Alert>,
    left: f32,
    investigation: Option<Investigation>,
    looking: Option<LookAbout>,
    unease: Unease,
    witness: Option<Witness>,
    experience: Experience,
}

fn level() -> NavMesh {
    let mut grid = WalkGrid::new(30, 12, 1.0);
    for z in 0..12 {
        for x in 0..30 {
            grid.set(x, z, z < 4 || (12..18).contains(&x));
        }
    }
    NavMesh::build(&grid, Vector3::zeros(), |_, _| 0)
}

/// Moves `guard` and `map` on `dt`, the same way each time: what a game's frame does to them.
fn step(guard: &mut Guard, map: &mut SearchMap, mesh: &NavMesh, dt: f32) {
    guard.detection.step(&AI.detection, 0.0, guard.wariness.keenness(&AI.alert), dt);
    guard.wariness.step(&AI.alert, dt);
    guard.left -= dt;
    guard.alert = guard.alert.and_then(|alert| alert.next(false, guard.left));
    if let Some(look) = guard.investigation.as_mut() {
        look.step(&AI.investigate, guard.feet, guard.heading, dt);
    }
    if let Some(heading) = guard.looking.as_mut().and_then(|look| look.step(dt)) {
        guard.heading = heading;
    }
    guard.unease.step(&AI.threat, true, 6.0, dt);
    if let Some(witness) = guard.witness.as_mut() {
        witness.step(dt, Vector3::new(2.0, 0.0, 2.0), &[(3, Vector3::new(25.0, 0.0, 2.0))]);
    }
    guard.experience.step(&AI, dt);
    map.spread(mesh, 5.0 * dt);
    map.look(mesh, guard.feet, guard.heading, (20.0, 1.0, 2.0));
}

#[test]
fn a_guard_saved_mid_incident_carries_on_as_if_never_saved() {
    let mesh = level();
    let mut guard = Guard {
        feet: Vector3::new(4.5, 0.0, 2.5),
        heading: 1.2,
        detection: Detection::default(),
        wariness: Wariness::default(),
        alert: Some(Alert::Evasion),
        left: 60.0,
        investigation: Some(Investigation::new(&AI.investigate, Vector3::new(15.5, 0.0, 9.5))),
        looking: Some(LookAbout::searching(&AI.search, 1.2)),
        unease: Unease::default(),
        witness: Some(Witness::new(Vector3::new(14.0, 0.0, 8.0), true)),
        experience: Experience::new(),
    };
    guard.detection.alert();
    guard.wariness.note(&AI.alert, Incident::Spotted);
    guard.experience.trail.doing(hydroxus_ai::tactics::conduct::Conduct::Chase);
    guard.experience.report(&AI, Some(&mesh), Outcome::failed(Venture::Pursuit).at(Vector3::new(15.5, 0.0, 9.5)));
    let mut map = SearchMap::with(AI.search);
    map.lose(&mesh, Vector3::new(15.5, 0.0, 6.5), Some(Vector3::new(0.0, 0.0, 1.0)));
    // Into the incident a while.
    for _ in 0..90 {
        step(&mut guard, &mut map, &mesh, 1.0 / 60.0);
    }

    // Saved, and loaded again.
    let saved = serde_json::to_string(&(&guard, &map)).unwrap();
    let (mut loaded, mut loaded_map): (Guard, SearchMap) = serde_json::from_str(&saved).unwrap();
    assert_eq!(loaded, guard);

    // Both carry on, side by side, for half a minute.
    for frame in 0..1800 {
        step(&mut guard, &mut map, &mesh, 1.0 / 60.0);
        step(&mut loaded, &mut loaded_map, &mesh, 1.0 / 60.0);
        assert_eq!(loaded, guard, "frame {frame}");
        // A search map's chances, summed in its hash map's order, may differ in the last place.
        for n in 0..mesh.len() as u32 {
            assert!((map.chance(n) - loaded_map.chance(n)).abs() < 1.0e-5, "frame {frame}, polygon {n}");
        }
    }
    assert!(guard.alert.is_some() && guard.wariness.level > 0.0, "still mid-incident: {guard:?}");
}
