//! Evaluating learning: whether what an NPC changed in how it goes about things made it do better
//! - or only made it different.
//!
//! [`super::lesson`] and [`super::experience`] change what an NPC does by how things turned out,
//! and [`super::memory`] by what happened. That they change it is easy to see; that the change
//! helped is not, because outcomes are noisy: a search that found the player once proves little.
//! An [`Evaluation`] keeps the evidence.
//!
//! - Each attempt is an approach and its outcome ([`Evaluation::attempt`]): a number, the higher
//!   the better - 1 or 0 for found or not, the seconds it took to find them less than nothing.
//!   Each approach keeps a [`Tally`]: how many attempts, their mean, how spread out.
//! - What it does as a rule - its **habit** - is the approach it took most in its last `window`
//!   attempts. One try of something else is not a change of habit; a run of them is. Each change
//!   of habit is an [`Adaptation`]: from what, to what, after how many attempts, with a tally of the
//!   outcomes under the old habit and one of those since.
//! - [`Evaluation::judge`] says whether an adaptation did better: only when the difference between
//!   the two tallies is greater than chance would make it, at the confidence asked for (`z`, 1.96
//!   for 95%), and only once each side has `least` attempts. Otherwise there is
//!   [`Judgement::NoClearDifference`] - so a change that happened to be followed by a lucky run is
//!   not taken for learning.
//! - [`Evaluation::stability`] is how often its habit changes, per hundred attempts: a learner that
//!   settles on what works changes rarely; one chasing noise, often.
//!
//! The comparison is Welch's: the difference of the two means over its standard error, each side's
//! variance its own but no less than both sides' together. It is a normal approximation, good
//! from a dozen or so attempts a side.

/// Running figures of one set of outcomes.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Tally {
    /// How many outcomes.
    pub n: u32,
    sum: f64,
    sum_sq: f64,
}

impl Tally {
    /// Adds `outcome`; one that is not a number is let go.
    pub fn add(&mut self, outcome: f32) {
        if outcome.is_finite() {
            let x = outcome as f64;
            self.n += 1;
            self.sum += x;
            self.sum_sq += x * x;
        }
    }

    /// The mean outcome; none with none.
    pub fn mean(&self) -> Option<f32> {
        (self.n > 0).then(|| (self.sum / self.n as f64) as f32)
    }

    /// How spread out the outcomes are: their sample variance; none with fewer than two.
    pub fn variance(&self) -> Option<f32> {
        (self.n > 1).then(|| {
            let n = self.n as f64;
            ((self.sum_sq - self.sum * self.sum / n) / (n - 1.0)).max(0.0) as f32
        })
    }

    /// How far either side of the mean the true mean likely is, at confidence `z` (1.96: 95%):
    /// `(low, high)`; none with fewer than two.
    pub fn interval(&self, z: f32) -> Option<(f32, f32)> {
        let (mean, variance) = (self.mean()?, self.variance()?);
        let half = z.max(0.0) * (variance / self.n as f32).sqrt();
        Some((mean - half, mean + half))
    }
}

/// Whether an adaptation did better.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Judgement {
    /// Better on the whole by `by`, more than chance would make it.
    Better { by: f32 },
    /// Worse on the whole by `by`, more than chance would make it.
    Worse { by: f32 },
    /// No difference greater than chance would make - or too few attempts yet to tell.
    NoClearDifference,
}

/// A change of habit: from one approach to another, and how things went under each.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Adaptation<A> {
    /// The habit before, if it had one, and the habit after.
    pub from: Option<A>,
    pub to: A,
    /// After how many attempts in all it changed.
    pub at: u32,
    /// The outcomes under the old habit (since the change before), and since this one.
    pub before: Tally,
    pub after: Tally,
}

/// How many recent attempts its habit is taken from, by default.
pub const WINDOW: usize = 10;

/// The evidence of how an NPC's approaches have gone, and of how its habits changed.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Evaluation<A> {
    approaches: Vec<(A, Tally)>,
    recent: Vec<A>,
    window: usize,
    habit: Option<A>,
    adaptations: Vec<Adaptation<A>>,
    /// Outcomes under its habit, since it last changed.
    current: Tally,
    attempts: u32,
}

impl<A> Default for Evaluation<A> {
    fn default() -> Self {
        Self::with_window(WINDOW)
    }
}

impl<A> Evaluation<A> {
    /// Its habit taken from its last `window` attempts (at least one).
    pub fn with_window(window: usize) -> Self {
        Self { approaches: Vec::new(), recent: Vec::new(), window: window.max(1), habit: None, adaptations: Vec::new(), current: Tally::default(), attempts: 0 }
    }
}

impl<A: Copy + PartialEq> Evaluation<A> {
    /// Nothing attempted yet.
    pub fn new() -> Self {
        Self::default()
    }

    /// It went about something by `approach`, and it turned out `outcome`, the higher the better.
    /// An outcome that is not a number is let go, attempt and all.
    pub fn attempt(&mut self, approach: A, outcome: f32) {
        if !outcome.is_finite() {
            return;
        }
        self.attempts += 1;
        match self.approaches.iter_mut().find(|(a, _)| *a == approach) {
            Some((_, tally)) => tally.add(outcome),
            None => {
                let mut tally = Tally::default();
                tally.add(outcome);
                self.approaches.push((approach, tally));
            }
        }
        self.recent.push(approach);
        if self.recent.len() > self.window {
            self.recent.remove(0);
        }
        // Its habit: what it did most lately - the newest of those tied.
        let mut habit = None;
        let mut most = 0;
        for &a in self.recent.iter().rev() {
            let count = self.recent.iter().filter(|&&b| b == a).count();
            if count > most {
                (habit, most) = (Some(a), count);
            }
        }
        // Only a majority of a full window is a habit, to begin with; after that, it changes only
        // once another approach has taken over the window.
        let settled = self.recent.len() >= self.window && most * 2 > self.window;
        match (self.habit, habit) {
            (None, Some(new)) if settled => {
                self.habit = Some(new);
                self.current = Tally::default();
            }
            (Some(old), Some(new)) if settled && old != new => {
                self.adaptations.push(Adaptation { from: Some(old), to: new, at: self.attempts, before: self.current, after: Tally::default() });
                self.habit = Some(new);
                self.current = Tally::default();
            }
            _ => (),
        }
        self.current.add(outcome);
        if let Some(last) = self.adaptations.last_mut().filter(|last| Some(last.to) == self.habit) {
            last.after = self.current;
        }
    }

    /// What it does as a rule now, if it has settled on anything.
    pub fn habit(&self) -> Option<A> {
        self.habit
    }

    /// How `approach` has gone, every time it was taken.
    pub fn tally(&self, approach: A) -> Tally {
        self.approaches.iter().find(|(a, _)| *a == approach).map_or_else(Tally::default, |&(_, t)| t)
    }

    /// Each change of habit, in order.
    pub fn adaptations(&self) -> &[Adaptation<A>] {
        &self.adaptations
    }

    /// How many attempts in all.
    pub fn attempts(&self) -> u32 {
        self.attempts
    }

    /// How often its habit has changed, per hundred attempts.
    pub fn stability(&self) -> f32 {
        if self.attempts == 0 {
            0.0
        } else {
            self.adaptations.len() as f32 * 100.0 / self.attempts as f32
        }
    }

    /// Whether `adaptation` did better: see [`judge`].
    pub fn judge(&self, adaptation: &Adaptation<A>, z: f32, least: u32) -> Judgement {
        judge(&adaptation.before, &adaptation.after, z, least)
    }
}

/// Whether `after` did better than `before`: by Welch's test, the difference of their means over
/// its standard error past `z` (1.96 for 95% confidence), with at least `least` outcomes each
/// (and two, to have a spread) - each side's variance taken as no less than both sides' together,
/// so that a small side that happened to come out all the same is not taken as certain. Two sides
/// without any spread at all, together, are judged by their means.
pub fn judge(before: &Tally, after: &Tally, z: f32, least: u32) -> Judgement {
    let enough = least.max(2);
    if before.n < enough || after.n < enough {
        return Judgement::NoClearDifference;
    }
    let (Some(a), Some(b), Some(va), Some(vb)) = (before.mean(), after.mean(), before.variance(), after.variance()) else {
        return Judgement::NoClearDifference;
    };
    // Neither side's spread is taken for less than both sides' together: five successes in five
    // do not make it sure of always succeeding. Under no difference at all, that is the spread to
    // expect; so a few lucky outcomes with no spread among them cannot make a change look clear.
    let both = Tally { n: before.n + after.n, sum: before.sum + after.sum, sum_sq: before.sum_sq + after.sum_sq };
    let pooled = both.variance().unwrap_or(0.0);
    let (va, vb) = (va.max(pooled), vb.max(pooled));
    let by = b - a;
    let error = (va / before.n as f32 + vb / after.n as f32).sqrt();
    let clear = if error > 1.0e-9 { by.abs() / error > z.max(0.0) } else { by.abs() > 1.0e-6 };
    match (clear, by > 0.0) {
        (true, true) => Judgement::Better { by },
        (true, false) => Judgement::Worse { by: -by },
        _ => Judgement::NoClearDifference,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::navigation::grid::Rng;

    #[derive(Debug, Clone, Copy, PartialEq)]
    enum Way {
        Chase,
        Search,
    }

    fn coin(rng: &mut Rng, p: f32) -> f32 {
        if (rng.below(10_000) as f32) < p * 10_000.0 {
            1.0
        } else {
            0.0
        }
    }

    #[test]
    fn a_tally_keeps_mean_spread_and_how_sure_of_the_mean() {
        let mut t = Tally::default();
        assert_eq!((t.mean(), t.variance(), t.interval(1.96)), (None, None, None));
        for x in [2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0] {
            t.add(x);
        }
        t.add(f32::NAN);
        assert_eq!(t.n, 8);
        assert_eq!(t.mean(), Some(5.0));
        assert!((t.variance().unwrap() - 32.0 / 7.0).abs() < 1.0e-5);
        let (lo, hi) = t.interval(1.96).unwrap();
        assert!(lo < 5.0 && hi > 5.0 && ((hi - lo) / 2.0 - 1.96 * (32.0f32 / 7.0 / 8.0).sqrt()).abs() < 1.0e-5);
    }

    #[test]
    fn a_habit_changes_with_a_run_not_a_single_try() {
        let mut e = Evaluation::with_window(5);
        for _ in 0..5 {
            e.attempt(Way::Chase, 0.0);
        }
        assert_eq!(e.habit(), Some(Way::Chase));
        // One try of something else is no change of habit.
        e.attempt(Way::Search, 1.0);
        e.attempt(Way::Chase, 0.0);
        assert!(e.adaptations().is_empty());
        // A run of it is: at the ninth attempt three of the last five are searches.
        for _ in 0..3 {
            e.attempt(Way::Search, 1.0);
        }
        assert_eq!(e.habit(), Some(Way::Search));
        let a = e.adaptations()[0];
        assert_eq!((a.from, a.to, a.at), (Some(Way::Chase), Way::Search, 9));
        assert_eq!(a.before.n, 4, "the old habit's outcomes, from when it formed, the odd try and all");
        e.attempt(Way::Search, 1.0);
        assert_eq!(e.adaptations()[0].after.n, 3, "and since");
        assert_eq!(e.tally(Way::Search).n, 5);
        assert!((e.stability() - 100.0 / 11.0).abs() < 1.0e-4);
    }

    #[test]
    fn a_real_improvement_is_judged_better_and_a_real_decline_worse() {
        let mut rng = Rng::new(5);
        let (mut before, mut after) = (Tally::default(), Tally::default());
        for _ in 0..60 {
            before.add(coin(&mut rng, 0.3));
            after.add(coin(&mut rng, 0.7));
        }
        assert!(matches!(judge(&before, &after, 1.96, 10), Judgement::Better { by } if by > 0.25));
        assert!(matches!(judge(&after, &before, 1.96, 10), Judgement::Worse { by } if by > 0.25));
        // Too few attempts yet: it says nothing.
        let (mut few_a, mut few_b) = (Tally::default(), Tally::default());
        for _ in 0..3 {
            few_a.add(0.0);
            few_b.add(1.0);
        }
        assert_eq!(judge(&few_a, &few_b, 1.96, 10), Judgement::NoClearDifference);
    }

    #[test]
    fn a_change_followed_by_luck_is_seldom_taken_for_learning() {
        // Both the same, 50:50, many times over: judged better about as rarely as 95% confidence
        // says it should be - a fortieth of the time, one way - and never most of the time.
        let mut rng = Rng::new(17);
        let trials = 1000;
        let mut better = 0;
        for _ in 0..trials {
            let (mut before, mut after) = (Tally::default(), Tally::default());
            for _ in 0..30 {
                before.add(coin(&mut rng, 0.5));
                after.add(coin(&mut rng, 0.5));
            }
            if matches!(judge(&before, &after, 1.96, 10), Judgement::Better { .. }) {
                better += 1;
            }
        }
        let rate = better as f32 / trials as f32;
        assert!(rate < 0.05, "judged better {rate:.3} of the time with no difference at all");
    }

    #[test]
    fn a_few_lucky_outcomes_all_alike_are_not_taken_as_certain() {
        // Five successes in five, then a long run at 64%: no clear decline - five is too few.
        let (mut before, mut after) = (Tally::default(), Tally::default());
        for _ in 0..5 {
            before.add(1.0);
        }
        for n in 0..136 {
            after.add(if n % 25 < 16 { 1.0 } else { 0.0 });
        }
        assert_eq!(judge(&before, &after, 1.96, 5), Judgement::NoClearDifference);
        // Fifty in fifty would be another matter.
        let mut many = Tally::default();
        for _ in 0..50 {
            many.add(1.0);
        }
        assert!(matches!(judge(&many, &after, 1.96, 5), Judgement::Worse { .. }));
    }

    #[test]
    fn nonsense_is_let_go() {
        let mut e = Evaluation::<Way>::new();
        e.attempt(Way::Chase, f32::NAN);
        e.attempt(Way::Chase, f32::INFINITY);
        assert_eq!((e.attempts(), e.stability(), e.habit()), (0, 0.0, None));
        let still = Tally::default();
        let mut same = Tally::default();
        for _ in 0..20 {
            same.add(1.0);
        }
        assert_eq!(judge(&same, &same, 1.96, 10), Judgement::NoClearDifference);
        assert_eq!(judge(&still, &same, 1.96, 10), Judgement::NoClearDifference);
        let mut higher = Tally::default();
        for _ in 0..20 {
            higher.add(2.0);
        }
        assert_eq!(judge(&same, &higher, 1.96, 10), Judgement::Better { by: 1.0 }, "no spread either side: the means tell");
    }
}
