//! What has worked: an NPC learning, from how each thing it did turned out, to do it more or less.
//!
//! [`Lessons`] keep, for each thing it could do, what it makes of it from -1 (it never works) to 1
//! (it always does), starting at nothing. Each time the game says how one turned out
//! ([`Lessons::learn`]), from -1 to 1 - a search that found the player, 1; a chase they got away
//! from, -1; a warning that sent them off, a half - it moves that way by the tuning's `rate`, so
//! that one fluke does not undo a habit, but a run of them does. What it learned fades back to
//! nothing ([`Lessons::step`]): a player who changes their ways is met afresh.
//!
//! How something turns out is seldom down to what an NPC is doing that moment alone: a search
//! finds the player because the chase before it kept them in sight. A [`Trail`] keeps what it did
//! lately, and an outcome told to it ([`Trail::turned_out`]) is taken by each of them it could be
//! down to, by how lately it did it - what it is doing, all of it; what it gave up the tuning's
//! `credit` seconds ago, half - so the game need only say how things went, not when it chose what.
//! [`crate::think::experience`] does this for the crate's own [`crate::tactics::conduct`].
//!
//! What it learned weighs a [`Reasoner`]'s choice ([`Lessons::choose`]): each action's score
//! times one and the tuning's `sway` of what it makes of it - by default from a half as much, for
//! what never works, to half as much again, for what always does. It only weighs what is open to
//! it: what its considerations shut, it still does not do, and what never worked it is less
//! inclined to, never barred from - so that, given nothing else, it tries it again.
//!
//! Once what it makes of something goes past the tuning's `sure` either way, it has come to a
//! [`View`] of it, and says so: each call that moves what it learned gives back a [`Verdict`] for
//! each action it came to a view of, or gave one up on - only back under half of `sure`, so that
//! it does not change its mind back and forth - for the game to say ("Chasing them is no use -
//! search the alleys"), or to tell the rest of its squad.
//!
//! One can tell another what it learned ([`Lessons::tell`]), who takes it as if it had happened
//! to it, by the tuning's `told` share: a squad comes to think alike.
//!
//! **Settling.** The more it has to go on about something, the less each new outcome moves it: the
//! first `free` outcomes count at the full `rate`, and each one after slows it by `settle`, down
//! to `least_rate`. So a few at the start teach it fast, and once it knows what works, a run of
//! bad luck does not talk it out of it - while `least_rate` still lets a world that has really
//! changed change its mind. What it has to go on fades as what it makes of it does, halfway in
//! the `half_life`: after a long while without, it learns fast again.
//!
//! **Trying the other way.** Settled, it would never again try what it once tried and was
//! unlucky with, if what it does instead is only middling. So what it knows a lot about is weighed
//! a little below what it knows little about - by up to the tuning's `explore` of its `sway`,
//! the more the more it has to go on - and a well-tried way that only does middling lets the
//! less-tried one have another go now and then. What it has never tried is weighed as nothing
//! learned: 1. Nothing random: the same history, the same choice.

use crate::tactics::utility::{Reasoner, Situation};

/// How far each outcome moves what an NPC makes of what it did, towards it: a quarter.
pub const RATE: f32 = 0.25;
/// How far what it makes of something weighs it, either way: by up to a half.
pub const SWAY: f32 = 0.5;
/// How long, in seconds, what it learned takes to fade halfway back to nothing: ten minutes.
pub const HALF_LIFE: f32 = 600.0;
/// How much of its rate what it is told counts for, against what happened to it: a half.
pub const TOLD: f32 = 0.5;
/// How far what it makes of something has to go, either way, for it to come to a view of it.
pub const SURE: f32 = 0.5;
/// How long, in seconds, after it gave something up an outcome counts half for it: ten seconds.
pub const CREDIT: f32 = 10.0;
/// How many of the things it did lately a [`Trail`] keeps.
pub const TRAIL: usize = 8;
/// How many outcomes count at the full rate, how much each one after slows it, and how slow it
/// gets at slowest.
pub const FREE: f32 = 4.0;
pub const SETTLE: f32 = 0.1;
pub const LEAST_RATE: f32 = 0.05;
/// How much less, of its `sway`, what it knows all about is weighed than what it knows nothing of.
pub const EXPLORE: f32 = 0.3;

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
        /// How far what it makes of something has to go for it to come to a view of it.
        sure: f32 = SURE,
        /// How long, in seconds, after it gave something up an outcome counts half for it.
        credit: f32 = CREDIT,
        /// How many outcomes count at the full rate, how much each one after slows it, and how
        /// slow it gets at slowest.
        free: f32 = FREE,
        settle: f32 = SETTLE,
        least_rate: f32 = LEAST_RATE,
        /// How much less, of its sway, what it knows all about is weighed than what it does not.
        explore: f32 = EXPLORE,
    }
);

/// What an NPC has come to make of something it does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum View {
    /// It works: it is the more inclined to it.
    Works,
    /// It is no use: it is the less inclined to it.
    Fails,
}

/// An NPC changed its mind about `action`: it came to `view` of it, or with `None`, gave up the
/// view it had.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Verdict<A> {
    /// What it changed its mind about.
    pub action: A,
    /// What it makes of it now, if anything.
    pub view: Option<View>,
    /// What it makes of it now, from -1 to 1.
    pub worth: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
struct Learned<A> {
    action: A,
    worth: f32,
    view: Option<View>,
    /// How much it has to go on: an outcome of its own, one; what it is told, or an outcome put
    /// down to it in part, less. Fading as `worth` does.
    #[cfg_attr(feature = "serde", serde(default))]
    evidence: f32,
}

impl<A: Copy> Learned<A> {
    /// Comes to a view, or gives one up, for what it makes of it now - giving back what changed.
    fn settle(&mut self, sure: f32) -> Option<Verdict<A>> {
        let sure = sure.max(1.0e-3);
        let view = if self.worth >= sure {
            Some(View::Works)
        } else if self.worth <= -sure {
            Some(View::Fails)
        } else {
            match self.view {
                Some(View::Works) if self.worth >= sure / 2.0 => Some(View::Works),
                Some(View::Fails) if self.worth <= -sure / 2.0 => Some(View::Fails),
                _ => None,
            }
        };
        (view != self.view).then(|| {
            self.view = view;
            Verdict { action: self.action, view, worth: self.worth }
        })
    }
}

/// What an NPC has learned of each thing it could do, from -1 (it never works) to 1 (it always
/// does): nothing for what it has never done, or long forgotten.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Lessons<A> {
    learned: Vec<Learned<A>>,
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
        self.learned.iter().find(|l| l.action == action).map_or(0.0, |l| l.worth)
    }

    /// What view it has come to of `action`, if any.
    pub fn view(&self, action: A) -> Option<View> {
        self.learned.iter().find(|l| l.action == action).and_then(|l| l.view)
    }

    /// Everything it has learned anything of, and what it makes of each.
    pub fn learned(&self) -> impl Iterator<Item = (A, f32)> + '_ {
        self.learned.iter().map(|l| (l.action, l.worth))
    }

    /// It did `action`, and it turned out `outcome`, from -1 (as badly as it could) to 1 (as
    /// well) - giving back whether it changed its mind about it for that.
    pub fn learn(&mut self, tuning: &Tuning, action: A, outcome: f32) -> Option<Verdict<A>> {
        self.move_towards(tuning, action, outcome, 1.0)
    }

    /// How much it has to go on about `action`: about how many outcomes of its own, lately.
    pub fn evidence(&self, action: A) -> f32 {
        self.learned.iter().find(|l| l.action == action).map_or(0.0, |l| l.evidence)
    }

    /// It tells `to` what it learned: each thing it makes anything of, taken as if it had turned
    /// out that way for them, by the tuning's `told` share of the rate - giving back what `to`
    /// changed its mind about for it.
    pub fn tell(&self, tuning: &Tuning, to: &mut Lessons<A>) -> Vec<Verdict<A>> {
        self.learned
            .iter()
            .filter_map(|l| to.move_towards(tuning, l.action, l.worth, tuning.told))
            .collect()
    }

    /// Moves it on `dt` seconds: what it learned fading back to nothing, halfway in the tuning's
    /// half-life - forgotten once next to nothing is left - giving back the views it gave up.
    pub fn step(&mut self, tuning: &Tuning, dt: f32) -> Vec<Verdict<A>> {
        let mut given_up = Vec::new();
        for learned in &mut self.learned {
            learned.worth = super::fade(learned.worth, tuning.half_life, dt);
            learned.evidence = super::fade(learned.evidence, tuning.half_life, dt);
            given_up.extend(learned.settle(tuning.sure));
        }
        self.learned.retain(|l| l.worth.abs() > 1.0e-3 || l.evidence > 1.0e-2);
        given_up
    }

    /// How many times as much `action` scores, for what it learned of it: from one less the
    /// tuning's sway, for what never works, to one and the sway, for what always does - less up to
    /// `explore` of the sway the better known it is (see the module's trying the other way). 1 for
    /// what it has never tried.
    pub fn bias(&self, tuning: &Tuning, action: A) -> f32 {
        // One look for it; never tried, nothing learned.
        let Some(learned) = self.learned.iter().find(|l| l.action == action) else {
            return 1.0;
        };
        // How well known it is: 0 never tried, toward 1 the more it has to go on.
        let known = 1.0 - 1.0 / (1.0 + learned.evidence.max(0.0)).sqrt();
        (1.0 + tuning.sway * (learned.worth - tuning.explore.max(0.0) * known)).max(0.0)
    }

    /// What `reasoner` chooses in `situation`, doing `current` already, with what it learned
    /// weighing each action (see [`Reasoner::choose_by`]).
    pub fn choose<I: Copy>(&self, tuning: &Tuning, reasoner: &mut Reasoner<A, I>, situation: &impl Situation<I>, current: Option<A>) -> Option<A> {
        self.choose_with(tuning, reasoner, situation, current, |_| 1.0)
    }

    /// As [`Lessons::choose`], each action weighed by `weigh` of it as well - by the situation,
    /// say (see [`crate::tactics::priority`]).
    pub fn choose_with<I: Copy>(
        &self,
        tuning: &Tuning,
        reasoner: &mut Reasoner<A, I>,
        situation: &impl Situation<I>,
        current: Option<A>,
        weigh: impl Fn(A) -> f32,
    ) -> Option<A> {
        reasoner.choose_by(situation, current, |action| self.bias(tuning, action) * weigh(action).max(0.0))
    }

    /// Moves what it makes of `action` toward `outcome`, the outcome counting `weight` of one of its
    /// own (what it is told, or an outcome put down to it in part, less), at the tuning's rate as
    /// slowed by what it has to go on already (see the module's settling).
    fn move_towards(&mut self, tuning: &Tuning, action: A, outcome: f32, weight: f32) -> Option<Verdict<A>> {
        let (outcome, weight) = (outcome.clamp(-1.0, 1.0), weight.clamp(0.0, 1.0));
        let rate = tuning.rate.clamp(0.0, 1.0);
        if outcome.is_nan() || weight.is_nan() || weight == 0.0 || rate == 0.0 {
            return None;
        }
        let learned = match self.learned.iter().position(|l| l.action == action) {
            Some(n) => &mut self.learned[n],
            None => {
                self.learned.push(Learned { action, worth: 0.0, view: None, evidence: 0.0 });
                self.learned.last_mut().unwrap()
            }
        };
        let over = (learned.evidence - tuning.free).max(0.0);
        let settled = (rate / (1.0 + tuning.settle.max(0.0) * over)).max(tuning.least_rate.clamp(0.0, rate));
        learned.worth += (outcome - learned.worth) * settled * weight;
        learned.evidence += weight;
        learned.settle(tuning.sure)
    }
}

/// What an NPC did lately, for an outcome to be put down to: each thing, and how long ago it gave
/// it up - nothing for what it is doing still.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Trail<A> {
    did: Vec<(A, f32)>,
    doing: bool,
}

impl<A> Default for Trail<A> {
    fn default() -> Self {
        Self { did: Vec::new(), doing: false }
    }
}

impl<A: Copy + PartialEq> Trail<A> {
    /// Nothing done yet.
    pub fn new() -> Self {
        Self::default()
    }

    /// It is doing `action` - as it was, or newly, giving up what it was doing. Fine to call every
    /// frame with what it chose.
    pub fn doing(&mut self, action: A) {
        if self.doing && self.did.last().is_some_and(|&(a, _)| a == action) {
            return;
        }
        self.did.retain(|&(a, _)| a != action);
        if self.did.len() >= TRAIL {
            self.did.remove(0);
        }
        self.did.push((action, 0.0));
        self.doing = true;
    }

    /// It gave up what it was doing, for nothing it chose: held, down, or talked to.
    pub fn stopped(&mut self) {
        self.doing = false;
    }

    /// What it is doing, if anything.
    pub fn current(&self) -> Option<A> {
        self.did.last().filter(|_| self.doing).map(|&(a, _)| a)
    }

    /// How much of an outcome now would be put down to `action`: all of it for what it is doing,
    /// half for what it gave up the tuning's `credit` seconds ago, none for what it has not done.
    pub fn share(&self, tuning: &Tuning, action: A) -> f32 {
        self.did.iter().find(|&&(a, _)| a == action).map_or(0.0, |&(_, since)| Self::credit(tuning, since))
    }

    /// Moves it on `dt` seconds: what it gave up, longer ago - forgotten once an outcome would
    /// count for less than a sixteenth for it.
    pub fn step(&mut self, tuning: &Tuning, dt: f32) {
        let doing = self.doing as usize;
        let ended = self.did.len() - doing.min(self.did.len());
        for (_, since) in &mut self.did[..ended] {
            *since += dt.max(0.0);
        }
        let forget = tuning.credit.max(1.0e-3) * 4.0;
        self.did.retain(|&(_, since)| since < forget);
    }

    /// What it did lately turned out `outcome`, from -1 to 1: each thing it did that the outcome
    /// is down to (`down_to`) learns it, in `lessons`, by its share of it - giving back what it
    /// changed its mind about for it. What it did that the outcome is nothing to do with is left
    /// for its own. Each is put down to once: what it gave up is done with, and only what it is
    /// doing still takes the next outcome.
    pub fn turned_out(&mut self, tuning: &Tuning, lessons: &mut Lessons<A>, outcome: f32, down_to: impl Fn(A) -> bool) -> Vec<Verdict<A>> {
        let verdicts = self
            .did
            .iter()
            .filter(|&&(action, _)| down_to(action))
            .filter_map(|&(action, since)| lessons.move_towards(tuning, action, outcome, Self::credit(tuning, since)))
            .collect();
        let current = self.current();
        self.did.retain(|&(a, _)| Some(a) == current || !down_to(a));
        verdicts
    }

    /// How many things it did lately it keeps.
    pub fn len(&self) -> usize {
        self.did.len()
    }

    /// Whether it did nothing lately.
    pub fn is_empty(&self) -> bool {
        self.did.is_empty()
    }

    fn credit(tuning: &Tuning, since: f32) -> f32 {
        super::fade(1.0, tuning.credit, since)
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
    fn it_comes_to_a_view_and_does_not_change_its_mind_back_and_forth() {
        let mut lessons = Lessons::new();
        assert!(lessons.learn(&T, Conduct::Search, 1.0).is_none(), "once is not a view");
        let came = (0..4).find_map(|_| lessons.learn(&T, Conduct::Search, 1.0)).expect("a run is");
        assert_eq!((came.action, came.view), (Conduct::Search, Some(View::Works)));
        assert!(came.worth >= SURE);
        // Back under sure, but not under half of it: it keeps its view, and says nothing.
        while lessons.worth(Conduct::Search) >= SURE {
            assert!(lessons.step(&T, 10.0).is_empty());
        }
        assert_eq!(lessons.view(Conduct::Search), Some(View::Works));
        // Under half of it, it gives it up, once.
        let mut given_up = Vec::new();
        while lessons.worth(Conduct::Search) >= SURE / 2.0 {
            given_up.extend(lessons.step(&T, 10.0));
        }
        given_up.extend(lessons.step(&T, 10.0));
        assert_eq!(given_up.iter().map(|v| v.view).collect::<Vec<_>>(), vec![None]);
        // A run of failures brings it to the other view.
        let failed: Vec<_> = (0..6).filter_map(|_| lessons.learn(&T, Conduct::Search, -1.0)).collect();
        assert_eq!(failed.last().map(|v| v.view), Some(Some(View::Fails)));
    }

    #[test]
    fn a_trail_keeps_what_it_did_lately_once_each() {
        let mut trail = Trail::new();
        for _ in 0..3 {
            trail.doing(Conduct::Chase);
        }
        assert_eq!(trail.len(), 1);
        for conduct in [Conduct::Fight, Conduct::Search, Conduct::Investigate, Conduct::Report, Conduct::Shelter, Conduct::BackAway, Conduct::Watch, Conduct::Mourn] {
            trail.doing(conduct);
        }
        assert_eq!(trail.len(), TRAIL);
        assert_eq!(trail.share(&T, Conduct::Chase), 0.0, "the oldest went");
        assert_eq!(trail.current(), Some(Conduct::Mourn));
        assert_eq!(trail.share(&T, Conduct::Mourn), 1.0);
        // What it is doing does not age; what it gave up does.
        trail.step(&T, CREDIT);
        assert_eq!(trail.share(&T, Conduct::Mourn), 1.0);
        assert!((trail.share(&T, Conduct::Watch) - 0.5).abs() < 1.0e-6);
        trail.stopped();
        trail.step(&T, CREDIT * 4.0);
        assert!(trail.is_empty());
    }

    #[test]
    fn well_learned_it_is_not_talked_out_of_it_by_a_run_of_bad_luck() {
        // Thirty encounters of a way that works seven times in ten; then three failures in a row.
        let mut lessons = Lessons::new();
        for n in 0..30 {
            lessons.learn(&T, Conduct::Search, if n % 10 < 7 { 1.0 } else { -0.5 });
        }
        let settled = lessons.worth(Conduct::Search);
        for _ in 0..3 {
            lessons.learn(&T, Conduct::Search, -0.5);
        }
        let after = lessons.worth(Conduct::Search);
        assert!(after > 0.25, "still thought well of: {settled} -> {after}");
        assert!(lessons.evidence(Conduct::Search) > 30.0);
        // Not settled - the full rate every time - the same run would have taken it below a quarter.
        let unsettled = Tuning { free: f32::INFINITY, ..T };
        let mut raw = Lessons::new();
        for n in 0..30 {
            raw.learn(&unsettled, Conduct::Search, if n % 10 < 7 { 1.0 } else { -0.5 });
        }
        for _ in 0..3 {
            raw.learn(&unsettled, Conduct::Search, -0.5);
        }
        assert!(raw.worth(Conduct::Search) < 0.25, "{}", raw.worth(Conduct::Search));
    }

    #[test]
    fn settled_it_still_changes_its_mind_if_the_world_really_changes() {
        let mut lessons = Lessons::new();
        for _ in 0..60 {
            lessons.learn(&T, Conduct::Search, 1.0);
        }
        assert_eq!(lessons.view(Conduct::Search), Some(View::Works));
        // From now on it never works: in time it comes round, however settled.
        let mut took = None;
        for n in 0..200 {
            lessons.learn(&T, Conduct::Search, -1.0);
            if lessons.view(Conduct::Search) == Some(View::Fails) {
                took = Some(n + 1);
                break;
            }
        }
        let took = took.expect("it came round");
        assert!(took <= 60, "{took} failures to come round");
        // Fewer once a while has passed without: what it has to go on fades.
        let mut rested = Lessons::new();
        for _ in 0..60 {
            rested.learn(&T, Conduct::Search, 1.0);
        }
        rested.step(&T, HALF_LIFE * 3.0);
        let mut sooner = 0;
        while rested.worth(Conduct::Search) > -SURE {
            rested.learn(&T, Conduct::Search, -1.0);
            sooner += 1;
        }
        assert!(sooner < took, "{sooner} against {took}");
    }

    #[test]
    fn a_well_known_middling_way_lets_a_less_tried_one_have_another_go() {
        let mut lessons = Lessons::new();
        for n in 0..30 {
            lessons.learn(&T, Conduct::Investigate, if n % 10 < 3 { 1.0 } else { -0.5 });
        }
        // Middling, and well known: weighed below one it knows nothing of.
        assert!(lessons.worth(Conduct::Investigate).abs() < 0.2);
        assert!(lessons.bias(&T, Conduct::Investigate) < lessons.bias(&T, Conduct::Search));
        assert_eq!(lessons.bias(&T, Conduct::Search), 1.0, "never tried: nothing learned");
        // Switched off, nothing of the kind.
        let off = Tuning { sway: 0.0, ..T };
        assert_eq!(lessons.bias(&off, Conduct::Investigate), 1.0);
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
        assert!((r.scores[0].1 - lessons.bias(&T, Do::Shout)).abs() < 1.0e-6);
        // One failure: worth -RATE, and a little more for being known now.
        let known = 1.0 - 1.0 / 2.0f32.sqrt();
        assert!((lessons.bias(&T, Do::Shout) - (1.0 - SWAY * (RATE + EXPLORE * known))).abs() < 1.0e-6);
    }
}
