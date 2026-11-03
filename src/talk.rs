//! Who can be talked to: an NPC close by and in front of the player, nearest first.

use crate::flat;
use nalgebra::Vector3;

/// How near the player's feet an NPC's have to be, in meters, and how far off where the player
/// looks it can be, in radians, for the player to talk to it.
pub const TALK_REACH: f32 = 2.5;
pub const TALK_CONE: f32 = 40.0 * std::f32::consts::PI / 180.0;

/// Those of the NPCs with their feet at `npcs` that a player at `feet`, looking `ahead` along the
/// ground, is close enough to and facing enough to talk to, nearest first, by index.
pub fn within_talking(npcs: impl Iterator<Item = Vector3<f32>>, feet: Vector3<f32>, ahead: Vector3<f32>) -> Vec<usize> {
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
            (distance < TALK_REACH && off < TALK_CONE).then_some((distance, i))
        })
        .collect();
    near.sort_by(|a, b| a.0.total_cmp(&b.0));
    near.into_iter().map(|(_, i)| i).collect()
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
        assert_eq!(within_talking(npcs.into_iter(), Vector3::zeros(), Vector3::z()), [1, 0]);
    }
}
