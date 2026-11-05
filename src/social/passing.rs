//! Passing the player by: an NPC going about its business that the player comes close to shows
//! what it thinks of them ([`gesture`]). One that likes them greets them - or, knowing something
//! they would want to know, tips them off: security about, say ([`Gesture::Tip`]). One that thinks
//! nothing much of them may nod, now and then. One wary of them keeps its distance; one that hates
//! them stops and glares.
//!
//! Each NPC does so once in a while at most ([`AGAIN_AFTER`]), and the town as a whole not too
//! often ([`BETWEEN`]): a street of them is not a chorus. A game keeps a [`Passing`] for each
//! NPC, and one for them all.

use crate::social::opinion::Attitude;

/// How near the player, in meters, an NPC shows what it thinks of them; how long after it last
/// did, in seconds, before it does again; how long after any did before another does; how often
/// one that thinks nothing much of them nods, from 0 to 1; and how long it stops and looks at
/// them, in seconds.
pub const NEAR: f32 = 5.0;
pub const AGAIN_AFTER: f32 = 120.0;
pub const BETWEEN: f32 = 6.0;
pub const NOD: f32 = 0.3;
pub const LOOK_FOR: f32 = 2.5;

tuning!(
    /// How NPCs show what they think of the player in passing (see the module's constants, its
    /// defaults).
    {
        /// How near, in meters.
        near: f32 = NEAR,
        /// How long before one does again, and before any other does, in seconds.
        again_after: f32 = AGAIN_AFTER,
        between: f32 = BETWEEN,
        /// How often one that thinks nothing much of them nods.
        nod: f32 = NOD,
        /// How long it stops and looks at them, in seconds.
        look_for: f32 = LOOK_FOR,
    }
);

/// How an NPC shows what it thinks of the player, passing them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Gesture {
    /// It likes them: it greets them.
    Greet,
    /// It likes them, and tells them something they would want to know.
    Tip,
    /// It thinks nothing much of them: a nod, and nothing said.
    Nod,
    /// It is wary of them: it keeps its distance.
    Avoid,
    /// It hates them: it stops and glares.
    Glare,
}

impl Gesture {
    /// Whether it says anything with it.
    pub fn says(self) -> bool {
        self != Gesture::Nod
    }

    /// Whether it stops and looks at them with it, rather than keep away.
    pub fn looks(self) -> bool {
        self != Gesture::Avoid
    }
}

/// How one that takes the player as `attitude` shows it, passing them: a tip if it likes them and
/// `has_tip`; and with `roll`, from 0 to 1, whether one that thinks nothing much of them nods.
/// None, for one that lets them by.
pub fn gesture(tuning: &Tuning, attitude: Attitude, has_tip: bool, roll: f32) -> Option<Gesture> {
    match attitude {
        Attitude::Friendly if has_tip => Some(Gesture::Tip),
        Attitude::Friendly => Some(Gesture::Greet),
        Attitude::Neutral => (roll < tuning.nod).then_some(Gesture::Nod),
        Attitude::Wary => Some(Gesture::Avoid),
        Attitude::Hostile => Some(Gesture::Glare),
    }
}

/// How long since one - or anyone - last showed the player what it thinks of them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Passing {
    ago: f32,
}

impl Default for Passing {
    /// Never yet.
    fn default() -> Self {
        Self { ago: f32::INFINITY }
    }
}

impl Passing {
    /// Whether it has been `after` seconds since it last did.
    pub fn ready(&self, after: f32) -> bool {
        self.ago >= after
    }

    /// It did, just now.
    pub fn did(&mut self) {
        self.ago = 0.0;
    }

    /// Moves it on `dt`.
    pub fn step(&mut self, dt: f32) {
        self.ago += dt;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_shows_what_it_thinks_and_not_too_often() {
        let t = Tuning::DEFAULT;
        assert_eq!(gesture(&t, Attitude::Friendly, false, 0.9), Some(Gesture::Greet));
        assert_eq!(gesture(&t, Attitude::Friendly, true, 0.9), Some(Gesture::Tip));
        assert_eq!(gesture(&t, Attitude::Neutral, true, 0.1), Some(Gesture::Nod), "no tip but for a friend");
        assert_eq!(gesture(&t, Attitude::Neutral, false, 0.9), None);
        assert_eq!(gesture(&t, Attitude::Wary, false, 0.9), Some(Gesture::Avoid));
        assert_eq!(gesture(&t, Attitude::Hostile, false, 0.9), Some(Gesture::Glare));
        assert!(!Gesture::Nod.says() && Gesture::Glare.says() && !Gesture::Avoid.looks());
        let mut passing = Passing::default();
        assert!(passing.ready(AGAIN_AFTER));
        passing.did();
        passing.step(AGAIN_AFTER - 1.0);
        assert!(!passing.ready(AGAIN_AFTER) && passing.ready(BETWEEN));
        passing.step(1.0);
        assert!(passing.ready(AGAIN_AFTER));
    }
}
