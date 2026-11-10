//! Ties between NPCs: those that walk about together - a pair, a group (see
//! [`crate::social::group`]) - care what becomes of each other. One of them shot down before the
//! rest, each takes it as its nerve has it ([`grieve`]): one with the nerve and the means goes
//! after whoever did it, if it knows who ([`Grief::Avenge`]); one with little nerve runs
//! ([`Grief::Flee`]); and the rest go to it and kneel by it a while ([`Grief::Mourn`]) - and,
//! knowing who did it, all of them hold it against them the more ([`GRUDGE`]).
//!
//! An NPC's nerve is its own, the same each time - from its number, say ([`nerve`]) - so that the
//! same one always takes it the same way.

use nalgebra::Vector3;

/// The nerve, from 0 to 1, an NPC needs to go after whoever shot one of its own, and under which
/// it runs instead.
pub const AVENGE_NERVE: f32 = 0.5;
pub const FLEE_NERVE: f32 = 0.3;
/// How much further one of a pair or group that saw who shot one of its own goes against them, on
/// top of what seeing it done does.
pub const GRUDGE: f32 = 0.3;
/// How near, in meters, a mourner goes to the one that went down; how long it kneels there, in
/// seconds; and how long it goes on trying to get there at most.
pub const REACH: f32 = 1.2;
pub const MOURN_FOR: f32 = 8.0;
pub const GIVE_UP: f32 = 20.0;

tuning!(
    /// How NPCs take what becomes of their own (see the module's constants, its defaults).
    {
        /// The nerve to go after whoever did it, and under which one runs.
        avenge_nerve: f32 = AVENGE_NERVE,
        flee_nerve: f32 = FLEE_NERVE,
        /// How much further one that saw who did it goes against them.
        grudge: f32 = GRUDGE,
        /// How near it goes, in meters; how long it kneels, and tries to get there, in seconds.
        reach: f32 = REACH,
        mourn_for: f32 = MOURN_FOR,
        give_up: f32 = GIVE_UP,
    }
);

/// How one takes one of its own shot down before it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Grief {
    /// It goes to it, and kneels by it a while.
    Mourn,
    /// It runs.
    Flee,
    /// It goes after whoever did it.
    Avenge,
}

/// An NPC's nerve, from 0 to 1, from `seed` - its number, say: the same seed, the same nerve.
pub fn nerve(seed: u32) -> f32 {
    // A few rounds of an integer hash, so that seeds next to each other are not alike.
    let mut x = seed.wrapping_mul(0x9E37_79B9) ^ 0x5bd1_e995;
    x ^= x >> 15;
    x = x.wrapping_mul(0x2c1b_3c6d);
    x ^= x >> 12;
    x = x.wrapping_mul(0x297a_2d39);
    x ^= x >> 15;
    (x >> 8) as f32 / (1u32 << 24) as f32
}

/// How one with `nerve` takes one of its own shot down before it: going after whoever did it, if
/// it `can_fight` and `knows_who` and has the nerve; running, without much nerve; or mourning it.
pub fn grieve(tuning: &Tuning, nerve: f32, can_fight: bool, knows_who: bool) -> Grief {
    if can_fight && knows_who && nerve >= tuning.avenge_nerve {
        Grief::Avenge
    } else if nerve < tuning.flee_nerve {
        Grief::Flee
    } else {
        Grief::Mourn
    }
}

/// What a mourner does this step.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Doing {
    /// Goes to `at` - working out its way there `afresh` the first time.
    GoTo { at: Vector3<f32>, afresh: bool },
    /// Kneels there, facing `heading`, towards the one lying there.
    Kneel { heading: f32 },
    /// Done with it - it has mourned a while, or could not get there - and goes back to what it
    /// was doing.
    Done,
}

/// One going to one of its own that went down, and kneeling by it.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Mourning {
    /// Where it lies.
    pub at: Vector3<f32>,
    /// How long it has been at it, and kneeling, in seconds.
    time: f32,
    knelt: f32,
    started: bool,
}

impl Mourning {
    /// Going to one lying `at`.
    pub fn new(at: Vector3<f32>) -> Self {
        Self { at, time: 0.0, knelt: 0.0, started: false }
    }

    /// Whether it is kneeling there.
    pub fn kneeling(&self) -> bool {
        self.knelt > 0.0
    }

    /// Moves it on `dt` for an NPC at `feet`: what it does.
    pub fn step(&mut self, tuning: &Tuning, feet: Vector3<f32>, dt: f32) -> Doing {
        self.time += dt;
        let to = Vector3::new(self.at.x - feet.x, 0.0, self.at.z - feet.z);
        if self.knelt > 0.0 || to.norm() <= tuning.reach {
            self.knelt += dt;
            if self.knelt >= tuning.mourn_for {
                return Doing::Done;
            }
            return Doing::Kneel { heading: to.x.atan2(to.z) };
        }
        if self.time >= tuning.give_up {
            return Doing::Done;
        }
        let afresh = !std::mem::replace(&mut self.started, true);
        Doing::GoTo { at: self.at, afresh }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_takes_it_as_its_nerve_has_it_the_same_every_time() {
        let t = Tuning::DEFAULT;
        assert_eq!(grieve(&t, 0.8, true, true), Grief::Avenge);
        assert_eq!(grieve(&t, 0.8, true, false), Grief::Mourn, "it does not know who");
        assert_eq!(grieve(&t, 0.8, false, true), Grief::Mourn, "it has no means");
        assert_eq!(grieve(&t, 0.1, true, true), Grief::Flee);
        assert_eq!(nerve(42), nerve(42));
        let nerves: Vec<f32> = (0..1000).map(nerve).collect();
        assert!(nerves.iter().all(|n| (0.0..1.0).contains(n)));
        let flee = nerves.iter().filter(|&&n| n < FLEE_NERVE).count();
        assert!((200..400).contains(&flee), "about a third run: {flee}");
    }

    #[test]
    fn a_mourner_goes_to_it_kneels_a_while_and_is_done() {
        let t = Tuning::DEFAULT;
        let mut mourning = Mourning::new(Vector3::new(0.0, 0.0, 10.0));
        assert_eq!(mourning.step(&t, Vector3::zeros(), 0.1), Doing::GoTo { at: mourning.at, afresh: true });
        assert!(matches!(mourning.step(&t, Vector3::zeros(), 0.1), Doing::GoTo { afresh: false, .. }));
        let by = Vector3::new(0.0, 0.0, 9.0);
        let Doing::Kneel { heading } = mourning.step(&t, by, 0.1) else {
            panic!("kneels");
        };
        assert!(heading.abs() < 1e-6, "facing it");
        assert!(mourning.kneeling());
        for _ in 0..(MOURN_FOR * 10.0) as usize {
            if mourning.step(&t, by, 0.1) == Doing::Done {
                return;
            }
        }
        panic!("it never got up");
    }

    #[test]
    fn one_that_cannot_get_there_gives_it_up() {
        let t = Tuning::DEFAULT;
        let mut mourning = Mourning::new(Vector3::new(0.0, 0.0, 100.0));
        let mut last = Doing::Done;
        for _ in 0..(GIVE_UP * 10.0) as usize + 2 {
            last = mourning.step(&t, Vector3::zeros(), 0.1);
        }
        assert_eq!(last, Doing::Done);
    }
}
