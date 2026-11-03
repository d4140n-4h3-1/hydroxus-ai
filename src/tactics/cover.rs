//! Taking cover: a bystander within [`COVER_NEAR`] meters of a shooting runs from it, about
//! [`COVER_RUN`] meters, to somewhere it cannot be seen from if there is anywhere about - trying
//! straight away from it first, and then further and further to either side, [`COVER_TRIES`]
//! ways in all - or else as far off as it can.

use crate::{
    flat, forward,
    navigation::grid::{Rng, WalkGrid},
    navigation::route::spot_near,
};
use nalgebra::Vector3;

pub const COVER_NEAR: f32 = 45.0;
pub const COVER_RUN: f32 = 20.0;
pub const COVER_TRIES: usize = 8;

/// Where one at `feet`, facing `heading`, runs to from a shooting at `danger`, over `grid` whose
/// corner is at `origin`: the first spot further from it that `open` says cannot be seen from it,
/// or else the furthest of them; none if there is no floor to run to.
pub fn take_cover(
    (grid, origin): (&WalkGrid, Vector3<f32>),
    feet: Vector3<f32>,
    heading: f32,
    danger: Vector3<f32>,
    rng: &mut Rng,
    open: impl Fn(Vector3<f32>) -> bool,
) -> Option<Vector3<f32>> {
    let away = flat(feet - danger).try_normalize(1.0e-3).map_or(heading, |away| away.x.atan2(away.z));
    let spots: Vec<Vector3<f32>> = (0..COVER_TRIES)
        .filter_map(|i| {
            let side = if i % 2 == 0 { 1.0 } else { -1.0 };
            let turn = side * (i as f32 / 2.0).ceil() * std::f32::consts::FRAC_PI_4 * 0.75;
            spot_near((grid, origin), feet + forward(away + turn) * COVER_RUN, COVER_RUN * 0.3, rng)
        })
        .filter(|&spot| flat(spot - danger).norm() > flat(feet - danger).norm())
        .collect();
    spots.iter().copied().find(|&spot| !open(spot)).or_else(|| {
        spots.iter().copied().max_by(|a, b| flat(*a - danger).norm().total_cmp(&flat(*b - danger).norm()))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_runs_away_from_the_shooting_somewhere_hidden_if_it_can() {
        let mut grid = WalkGrid::new(80, 80, 1.0);
        for z in 0..80 {
            for x in 0..80 {
                grid.set(x, z, true);
            }
        }
        let origin = Vector3::zeros();
        let feet = Vector3::new(40.0, 0.0, 40.0);
        let danger = Vector3::new(40.0, 0.0, 30.0);
        let mut rng = Rng::new(1);
        let to = take_cover((&grid, origin), feet, 0.0, danger, &mut rng, |_| true).unwrap();
        assert!(flat(to - danger).norm() > flat(feet - danger).norm() + 10.0);
        // Hidden only off to the side: there, rather than straight on.
        let to = take_cover((&grid, origin), feet, 0.0, danger, &mut rng, |at| at.x > 35.0 && at.x < 45.0).unwrap();
        assert!(to.x <= 35.0 || to.x >= 45.0);
    }
}
