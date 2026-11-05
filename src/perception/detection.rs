//! Becoming aware of someone, gradually: Fallout's sneaking, `[HIDDEN]`, `[CAUTION]`, `[DANGER]`.
//!
//! An NPC does not see the player the moment they are in view. Its awareness of them fills while
//! it can see them, the quicker the more visible they are (see
//! [`Sight::visibility`](crate::perception::sight::Sight::visibility)) - in the middle of its view
//! and close by, in a moment; at the edge of how far it sees, out of the corner of its eye,
//! crouched or in the dark, far more slowly - and the quicker again the more it is looking for
//! them already (`eagerness`). Out of sight a while, it drains away. Part full, it has noticed
//! something ([`Stage::Caution`]); full, it has spotted them ([`Stage::Danger`]), and a game would
//! have it go after them.
//!
//! What it hears adds to it too ([`Detection::hear`]): a noise close by - footfalls, a landing -
//! more than one at the edge of where it carries, and more again the keener it is looking. Enough
//! of them, and it is as sure someone is there as if it had seen them.
//!
//! A game keeps one [`Detection`] for each NPC that watches, steps it each frame with how visible
//! the player is to it, and shows the most aware of them ([`most_aware`]) as a meter.

tuning!(
    /// How quickly an NPC becomes aware of someone, and forgets them.
    {
        /// How long it takes to spot someone fully visible, from nothing, in seconds.
        fill: f32 = FILL,
        /// How long out of sight before awareness starts to drain, in seconds, and how much of it
        /// drains a second after that.
        forget_after: f32 = FORGET_AFTER,
        drain: f32 = DRAIN,
        /// How aware it has to be to have noticed something: [`Stage::Caution`].
        caution: f32 = CAUTION,
        /// How many times as quickly it becomes aware of someone it is searching for already, as
        /// `eagerness`, than calm.
        searching: f32 = SEARCHING,
        /// How much awareness a noise right beside it adds, of all of it: less the further off
        /// along the floor it was (see [`Heard::nearness`](crate::perception::hearing::Heard::nearness)).
        hearing: f32 = HEARING,
    }
);

/// How long it takes to spot someone fully visible, from nothing, in seconds.
pub const FILL: f32 = 1.2;
/// How long out of sight before awareness drains, in seconds, and how much of it drains a second.
pub const FORGET_AFTER: f32 = 1.0;
pub const DRAIN: f32 = 0.35;
/// How aware an NPC has to be, from 0 to 1, to have noticed something.
pub const CAUTION: f32 = 0.25;
/// How many times as quickly one searching becomes aware of someone, as calm.
pub const SEARCHING: f32 = 3.0;
/// How much awareness a noise right beside an NPC adds, of all of it.
pub const HEARING: f32 = 0.6;

/// How aware an NPC is of someone, as a sneaking player is shown it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Stage {
    /// Unaware.
    Hidden,
    /// It has noticed something, and is becoming aware of them.
    Caution,
    /// It has spotted them.
    Danger,
}

/// What a step of awareness came to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Change {
    /// It has noticed something: from Hidden to Caution.
    Noticed,
    /// It has spotted them: to Danger.
    Spotted,
    /// It has lost interest: back to Hidden.
    Lost,
}

/// One NPC's awareness of someone.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Detection {
    /// How aware it is, from 0 to 1: 1, it has spotted them.
    pub level: f32,
    /// How long since it last saw them, in seconds.
    unseen: f32,
}

impl Detection {
    /// Its stage, as `tuning` has it.
    pub fn stage(&self, tuning: &Tuning) -> Stage {
        stage_at(tuning, self.level)
    }

    /// Whether it has spotted them.
    pub fn spotted(&self) -> bool {
        self.level >= 1.0
    }

    /// Fully aware of them, at once: shot at by them, say, or told where they are.
    pub fn alert(&mut self) {
        self.level = 1.0;
        self.unseen = 0.0;
    }

    /// Unaware of anyone: put down afresh.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// A noise heard `nearness` near (see
    /// [`Heard::nearness`](crate::perception::hearing::Heard::nearness)) - 1 right beside it - it
    /// listening `eagerness` times as keenly as when calm: the more aware of someone for it. What
    /// that came to, if anything.
    pub fn hear(&mut self, tuning: &Tuning, nearness: f32, eagerness: f32) -> Option<Change> {
        let was = self.stage(tuning);
        self.unseen = 0.0;
        self.level = (self.level + tuning.hearing * nearness.clamp(0.0, 1.0) * eagerness.max(0.0)).min(1.0);
        change(was, self.stage(tuning))
    }

    /// Moves it on `dt` with them as `visible` as they are to it now - 0 out of sight (see
    /// [`Sight::visibility`](crate::perception::sight::Sight::visibility)) - it looking for them
    /// `eagerness` times as keenly as when calm: filling while it sees them, draining a while
    /// after it last did. What that came to, if anything.
    pub fn step(&mut self, tuning: &Tuning, visible: f32, eagerness: f32, dt: f32) -> Option<Change> {
        let was = self.stage(tuning);
        if visible > 0.0 {
            self.unseen = 0.0;
            self.level = (self.level + visible * eagerness.max(0.0) * dt / tuning.fill.max(1.0e-3)).min(1.0);
        } else {
            self.unseen += dt;
            if self.unseen > tuning.forget_after {
                self.level = (self.level - tuning.drain * dt).max(0.0);
            }
        }
        change(was, self.stage(tuning))
    }
}

/// What going from stage `was` to `now` came to.
fn change(was: Stage, now: Stage) -> Option<Change> {
    match (was, now) {
        (before, Stage::Danger) if before != Stage::Danger => Some(Change::Spotted),
        (Stage::Hidden, Stage::Caution) => Some(Change::Noticed),
        (before, Stage::Hidden) if before != Stage::Hidden => Some(Change::Lost),
        _ => None,
    }
}

/// The stage of one `level` aware, from 0 to 1, as `tuning` has it.
pub fn stage_at(tuning: &Tuning, level: f32) -> Stage {
    if level >= 1.0 {
        Stage::Danger
    } else if level >= tuning.caution {
        Stage::Caution
    } else {
        Stage::Hidden
    }
}

/// The most aware of `watchers`, and its stage, as `tuning` has it: what a sneaking player is
/// shown. Hidden, at nothing, with none.
pub fn most_aware<'a>(tuning: &Tuning, watchers: impl IntoIterator<Item = &'a Detection>) -> (Stage, f32) {
    let most = watchers.into_iter().map(|d| d.level).fold(0.0, f32::max);
    (stage_at(tuning, most), most)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::perception::sight::{Sight, Stance};
    use nalgebra::Vector3;

    /// How long, in seconds, to spot someone as visible as `visible`, stepping `dt` at a time.
    fn time_to_spot(visible: f32, eagerness: f32) -> f32 {
        let (mut d, dt) = (Detection::default(), 0.01);
        let mut t = 0.0;
        while !d.spotted() && t < 60.0 {
            d.step(&Tuning::DEFAULT, visible, eagerness, dt);
            t += dt;
        }
        t
    }

    #[test]
    fn it_spots_the_more_visible_sooner_and_sooner_still_looking_for_them() {
        let full = time_to_spot(1.0, 1.0);
        assert!((full - FILL).abs() < 0.05, "{full}");
        assert!(time_to_spot(0.25, 1.0) > 3.0 * full);
        assert!(time_to_spot(1.0, 4.0) < full / 3.0);
    }

    #[test]
    fn it_notices_then_spots_and_forgets_out_of_sight() {
        let t = Tuning::DEFAULT;
        let mut d = Detection::default();
        assert_eq!(d.stage(&t), Stage::Hidden);
        let mut changes = Vec::new();
        for _ in 0..200 {
            changes.extend(d.step(&t, 1.0, 1.0, 0.01));
        }
        assert_eq!(changes, vec![Change::Noticed, Change::Spotted]);
        assert_eq!(d.stage(&t), Stage::Danger);
        // Out of sight: a moment as it was, and then less and less, till it is unaware again.
        d.step(&t, 0.0, 1.0, FORGET_AFTER * 0.5);
        assert!(d.spotted(), "not forgotten at once");
        let mut lost = false;
        for _ in 0..1000 {
            lost |= d.step(&t, 0.0, 1.0, 0.01) == Some(Change::Lost);
        }
        assert!(lost && d.level == 0.0);
    }

    #[test]
    fn a_glimpse_short_of_spotting_them_fades_again() {
        let t = Tuning::DEFAULT;
        let mut d = Detection::default();
        for _ in 0..50 {
            d.step(&t, 1.0, 1.0, 0.01);
        }
        assert_eq!(d.stage(&t), Stage::Caution);
        for _ in 0..600 {
            d.step(&t, 0.0, 1.0, 0.01);
        }
        assert_eq!(d.stage(&t), Stage::Hidden);
    }

    #[test]
    fn noises_close_by_make_it_aware_and_far_off_barely() {
        let t = Tuning::DEFAULT;
        // A footfall right beside it: noticed at once; another, and it is sure.
        let mut d = Detection::default();
        assert_eq!(d.hear(&t, 1.0, 1.0), Some(Change::Noticed));
        assert_eq!(d.hear(&t, 1.0, 1.0), Some(Change::Spotted));
        // At the edge of where they carry: many before it so much as notices.
        let mut d = Detection::default();
        let heard = (1..20).find(|_| d.hear(&t, 0.05, 1.0) == Some(Change::Noticed));
        assert!(heard.is_some_and(|n| n >= 8), "{heard:?}");
        // Heard, it does not forget at once.
        let mut d = Detection::default();
        d.hear(&t, 0.8, 1.0);
        d.step(&t, 0.0, 1.0, FORGET_AFTER * 0.5);
        assert!(d.level > 0.4);
    }

    #[test]
    fn the_most_aware_watcher_is_what_is_shown() {
        let t = Tuning::DEFAULT;
        let (a, b) = (Detection { level: 0.1, unseen: 0.0 }, Detection { level: 0.6, unseen: 0.0 });
        assert_eq!(most_aware(&t, [&a, &b]), (Stage::Caution, 0.6));
        assert_eq!(most_aware(&t, []), (Stage::Hidden, 0.0));
    }

    #[test]
    fn crouched_far_off_or_to_the_side_one_is_less_visible() {
        let sight = Sight::DEFAULT;
        let v = |x: f32, z: f32, stance: Stance, dark: bool| sight.visibility(None, Vector3::zeros(), 0.0, Vector3::new(x, 0.0, z), stance, dark);
        let near = v(0.0, 2.0, Stance::Standing, false);
        assert!(near > 0.9, "{near}");
        assert!(v(0.0, 25.0, Stance::Standing, false) < 0.5);
        assert!(v(0.0, 10.0, Stance::Crouching, false) < v(0.0, 10.0, Stance::Standing, false));
        assert!(v(0.0, 10.0, Stance::Standing, true) < v(0.0, 10.0, Stance::Standing, false));
        // Out of the corner of its eye: less, near as it is.
        assert!(v(3.0, 0.5, Stance::Standing, false) < 0.6 && v(3.0, 0.5, Stance::Standing, false) > 0.0);
        assert_eq!(v(0.0, -2.0, Stance::Standing, false), 0.0, "behind it");
    }
}
