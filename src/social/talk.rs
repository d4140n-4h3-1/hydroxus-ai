//! Talking: who the player can talk to - an NPC close by and in front of them, nearest first;
//! two going about by themselves that meet, and stop to talk; a group that talks among itself
//! near the player; and how far off what they say is heard.

use crate::flat;
use nalgebra::Vector3;

/// How near the player's feet an NPC's have to be, in meters, and how far off where the player
/// looks it can be, in radians, for the player to talk to it.
pub const TALK_REACH: f32 = 2.5;
pub const TALK_CONE: f32 = 40.0 * std::f32::consts::PI / 180.0;
/// How near the player, in meters, what NPCs call out to each other is heard.
pub const HEARD: f32 = 40.0;

tuning!(
    /// Who can be talked to, and how far off talk carries (see the module's constants, its
    /// defaults).
    {
        /// How near the player an NPC has to be, in meters, and how far off where they look, in
        /// radians, to be talked to.
        talk_reach: f32 = TALK_REACH,
        talk_cone: f32 = TALK_CONE,
        /// How near the player what NPCs call out to each other is heard, in meters.
        heard: f32 = HEARD,
    }
);

/// Those of the NPCs with their feet at `npcs` that a player at `feet`, looking `ahead` along the
/// ground, is close enough to and facing enough to talk to, nearest first, by index.
pub fn within_talking(tuning: &Tuning, npcs: impl Iterator<Item = Vector3<f32>>, feet: Vector3<f32>, ahead: Vector3<f32>) -> Vec<usize> {
    let mut near: Vec<(f32, usize)> = npcs
        .enumerate()
        .filter_map(|(i, there)| {
            let to_them = flat(there - feet);
            let distance = to_them.norm();
            let off = if distance < 1.0e-4 {
                0.0
            } else {
                (to_them.dot(&ahead) / distance).clamp(-1.0, 1.0).acos()
            };
            (distance < tuning.talk_reach && off < tuning.talk_cone).then_some((distance, i))
        })
        .collect();
    near.sort_by(|a, b| a.0.total_cmp(&b.0));
    near.into_iter().map(|(_, i)| i).collect()
}

/// Whether what one at `feet` calls out is heard by the player at `player`.
pub fn heard(tuning: &Tuning, feet: Vector3<f32>, player: Vector3<f32>) -> bool {
    flat(feet - player).norm() < tuning.heard
}

/// Of `lone` - NPCs going about by themselves and at ease, by index, where each is - the two
/// nearest the `player` that have come within `near` meters of each other: to stop and talk.
pub fn meeting(lone: &[(usize, Vector3<f32>)], player: Vector3<f32>, near: f32) -> Option<(usize, usize)> {
    let away = |at: Vector3<f32>| flat(at - player).norm();
    lone.iter()
        .enumerate()
        .flat_map(|(i, &a)| lone[i + 1..].iter().map(move |&b| (a, b)))
        .filter(|&((_, a), (_, b))| flat(a - b).norm() < near)
        .min_by(|x, y| (away(x.0 .1) + away(x.1 .1)).total_cmp(&(away(y.0 .1) + away(y.1 .1))))
        .map(|((n, _), (m, _))| (n, m))
}

/// Of `groups` - each its members, the one leading it first - the one whose leader, at `feet` of
/// it, is nearest the `player`.
pub fn nearest_group(
    groups: impl IntoIterator<Item = Vec<usize>>,
    feet: impl Fn(usize) -> Vector3<f32>,
    player: Vector3<f32>,
) -> Option<Vec<usize>> {
    groups.into_iter().filter(|g| !g.is_empty()).min_by(|a, b| {
        flat(feet(a[0]) - player).norm().total_cmp(&flat(feet(b[0]) - player).norm())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_those_close_by_and_in_front_can_be_talked_to_nearest_first() {
        let npcs = [
            Vector3::new(0.0, 0.0, 2.0),  // ahead
            Vector3::new(0.3, 0.0, 1.0),  // nearer, a little to one side
            Vector3::new(0.0, 0.0, -1.0), // behind
            Vector3::new(2.0, 0.0, 0.5),  // well off to the side
            Vector3::new(0.0, 0.0, 4.0),  // too far
        ];
        assert_eq!(within_talking(&Tuning::DEFAULT, npcs.into_iter(), Vector3::zeros(), Vector3::z()), [1, 0]);
    }

    #[test]
    fn two_by_themselves_that_meet_near_the_player_stop_to_talk_and_the_nearest_group_talks() {
        let player = Vector3::zeros();
        let lone = [
            (3, Vector3::new(10.0, 0.0, 0.0)),
            (5, Vector3::new(11.0, 0.0, 0.0)),
            (8, Vector3::new(4.0, 0.0, 0.0)),
            (9, Vector3::new(4.5, 0.0, 0.0)),
            (12, Vector3::new(30.0, 0.0, 0.0)),
        ];
        assert_eq!(meeting(&lone, player, 2.0), Some((8, 9)));
        assert_eq!(meeting(&lone[..2], player, 2.0), Some((3, 5)));
        assert_eq!(meeting(&[lone[0], lone[4]], player, 2.0), None);
        let feet = |n: usize| Vector3::new(n as f32, 0.0, 0.0);
        assert_eq!(nearest_group([vec![7, 8], vec![2, 3]], feet, player), Some(vec![2, 3]));
        assert!(heard(&Tuning::DEFAULT, Vector3::new(HEARD - 1.0, 0.0, 0.0), player) && !heard(&Tuning::DEFAULT, Vector3::new(HEARD + 1.0, 0.0, 0.0), player));
    }
}
