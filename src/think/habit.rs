//! The player's habits: what they tend to do, as an NPC - or a whole faction - has seen it, and how
//! far it can go by it.
//!
//! A game tells apart whatever it likes of what the player does - by its own type, an enum, say:
//! the way they come in (the front gate, over the roofs, through the sewers), how they deal with a
//! guard (a knife, a shot, a bribe, going round), the trick they use. Each time an NPC sees, or
//! comes across the signs of, one of them ([`Habits::saw`]), it counts it, and what it counted
//! fades ([`Habits::step`]), so that what the player did lately counts most.
//!
//! What it makes of them is a share and a sureness: how much of what they do is each thing
//! ([`Habits::share`]), and how far that is worth going by, for how much it has seen
//! ([`Habits::sureness`]) - a half once it has seen the tuning's `sure` of them, nearer all of it
//! the more. [`Habits::expect`] is both in one, from 0 to 1, as a consideration's input: a guard
//! that has seen the player come over the roofs time after time comes to watch the roofs, one
//! that has seen it once barely does.

/// How long, in seconds, what was seen takes to fade halfway: a quarter of an hour.
pub const HALF_LIFE: f32 = 900.0;
/// How many times something has to be seen for an NPC to be half sure of the player's habits.
pub const SURE: f32 = 4.0;
/// How much what an NPC is told counts for, against what it saw itself: a half.
pub const TOLD: f32 = 0.5;

tuning!(
    /// How an NPC learns the player's habits (see the module's constants, its defaults).
    {
        /// How long, in seconds, what was seen takes to fade halfway.
        half_life: f32 = HALF_LIFE,
        /// How many times something has to be seen to be half sure.
        sure: f32 = SURE,
        /// How much what it is told counts for.
        told: f32 = TOLD,
    }
);

/// How often an NPC has seen the player do each thing, lately.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Habits<T> {
    seen: Vec<(T, f32)>,
}

impl<T> Default for Habits<T> {
    fn default() -> Self {
        Self { seen: Vec::new() }
    }
}

impl<T: Copy + Eq> Habits<T> {
    /// Nothing seen yet.
    pub fn new() -> Self {
        Self::default()
    }

    /// It saw the player do `what`, or came across the signs of it.
    pub fn saw(&mut self, what: T) {
        self.count(what, 1.0);
    }

    /// It tells `to` what it has seen: each counted the tuning's `told` share as much.
    pub fn tell(&self, tuning: &Tuning, to: &mut Habits<T>) {
        for &(what, times) in &self.seen {
            to.count(what, times * tuning.told.max(0.0));
        }
    }

    /// Moves it on `dt` seconds: what was seen fading, halfway in the tuning's half-life, and
    /// forgotten once next to nothing is left.
    pub fn step(&mut self, tuning: &Tuning, dt: f32) {
        for (_, times) in &mut self.seen {
            *times = super::fade(*times, tuning.half_life, dt);
        }
        self.seen.retain(|(_, times)| *times > 1.0e-2);
    }

    /// How many times, lately, it has seen the player do `what`.
    pub fn times(&self, what: T) -> f32 {
        self.seen.iter().find(|(w, _)| *w == what).map_or(0.0, |&(_, times)| times)
    }

    /// How many times, lately, it has seen the player do anything.
    pub fn total(&self) -> f32 {
        self.seen.iter().map(|(_, times)| times).sum()
    }

    /// How much of what it has seen the player do is `what`, from 0 to 1.
    pub fn share(&self, what: T) -> f32 {
        let total = self.total();
        if total <= 0.0 {
            0.0
        } else {
            self.times(what) / total
        }
    }

    /// What it has seen the player do most, and its share - none if it has seen nothing. The
    /// first seen wins a tie.
    pub fn likeliest(&self) -> Option<(T, f32)> {
        let total = self.total();
        let mut best: Option<(T, f32)> = None;
        for &(what, times) in &self.seen {
            if best.is_none_or(|(_, b)| times > b) {
                best = Some((what, times));
            }
        }
        best.filter(|_| total > 0.0).map(|(what, times)| (what, times / total))
    }

    /// How far it can go by what it has seen, from 0 to 1: a half once it has seen the tuning's
    /// `sure` of it.
    pub fn sureness(&self, tuning: &Tuning) -> f32 {
        let total = self.total();
        total / (total + tuning.sure.max(1.0e-3))
    }

    /// How much it expects the player to do `what` next, from 0 to 1: its share of what it has
    /// seen, for how sure it can be of it.
    pub fn expect(&self, tuning: &Tuning, what: T) -> f32 {
        self.share(what) * self.sureness(tuning)
    }

    fn count(&mut self, what: T, times: f32) {
        if times.is_nan() || times <= 0.0 {
            return;
        }
        match self.seen.iter_mut().find(|(w, _)| *w == what) {
            Some((_, had)) => *had += times,
            None => self.seen.push((what, times)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T: Tuning = Tuning::DEFAULT;

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    enum Way {
        Gate,
        Roofs,
        Sewers,
    }

    #[test]
    fn it_comes_to_expect_what_the_player_keeps_doing() {
        let mut habits = Habits::new();
        assert_eq!(habits.likeliest(), None);
        assert_eq!(habits.expect(&T, Way::Roofs), 0.0);
        habits.saw(Way::Roofs);
        let once = habits.expect(&T, Way::Roofs);
        assert!(once < 0.25, "seen once, barely: {once}");
        for _ in 0..7 {
            habits.saw(Way::Roofs);
        }
        habits.saw(Way::Gate);
        assert_eq!(habits.likeliest().map(|(w, _)| w), Some(Way::Roofs));
        assert!(habits.expect(&T, Way::Roofs) > 0.6, "{}", habits.expect(&T, Way::Roofs));
        assert!(habits.expect(&T, Way::Gate) < 0.1);
        assert_eq!(habits.expect(&T, Way::Sewers), 0.0);
    }

    #[test]
    fn what_they_did_lately_counts_most() {
        let mut habits = Habits::new();
        for _ in 0..6 {
            habits.saw(Way::Roofs);
        }
        habits.step(&T, HALF_LIFE * 3.0);
        for _ in 0..2 {
            habits.saw(Way::Sewers);
        }
        assert_eq!(habits.likeliest().map(|(w, _)| w), Some(Way::Sewers), "they changed their ways");
        habits.step(&T, HALF_LIFE * 30.0);
        assert_eq!(habits.total(), 0.0, "and it all fades");
    }

    #[test]
    fn what_it_is_told_counts_for_less() {
        let mut sentry = Habits::new();
        for _ in 0..4 {
            sentry.saw(Way::Sewers);
        }
        let mut other = Habits::new();
        sentry.tell(&T, &mut other);
        assert_eq!(other.times(Way::Sewers), 4.0 * TOLD);
        assert!(other.sureness(&T) < sentry.sureness(&T));
        assert_eq!(other.likeliest().map(|(w, _)| w), Some(Way::Sewers));
    }
}
