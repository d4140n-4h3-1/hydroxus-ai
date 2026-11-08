//! Utility scoring: an NPC weighs what it could do against what it takes in, and does whatever
//! scores best - rather than a state machine's fixed `if this, then that`.
//!
//! Each [`Action`] it could take has a weight and [`Consideration`]s: each reads one input of its
//! situation - whether it sees the player, how far they are, how long it has left to search - and
//! turns it into a number from 0 to 1 through a response [`Curve`]. An action's score is its
//! weight times all of its considerations, made up a little for having more of them (Dave Mark's
//! compensation, so that an action with five considerations is not marked down for the fifth). A
//! [`Reasoner`] scores them all and picks the best, favouring the one it is doing already by its
//! `inertia`, so that two near-equal choices do not flicker one into the other.
//!
//! The inputs are named by the user's own type - an enum, say - and read from its own situation
//! through [`Situation`], so a reasoner is plain data: it can be built in code, changed by a
//! game's designer, or read from a file with the `serde` feature. It keeps what it last scored,
//! for a game to show why an NPC did what it did.

/// How a consideration turns an input into a score, from 0 to 1.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Curve {
    /// 1 over `at`, 0 at it and under.
    Above { at: f32 },
    /// 1 under `at`, 0 at it and over.
    Below { at: f32 },
    /// From 0 at `from` to 1 at `to`, in a straight line, and no further either way. `to` can be
    /// less than `from`, for less the more.
    Linear { from: f32, to: f32 },
    /// An S from 0 to 1: a half at `middle`, the steeper the greater `steepness` (less than
    /// nothing for less the more).
    Logistic { middle: f32, steepness: f32 },
    /// The same whatever the input.
    Constant(f32),
}

impl Curve {
    /// The score for `x`, from 0 to 1.
    pub fn score(self, x: f32) -> f32 {
        let score = match self {
            Curve::Above { at } => f32::from(u8::from(x > at)),
            Curve::Below { at } => f32::from(u8::from(x < at)),
            Curve::Linear { from, to } if (to - from).abs() < f32::EPSILON => f32::from(u8::from(x >= to)),
            Curve::Linear { from, to } => (x - from) / (to - from),
            Curve::Logistic { middle, steepness } => 1.0 / (1.0 + (-steepness * (x - middle)).exp()),
            Curve::Constant(score) => score,
        };
        if score.is_nan() {
            0.0
        } else {
            score.clamp(0.0, 1.0)
        }
    }
}

/// What a reasoner reads its inputs from: an NPC's situation, by the inputs' names.
pub trait Situation<I> {
    /// The value of `input`, as it is now.
    fn input(&self, input: I) -> f32;
}

/// One thing an action turns on: an input, through a curve.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Consideration<I> {
    pub input: I,
    pub curve: Curve,
}

impl<I: Copy> Consideration<I> {
    pub fn new(input: I, curve: Curve) -> Self {
        Self { input, curve }
    }
}

/// Something an NPC could do: how much it is worth at best, and what that turns on.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Action<A, I> {
    pub action: A,
    pub weight: f32,
    pub considerations: Vec<Consideration<I>>,
}

impl<A: Copy, I: Copy> Action<A, I> {
    /// `action`, worth `weight` at best, turning on nothing yet.
    pub fn new(action: A, weight: f32) -> Self {
        Self { action, weight, considerations: Vec::new() }
    }

    /// The action, turning on `input` through `curve` as well.
    pub fn when(mut self, input: I, curve: Curve) -> Self {
        self.considerations.push(Consideration::new(input, curve));
        self
    }

    /// What it scores in `situation`: its weight times every consideration's score, each made
    /// up a little towards 1 the more of them there are, so that more is not less.
    pub fn score(&self, situation: &impl Situation<I>) -> f32 {
        let n = self.considerations.len();
        if n == 0 {
            return self.weight;
        }
        let makeup = 1.0 - 1.0 / n as f32;
        let mut score = 1.0;
        for consideration in &self.considerations {
            let s = consideration.curve.score(situation.input(consideration.input));
            score *= s + (1.0 - s) * makeup * s;
            if score <= 0.0 {
                return 0.0;
            }
        }
        self.weight * score
    }
}

/// Chooses what to do: every action scored, the best taken.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Reasoner<A, I> {
    pub actions: Vec<Action<A, I>>,
    /// How much more the action it is doing already counts, as a share of its score: 0.1, a
    /// tenth more.
    pub inertia: f32,
    /// What each action scored the last time it chose - any bias in, its inertia left out - in the order of
    /// `actions`: for a game to show why it did what it did.
    #[cfg_attr(feature = "serde", serde(skip, default = "Vec::new"))]
    pub scores: Vec<(A, f32)>,
}

impl<A: Copy + PartialEq, I: Copy> Reasoner<A, I> {
    /// A reasoner choosing between `actions`, favouring what it is doing already by `inertia`.
    pub fn new(actions: Vec<Action<A, I>>, inertia: f32) -> Self {
        Self { actions, inertia, scores: Vec::new() }
    }

    /// The best of its actions in `situation`, doing `current` already - none if it has none, or
    /// none scores anything. The first listed wins a tie.
    pub fn choose(&mut self, situation: &impl Situation<I>, current: Option<A>) -> Option<A> {
        self.choose_by(situation, current, |_| 1.0)
    }

    /// As [`Reasoner::choose`], each action's score times `bias` of it as well - what the NPC has
    /// learned of it, say (see [`crate::think::lesson`]). A bias can weigh an action up or down,
    /// but never opens one its considerations shut. What it scored, kept in `scores`, has it in.
    pub fn choose_by(&mut self, situation: &impl Situation<I>, current: Option<A>, bias: impl Fn(A) -> f32) -> Option<A> {
        self.scores.clear();
        self.scores.extend(self.actions.iter().map(|a| (a.action, a.score(situation) * bias(a.action).max(0.0))));
        let mut best: Option<(A, f32)> = None;
        for &(action, score) in &self.scores {
            let score = if Some(action) == current { score * (1.0 + self.inertia) } else { score };
            if score > 0.0 && best.is_none_or(|(_, b)| score > b) {
                best = Some((action, score));
            }
        }
        best.map(|(action, _)| action)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, Copy, PartialEq)]
    enum In {
        Hunger,
        Danger,
    }

    struct World {
        hunger: f32,
        danger: f32,
    }

    impl Situation<In> for World {
        fn input(&self, input: In) -> f32 {
            match input {
                In::Hunger => self.hunger,
                In::Danger => self.danger,
            }
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq)]
    enum Do {
        Eat,
        Flee,
        Idle,
    }

    fn reasoner() -> Reasoner<Do, In> {
        Reasoner::new(
            vec![
                Action::new(Do::Eat, 1.0).when(In::Hunger, Curve::Linear { from: 0.0, to: 1.0 }).when(In::Danger, Curve::Below { at: 0.5 }),
                Action::new(Do::Flee, 1.0).when(In::Danger, Curve::Logistic { middle: 0.5, steepness: 12.0 }),
                Action::new(Do::Idle, 0.2),
            ],
            0.1,
        )
    }

    #[test]
    fn the_curves_give_scores_from_0_to_1() {
        assert_eq!(Curve::Above { at: 0.0 }.score(0.1), 1.0);
        assert_eq!(Curve::Above { at: 0.0 }.score(0.0), 0.0);
        assert_eq!(Curve::Below { at: 2.0 }.score(1.0), 1.0);
        assert_eq!(Curve::Linear { from: 10.0, to: 20.0 }.score(15.0), 0.5);
        assert_eq!(Curve::Linear { from: 20.0, to: 10.0 }.score(5.0), 1.0, "less the more, and no further");
        assert!((Curve::Logistic { middle: 3.0, steepness: 2.0 }.score(3.0) - 0.5).abs() < 1e-6);
        assert_eq!(Curve::Linear { from: 0.0, to: 1.0 }.score(f32::NAN), 0.0);
    }

    #[test]
    fn it_does_what_scores_best_for_what_it_takes_in() {
        let mut r = reasoner();
        assert_eq!(r.choose(&World { hunger: 0.9, danger: 0.0 }, None), Some(Do::Eat));
        assert_eq!(r.choose(&World { hunger: 0.9, danger: 0.9 }, None), Some(Do::Flee), "danger first");
        assert_eq!(r.choose(&World { hunger: 0.0, danger: 0.0 }, None), Some(Do::Idle));
        // It says why: what each scored.
        assert_eq!(r.scores.iter().map(|&(a, _)| a).collect::<Vec<_>>(), vec![Do::Eat, Do::Flee, Do::Idle]);
        assert_eq!(r.scores[2].1, 0.2);
    }

    #[test]
    fn more_considerations_are_not_less_and_one_at_nothing_is_nothing() {
        let one = Action::new(Do::Eat, 1.0).when(In::Hunger, Curve::Constant(0.8));
        let two = one.clone().when(In::Danger, Curve::Constant(0.8));
        let world = World { hunger: 0.0, danger: 0.0 };
        assert!(two.score(&world) > 0.8 * 0.8, "made up for the second");
        assert_eq!(two.clone().when(In::Danger, Curve::Constant(0.0)).score(&world), 0.0);
    }

    #[test]
    fn it_keeps_to_what_it_is_doing_over_what_is_barely_better() {
        let mut r = Reasoner::new(vec![Action::new(Do::Eat, 0.50), Action::new(Do::Idle, 0.52)], 0.1);
        let world = World { hunger: 0.0, danger: 0.0 };
        assert_eq!(r.choose(&world, Some(Do::Eat)), Some(Do::Eat));
        assert_eq!(r.choose(&world, None), Some(Do::Idle));
    }
}
