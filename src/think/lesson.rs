//! What has worked: an NPC learning, from how each thing it did turned out, to do it more or less.
//!
//! [`Lessons`] keep, for each thing it could do, what it makes of it from -1 (it never works) to 1
//! (it always does), starting at nothing. Each time the game says how one turned out
//! ([`Lessons::learn`]), from -1 to 1 - a search that found the player, 1; a chase they got away
//! from, -1; a warning that sent them off, a half - it moves that way by the tuning's `rate`, so
//! that one fluke does not undo a habit, but a run of them does. What it learned fades back to
//! nothing ([`Lessons::step`]): a player who changes their ways is met afresh.
//!
//! What it learned weighs a [`Reasoner`]'s choice ([`Lessons::choose`]): each action's score
//! times one and the tuning's `sway` of what it makes of it - by default from a half as much, for
//! what never works, to half as much again, for what always does. It only weighs what is open to
//! it: what its considerations shut, it still does not do, and what never worked it is less
//! inclined to, never barred from - so that, given nothing else, it tries it again.
//!
//! One can tell another what it learned ([`Lessons::tell`]), who takes it as if it had happened
//! to it, by the tuning's `told` share: a squad comes to think alike.

use crate::tactics::utility::{Reasoner, Situation};

/// How far each outcome moves what an NPC makes of what it did, towards it: a quarter.
pub const RATE: f32 = 0.25;
/// How far what it makes of something weighs it, either way: by up to a half.
pub const SWAY: f32 = 0.5;
/// How long, in seconds, what it learned takes to fade halfway back to nothing: ten minutes.
pub const HALF_LIFE: f32 = 600.0;
/// How much of its rate what it is told counts for, against what happened to it: a half.
pub const TOLD: f32 = 0.5;

tuning!(
    /// How an NPC learns what works (see the module's constants, its defaults).
    {
        /// How far each outcome moves what it makes of what it did, towards it.
        rate: f32 = RATE,
        /// How far what it makes of something weighs it, either way.
        sway: f32 = SWAY,
        /// How long, in seconds, what it learned takes to fade halfway.
        half_life: f32 = HALF_LIFE,
        /// How much of its rate what it is told counts for.
        told: f32 = TOLD,
    }
);

/// What an NPC has learned of each thing it could do, from -1 (it never works) to 1 (it always
/// does): nothing for what it has never done, or long forgotten.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Lessons<A> {
    learned: Vec<(A, f32)>,
}

impl<A> Default for Lessons<A> {
    fn default() -> Self {
        Self { learned: Vec::new() }
    }
}

impl<A: Copy + PartialEq> Lessons<A> {
    /// Nothing learned yet.
    pub fn new() -> Self {
        Self::default()
    }

    /// What it makes of `action`, from -1 to 1.
    pub fn worth(&self, action: A) -> f32 {
        self.learned.iter().find(|(a, _)| *a == action).map_or(0.0, |&(_, worth)| worth)
    }

    /// Everything it has learned anything of, and what it makes of each.
    pub fn learned(&self) -> impl Iterator<Item = (A, f32)> + '_ {
        self.learned.iter().copied()
    }

    /// It did `action`, and it turned out `outcome`, from -1 (as badly as it could) to 1 (as well).
    pub fn learn(&mut self, tuning: &Tuning, action: A, outcome: f32) {
        self.move_towards(action, outcome, tuning.rate);
    }

    /// It tells `to` what it learned: each thing it makes anything of, taken as if it had turned
    /// out that way for them, by the tuning's `told` share of the rate.
    pub fn tell(&self, tuning: &Tuning, to: &mut Lessons<A>) {
        for &(action, worth) in &self.learned {
            to.move_towards(action, worth, tuning.rate * tuning.told);
        }
    }

    /// Moves it on `dt` seconds: what it learned fading back to nothing, halfway in the tuning's
    /// half-life - forgotten once next to nothing is left.
    pub fn step(&mut self, tuning: &Tuning, dt: f32) {
        for (_, worth) in &mut self.learned {
            *worth = super::fade(*worth, tuning.half_life, dt);
        }
        self.learned.retain(|(_, worth)| worth.abs() > 1.0e-3);
    }

    /// How many times as much `action` scores, for what it learned of it: from one less the
    /// tuning's sway, for what never works, to one and the sway, for what always does.
    pub fn bias(&self, tuning: &Tuning, action: A) -> f32 {
        (1.0 + tuning.sway * self.worth(action)).max(0.0)
    }

    /// What `reasoner` chooses in `situation`, doing `current` already, with what it learned
    /// weighing each action (see [`Reasoner::choose_by`]).
    pub fn choose<I: Copy>(&self, tuning: &Tuning, reasoner: &mut Reasoner<A, I>, situation: &impl Situation<I>, current: Option<A>) -> Option<A> {
        reasoner.choose_by(situation, current, |action| self.bias(tuning, action))
    }

    fn move_towards(&mut self, action: A, outcome: f32, rate: f32) {
        let (outcome, rate) = (outcome.clamp(-1.0, 1.0), rate.clamp(0.0, 1.0));
        if outcome.is_nan() || rate.is_nan() {
            return;
        }
        match self.learned.iter_mut().find(|(a, _)| *a == action) {
            Some((_, worth)) => *worth += (outcome - *worth) * rate,
            None => self.learned.push((action, outcome * rate)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tactics::conduct::{reasoner, Conduct, Reading, Role};
    use crate::tactics::utility::{Action, Curve};

    const T: Tuning = Tuning::DEFAULT;

    #[test]
    fn a_run_of_outcomes_moves_it_but_one_fluke_does_not_undo_it() {
        let mut lessons = Lessons::new();
        assert_eq!(lessons.worth(Conduct::Search), 0.0);
        for _ in 0..6 {
            lessons.learn(&T, Conduct::Search, 1.0);
        }
        let sure = lessons.worth(Conduct::Search);
        assert!(sure > 0.8, "{sure}");
        lessons.learn(&T, Conduct::Search, -1.0);
        assert!(lessons.worth(Conduct::Search) > 0.3, "still thought well of");
        assert!(lessons.bias(&T, Conduct::Search) > 1.0);
        assert_eq!(lessons.bias(&T, Conduct::Chase), 1.0, "nothing learned of it, nothing made of it");
    }

    #[test]
    fn what_it_learned_fades_and_is_forgotten() {
        let mut lessons = Lessons::new();
        lessons.learn(&T, Conduct::Chase, -1.0);
        let had = lessons.worth(Conduct::Chase);
        lessons.step(&T, HALF_LIFE);
        assert!((lessons.worth(Conduct::Chase) - had / 2.0).abs() < 1.0e-5);
        lessons.step(&T, HALF_LIFE * 20.0);
        assert_eq!(lessons.learned().count(), 0);
    }

    #[test]
    fn what_keeps_failing_it_comes_to_do_less_but_what_is_shut_stays_shut() {
        // Chasing or searching, both open to it: it chases, first.
        let mut sentry = reasoner(Role::Sentry);
        let both = Reading { hunting: true, searching: true, ..Reading::default() };
        let mut lessons = Lessons::new();
        assert_eq!(lessons.choose(&T, &mut sentry, &both, None), Some(Conduct::Chase));
        // Every chase got away from it; every search turned them up.
        for _ in 0..4 {
            lessons.learn(&T, Conduct::Chase, -1.0);
            lessons.learn(&T, Conduct::Search, 1.0);
        }
        assert_eq!(lessons.choose(&T, &mut sentry, &both, None), Some(Conduct::Search));
        // Chasing alone open to it, it chases still: weighed down, not barred.
        let hunting = Reading { hunting: true, ..Reading::default() };
        assert_eq!(lessons.choose(&T, &mut sentry, &hunting, None), Some(Conduct::Chase));
        // Searching, however well it went, is not open with no one to search for.
        assert_eq!(lessons.choose(&T, &mut sentry, &Reading::default(), None), Some(Conduct::GoAbout));
        // And with time, it forgets, and chases first again.
        lessons.step(&T, HALF_LIFE * 20.0);
        assert_eq!(lessons.choose(&T, &mut sentry, &both, None), Some(Conduct::Chase));
    }

    #[test]
    fn what_it_is_told_counts_but_less_than_what_happened_to_it() {
        let mut veteran = Lessons::new();
        for _ in 0..8 {
            veteran.learn(&T, Conduct::Shelter, 1.0);
        }
        let mut rookie = Lessons::new();
        veteran.tell(&T, &mut rookie);
        let told = rookie.worth(Conduct::Shelter);
        assert!(told > 0.0 && told < veteran.worth(Conduct::Shelter) * T.rate, "{told}");
    }

    #[test]
    fn its_scores_say_what_it_learned() {
        #[derive(Debug, Clone, Copy, PartialEq)]
        enum Do {
            Shout,
            Shoot,
        }
        struct Nothing;
        impl Situation<()> for Nothing {
            fn input(&self, _: ()) -> f32 {
                1.0
            }
        }
        let mut r: Reasoner<Do, ()> = Reasoner::new(vec![Action::new(Do::Shout, 1.0), Action::new(Do::Shoot, 0.9).when((), Curve::Constant(1.0))], 0.0);
        let mut lessons = Lessons::new();
        lessons.learn(&T, Do::Shout, -1.0);
        assert_eq!(lessons.choose(&T, &mut r, &Nothing, None), Some(Do::Shoot));
        assert!((r.scores[0].1 - (1.0 - SWAY * RATE)).abs() < 1.0e-6);
    }
}
