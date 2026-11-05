//! A city's crowd: only so many NPCs, all of them about the player. One that has gone far off,
//! out of sight, turns up again as another nearer by, somewhere the player cannot see, so that
//! wherever they go the streets are as busy - busier than the whole city's hundreds spread over
//! it would make them - and only those near enough to matter cost anything.

use crate::{
    flat, forward,
    social::group::{self, about_leader},
    navigation::{
        grid::Rng,
        navmesh::{NavMesh, Poly},
        route::between,
    },
};
use nalgebra::Vector3;

/// How far from the player, in meters along the ground, one of a crowd that is going about its
/// business and cannot be seen is taken out of the way.
pub const CROWD_GONE: f32 = 110.0;
/// How far from the player, in meters along the ground, it turns up again, from least to most -
/// and how far the crowd is put down to begin with, at most.
pub const CROWD_BACK: (f32, f32) = (45.0, 85.0);
/// How often the crowd is looked over, in seconds, and how many of them turn up again each time
/// at most, so that a crowd left behind all at once comes back over a few looks.
pub const CROWD_EVERY: f32 = 0.5;
pub const CROWD_AT_ONCE: usize = 6;
/// How long one of a crowd stands still between trips, in seconds, from least to most: not as
/// long as others, so that more of them are on the move.
pub const CROWD_REST: (f32, f32) = (1.0, 4.0);
/// How many spots it tries for each one turning up again before leaving it for the next look.
pub const CROWD_TRIES: usize = 16;
/// How far off where the player looks, in radians, is out of view whatever is in the way: a
/// little wider than half of the widest view.
pub const OUT_OF_VIEW: f32 = 75.0 * std::f32::consts::PI / 180.0;

tuning!(
    /// How a city's crowd keeps about the player (see the module's constants, its defaults).
    {
        /// How far off one has to be to go, and how far off it turns up again, in meters.
        crowd_gone: f32 = CROWD_GONE,
        crowd_back: (f32, f32) = CROWD_BACK,
        /// How often the crowd is looked over, in seconds; how many turn up again at once; how
        /// long one that turns up stands first, in seconds; and how many spots are tried for it.
        crowd_every: f32 = CROWD_EVERY,
        crowd_at_once: usize = CROWD_AT_ONCE,
        crowd_rest: (f32, f32) = CROWD_REST,
        crowd_tries: usize = CROWD_TRIES,
        /// How far off the player's view, in radians, counts as out of it.
        out_of_view: f32 = OUT_OF_VIEW,
    }
);

/// A crowd: where it can turn up, and when it is next looked over.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Crowd {
    /// Whether each polygon of the navmesh, by number, is somewhere one can turn up: off the
    /// roads, and where the player started from could be walked to.
    pub pavement: Vec<bool>,
    /// How long till the crowd is next looked over, in seconds.
    pub next: f32,
    /// How it keeps about the player.
    pub tuning: Tuning,
}

impl Crowd {
    /// A crowd that can turn up where `pavement` says, looked over first in [`CROWD_EVERY`].
    pub fn new(pavement: Vec<bool>) -> Self {
        Self::with(pavement, Tuning::DEFAULT)
    }

    /// A crowd that can turn up where `pavement` says, keeping about the player as `tuning` says.
    pub fn with(pavement: Vec<bool>, tuning: Tuning) -> Self {
        Self { pavement, next: tuning.crowd_every, tuning }
    }

    /// Somewhere on the floor of `mesh` that one of the crowd could
    /// have walked to, a little way off the `player`'s feet, where `hidden` says the player cannot
    /// see it turn up; and for a group, room for the rest about it, keeping to `slots` of
    /// the slots about a leader `formation` has (see [`crate::social::group::about_leader`]):
    /// where, which way it faces, and where the rest are.
    #[allow(clippy::type_complexity)]
    pub fn spot(
        &self,
        mesh: &NavMesh,
        player: Vector3<f32>,
        slots: &[usize],
        formation: &group::Tuning,
        rng: &mut Rng,
        hidden: impl Fn(Vector3<f32>) -> bool,
    ) -> Option<(Vector3<f32>, f32, Vec<Vector3<f32>>)> {
        (0..self.tuning.crowd_tries).find_map(|_| {
            let angle = between(rng, (-std::f32::consts::PI, std::f32::consts::PI));
            let away = between(rng, self.tuning.crowd_back);
            let at = player + forward(angle) * away;
            let pavement = |n: Poly| self.pavement.get(n as usize).copied().unwrap_or(false);
            // The floor at a spot nearest a height, if it is pavement.
            let floor = |at: Vector3<f32>, height: f32| {
                let n = mesh
                    .over_spot(at)
                    .into_iter()
                    .min_by(|&a, &b| (mesh.floor_of(a) - height).abs().total_cmp(&(mesh.floor_of(b) - height).abs()))?;
                pavement(n).then(|| Vector3::new(at.x, mesh.floor_of(n), at.z))
            };
            let feet = floor(at, player.y)?;
            if !hidden(feet) {
                return None;
            }
            let heading = between(rng, (-std::f32::consts::PI, std::f32::consts::PI));
            let about: Option<Vec<Vector3<f32>>> = slots
                .iter()
                .map(|&slot| floor(about_leader(formation, feet, heading, slot), feet.y))
                .collect();
            Some((feet, heading, about?))
        })
    }
}

/// Whether `at` is off to the side of the view of a player at `player` looking `ahead` along the
/// ground - out of view whatever is in the way (see [`OUT_OF_VIEW`]).
pub fn off_view(tuning: &Tuning, player: Vector3<f32>, ahead: Option<Vector3<f32>>, at: Vector3<f32>) -> bool {
    match (ahead, flat(at - player).try_normalize(1.0e-3)) {
        (Some(ahead), Some(to)) => to.dot(&ahead) < tuning.out_of_view.cos(),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_of_a_crowd_turns_up_on_the_pavement_a_way_off_where_it_cannot_be_seen() {
        let mut grid = crate::navigation::grid::WalkGrid::new(200, 200, 1.0);
        for z in 0..200 {
            for x in 0..200 {
                grid.set(x, z, true);
            }
        }
        // Pavement only on the west half.
        let mesh = NavMesh::build(&grid, Vector3::zeros(), |x, _| u8::from(x >= 100));
        let crowd = Crowd::new((0..mesh.len() as Poly).map(|n| mesh.polygon(n).area == 0).collect());
        let player = Vector3::new(100.0, 0.0, 100.0);
        let mut rng = Rng::new(4);
        let mut found = 0;
        for _ in 0..30 {
            let Some((feet, _, _)) = crowd.spot(&mesh, player, &[], &group::Tuning::DEFAULT, &mut rng, |at| at.z < 100.0) else {
                continue;
            };
            found += 1;
            let away = flat(feet - player).norm();
            assert!(feet.x < 100.0 && feet.z < 100.0, "{feet:?}");
            assert!(away >= CROWD_BACK.0 - 1.0 && away <= CROWD_BACK.1 + 1.0, "{away}");
        }
        assert!(found > 0);
        assert!(off_view(&Tuning::DEFAULT, player, Some(Vector3::z()), player - Vector3::z() * 10.0));
        assert!(!off_view(&Tuning::DEFAULT, player, Some(Vector3::z()), player + Vector3::z() * 10.0));
    }
}
