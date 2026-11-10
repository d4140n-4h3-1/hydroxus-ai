//! Memory brought to bear where it matters (ROADMAP 3.3): a searcher goes first where it has
//! found the player before - wherever the search has spread to, not only where they were lost.

use hydroxus_ai::{
    navigation::{grid::WalkGrid, navmesh::NavMesh},
    tactics::search::SearchMap,
    think::haunt::Haunts,
    Tuning,
};
use nalgebra::Vector3;

const AI: Tuning = Tuning::DEFAULT;

/// A corridor 80 m long, 2 wide, in 0.5 m cells: polygons of 8 m.
fn corridor() -> NavMesh {
    let mut grid = WalkGrid::new(160, 4, 0.5);
    for x in 0..160 {
        for z in 0..4 {
            grid.set(x, z, true);
        }
    }
    NavMesh::build(&grid, Vector3::zeros(), |_, _| 0)
}

#[test]
fn a_haunt_further_off_than_where_they_were_lost_counts_when_choosing_where_to_search() {
    let mesh = corridor();
    let (lost, haunt) = (Vector3::new(40.0, 0.0, 1.0), Vector3::new(60.0, 0.0, 1.0));
    let mut haunts = Haunts::new();
    for _ in 0..5 {
        haunts.found_at(&AI.haunt, &mesh, haunt);
    }
    let spread = |lean: bool| {
        let mut map = SearchMap::with(AI.search);
        map.lose(&mesh, lost, None);
        if lean {
            haunts.lean(&AI.haunt, &mesh, &mut map);
        }
        for _ in 0..120 {
            map.spread(&mesh, 0.25);
        }
        map
    };
    // Leaning the map just after it is set going weighs only where they were lost: two polygons
    // off, the haunt's chance is as if it had never been leant.
    let (plain, leant) = (spread(false), spread(true));
    assert!(plain.chance_at(&mesh, haunt) > 0.0);
    let (a, b) = (plain.chance_at(&mesh, haunt), leant.chance_at(&mesh, haunt));
    assert!((a - b).abs() <= a * 1.0e-5, "{a} {b}");
    // Weighing the choice does count it. Lost at 52 m and looked about there: the polygons either
    // side are as likely as each other, so a searcher there goes to one of them - and with a haunt
    // on the other side, to that one.
    let mut map = SearchMap::with(AI.search);
    let middle = Vector3::new(52.0, 0.0, 1.0);
    map.lose(&mesh, middle, None);
    for _ in 0..40 {
        map.spread(&mesh, 0.25);
    }
    map.look(&mesh, middle, 0.0, (4.5, std::f32::consts::PI, 4.5));
    // Another searcher is seeing to where it stands: it goes either side.
    let taken = [middle];
    let plain_choice = map.best(&mesh, middle, &taken, 4.0, 100.0).unwrap();
    let other_side = Vector3::new(104.0 - plain_choice.x, 0.0, 1.0);
    let mut haunts = Haunts::new();
    for _ in 0..5 {
        haunts.found_at(&AI.haunt, &mesh, other_side);
    }
    let weighed = map.best_by(&mesh, middle, &taken, 4.0, 100.0, haunts.weigh(&AI.haunt, &mesh)).unwrap();
    assert!((plain_choice - middle).norm() > 4.0, "a neighbour, not where it stands: {plain_choice:?}");
    assert!((weighed - other_side).norm() < 4.0, "plain {plain_choice:?}, weighed {weighed:?}, haunt {other_side:?}");
    let (from, plain) = (middle, map);
    let plain_choice = plain.best(&mesh, from, &[], 4.0, 100.0).unwrap();
    // With no haunts, it weighs nothing.
    assert_eq!(plain.best_by(&mesh, from, &[], 4.0, 100.0, Haunts::new().weigh(&AI.haunt, &mesh)), Some(plain_choice));
}
