//! Firing blind. An armed NPC that notices [`BLIND_AFTER`] bolts within [`BLIND_MEMORY`] seconds
//! of each other, from someone it cannot see, fires blind back where they came from, as Fallout's
//! do: for [`BLIND_FOR`] seconds, as often as [`BLIND_FIRE_EVERY`] says, each shot anywhere within
//! [`BLIND_WIDE`] meters of there, across, and half as much up and down.

use crate::{navigation::grid::Rng, navigation::route::between};
use nalgebra::Vector3;

pub const BLIND_AFTER: u8 = 3;
pub const BLIND_MEMORY: f32 = 4.0;
pub const BLIND_FOR: f32 = 3.0;
pub const BLIND_FIRE_EVERY: (f32, f32) = (0.15, 0.6);
pub const BLIND_WIDE: f32 = 3.0;

/// An NPC's count of the bolts it has noticed from someone unseen, and its firing blind.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct BlindFire {
    /// How many it has noticed, and how long until they are forgotten, in seconds.
    unseen: u8,
    memory: f32,
    /// Where it fires blind at, and for how much longer, in seconds.
    firing: Option<(Vector3<f32>, f32)>,
}

impl BlindFire {
    /// Notes a bolt from `from`, noticed by an armed NPC that does not see who fired it: after
    /// enough, it fires blind back there.
    pub fn unseen_bolt(&mut self, from: Vector3<f32>) {
        self.unseen = self.unseen.saturating_add(1);
        self.memory = BLIND_MEMORY;
        if self.unseen >= BLIND_AFTER {
            self.firing = Some((from, BLIND_FOR));
        }
    }

    /// Moves it on by `dt`: the bolts forgotten in a while; firing blind until it is done, or the
    /// NPC `sees_them` - whoever it was firing at - or is `down`.
    pub fn step(&mut self, dt: f32, sees_them: bool, down: bool) {
        self.memory = (self.memory - dt).max(0.0);
        if self.memory == 0.0 {
            self.unseen = 0;
        }
        if let Some((_, left)) = self.firing.as_mut() {
            *left -= dt;
        }
        if sees_them || down || self.firing.is_some_and(|(_, left)| left <= 0.0) {
            self.firing = None;
        }
    }

    /// Where it is firing blind at, if it is.
    pub fn at(&self) -> Option<Vector3<f32>> {
        self.firing.map(|(at, _)| at)
    }

    /// Forgets it all, and stops.
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

/// Where a shot fired blind at `from` goes, picked by `rng`: anywhere within [`BLIND_WIDE`] of it.
pub fn aim(from: Vector3<f32>, rng: &mut Rng) -> Vector3<f32> {
    from + Vector3::new(
        between(rng, (-BLIND_WIDE, BLIND_WIDE)),
        between(rng, (-0.5 * BLIND_WIDE, 0.5 * BLIND_WIDE)),
        between(rng, (-BLIND_WIDE, BLIND_WIDE)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enough_bolts_from_someone_unseen_close_together_and_it_fires_blind_back_for_a_while() {
        let from = Vector3::new(5.0, 0.0, 0.0);
        let mut blind = BlindFire::default();
        for _ in 0..BLIND_AFTER - 1 {
            blind.unseen_bolt(from);
        }
        assert_eq!(blind.at(), None);
        // Too long apart: forgotten.
        blind.step(BLIND_MEMORY + 0.1, false, false);
        blind.unseen_bolt(from);
        assert_eq!(blind.at(), None);
        blind.unseen_bolt(from);
        blind.unseen_bolt(from);
        assert_eq!(blind.at(), Some(from));
        blind.step(BLIND_FOR / 2.0, false, false);
        assert_eq!(blind.at(), Some(from));
        blind.step(BLIND_FOR, false, false);
        assert_eq!(blind.at(), None);
        // Seeing who it is, it stops.
        (0..BLIND_AFTER).for_each(|_| blind.unseen_bolt(from));
        blind.step(0.0, true, false);
        assert_eq!(blind.at(), None);
    }

    #[test]
    fn a_blind_shot_goes_anywhere_near_where_the_bolts_came_from() {
        let mut rng = Rng::new(7);
        let from = Vector3::new(10.0, 1.0, -4.0);
        for _ in 0..100 {
            let off = aim(from, &mut rng) - from;
            assert!(off.x.abs() <= BLIND_WIDE && off.z.abs() <= BLIND_WIDE && off.y.abs() <= 0.5 * BLIND_WIDE);
        }
    }
}
