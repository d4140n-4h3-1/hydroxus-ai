//! Being threatened: an NPC with a weapon pointed at it stands for it a while, then warns whoever
//! holds it, warns them again, and then does something about it - or, with the weapon off it
//! long enough, calms down again. How long it stands for it at first is up to its kind.
//!
//! A weapon only drawn, not pointed at it, it minds too, less ([`Unease`]): seeing one out near
//! it a moment, it grows uneasy and keeps an eye on whoever holds it; with them close, it backs
//! away; the weapon put away, or out of its sight, a while, it is at ease again.

use nalgebra::Vector3;

/// How far off, in meters, a weapon pointed at an NPC bothers it, and how far to the side of
/// the middle of the view it can be, in meters, for the weapon to count as pointed at it.
pub const AIM_RANGE: f32 = 20.0;
pub const AIM_WIDTH: f32 = 0.6;
/// How long each warning holds, in seconds, on top of how long the NPC stands for the weapon
/// in the first place, before it goes on to the next; and how long the weapon has to be off it,
/// in seconds, for it to calm down again.
pub const WARNING: f32 = 2.5;
pub const CALM: f32 = 2.5;
/// A weapon drawn but not pointed: how near, in meters, an NPC has to see it to mind it, and how
/// near is too near, so that it backs away; how long it has to see it, in seconds, before it grows
/// uneasy; how long it then stands watching whoever holds it; and how long the weapon has to be
/// away, or out of its sight, before it is at ease again.
pub const UNEASE_RANGE: f32 = 12.0;
pub const TOO_CLOSE: f32 = 4.0;
pub const UNEASY_AFTER: f32 = 0.6;
pub const WATCH_FOR: f32 = 4.0;
pub const SETTLE_AFTER: f32 = 3.0;

tuning!(
    /// How an NPC takes a weapon pointed at it (see the module's constants, its defaults).
    {
        /// How far off a weapon pointed at it bothers it, and how far to the side of the middle of
        /// the view it can be, in meters.
        aim_range: f32 = AIM_RANGE,
        aim_width: f32 = AIM_WIDTH,
        /// How long each warning holds, and how long the weapon has to be off it for it to calm
        /// down, in seconds.
        warning: f32 = WARNING,
        calm: f32 = CALM,
        /// A weapon only drawn: how near it is minded, and how near is too near, in meters; how
        /// long seen before it grows uneasy, how long it then watches, and how long put away
        /// before it is at ease, in seconds.
        unease_range: f32 = UNEASE_RANGE,
        too_close: f32 = TOO_CLOSE,
        uneasy_after: f32 = UNEASY_AFTER,
        watch_for: f32 = WATCH_FOR,
        settle_after: f32 = SETTLE_AFTER,
    }
);

/// How an NPC takes a weapon drawn near it, as it goes from one to the next.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Uneasy {
    /// It has seen it a moment: it stops and watches whoever holds it, and might say something.
    Noticed,
    /// They have come too close with it: it backs away.
    Afraid,
    /// They are far enough off again: it only keeps an eye on them.
    Wary,
    /// The weapon has been put away, or out of its sight, long enough: it is at ease again.
    Settled,
}

/// What a weapon drawn near an NPC is doing to it.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Unease {
    /// How long it has seen it, how long since it last did, and how long it has left watching
    /// whoever holds it, in seconds.
    seen: f32,
    unseen: f32,
    watching: f32,
    uneasy: bool,
    afraid: bool,
}

impl Unease {
    /// Whether it minds the weapon now.
    pub fn uneasy(&self) -> bool {
        self.uneasy
    }

    /// Whether it is backing away from whoever holds it.
    pub fn afraid(&self) -> bool {
        self.afraid
    }

    /// Whether it is standing watching whoever holds it.
    pub fn watching(&self) -> bool {
        self.uneasy && self.watching > 0.0
    }

    /// At ease, with nothing seen.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Moves it on `dt`: whether it `sees` a weapon drawn, and how far off it is, in meters. What
    /// that came to, if anything.
    pub fn step(&mut self, tuning: &Tuning, sees: bool, distance: f32, dt: f32) -> Option<Uneasy> {
        let minds = sees && distance <= tuning.unease_range;
        self.watching = (self.watching - dt).max(0.0);
        if minds {
            self.seen += dt;
            self.unseen = 0.0;
        } else {
            self.unseen += dt;
            if !self.uneasy {
                self.seen = 0.0;
            }
        }
        if !self.uneasy {
            if minds && self.seen >= tuning.uneasy_after {
                self.uneasy = true;
                self.watching = tuning.watch_for;
                return Some(Uneasy::Noticed);
            }
            return None;
        }
        if self.unseen >= tuning.settle_after {
            *self = Self::default();
            return Some(Uneasy::Settled);
        }
        match (self.afraid, minds && distance <= tuning.too_close) {
            (false, true) => {
                self.afraid = true;
                Some(Uneasy::Afraid)
            }
            (true, false) if minds || self.unseen > 0.5 => {
                self.afraid = false;
                self.watching = tuning.watch_for;
                Some(Uneasy::Wary)
            }
            _ => None,
        }
    }
}

/// How an NPC takes having a weapon pointed at it, as it goes from one stage to the next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Threat {
    /// It stops, faces whoever holds it and warns them.
    Warned,
    /// It warns them for the last time.
    WarnedAgain,
    /// It has stood for it long enough: its kind does something about it.
    Provoked,
    /// The weapon has been lowered long enough for it to calm down again.
    Calmed,
}

/// Whether the middle of a view from `eye`, looking `ahead` - one meter long - is on `target`:
/// in front, not too far off, and not too far to its side.
pub fn pointed_at(tuning: &Tuning, eye: Vector3<f32>, ahead: Vector3<f32>, target: Vector3<f32>) -> bool {
    let to = target - eye;
    let along = to.dot(&ahead);
    along > 0.0 && along < tuning.aim_range && (to - ahead * along).norm() < tuning.aim_width
}

/// How long an NPC that stands `patience` seconds of a weapon has it pointed at it, in seconds,
/// after it has warned whoever holds it `warned` times, before it goes on to the next stage:
/// first its patience, then that and as long again as it takes to warn them.
pub fn stage_after(tuning: &Tuning, warned: u8, patience: f32) -> f32 {
    if warned == 0 {
        patience
    } else {
        patience + tuning.warning
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_weapon_drawn_near_it_makes_it_uneasy_too_near_afraid_and_put_away_at_ease() {
        let t = Tuning::DEFAULT;
        let mut u = Unease::default();
        let dt = 0.1;
        // Seen far off: nothing. Near, a moment: uneasy, and it watches.
        assert_eq!(u.step(&t, true, UNEASE_RANGE + 5.0, 2.0), None);
        let mut said = Vec::new();
        for _ in 0..10 {
            said.extend(u.step(&t, true, 8.0, dt));
        }
        assert_eq!(said, vec![Uneasy::Noticed]);
        assert!(u.uneasy() && u.watching() && !u.afraid());
        // Too close: it backs away; further again, wary.
        assert_eq!(u.step(&t, true, 2.0, dt), Some(Uneasy::Afraid));
        assert!(u.afraid());
        assert_eq!(u.step(&t, true, 9.0, dt), Some(Uneasy::Wary));
        // Watching only a while.
        for _ in 0..50 {
            u.step(&t, true, 9.0, dt);
        }
        assert!(u.uneasy() && !u.watching());
        // Put away: at ease, a while after.
        let settled = (0..40).find_map(|_| u.step(&t, false, 9.0, dt));
        assert_eq!(settled, Some(Uneasy::Settled));
        assert!(!u.uneasy());
        // A glimpse too short to mind is forgotten.
        let mut u = Unease::default();
        u.step(&t, true, 5.0, 0.3);
        u.step(&t, false, 5.0, 0.1);
        assert_eq!(u.step(&t, true, 5.0, 0.4), None);
    }

    #[test]
    fn the_weapon_is_pointed_at_what_is_in_the_middle_of_the_view() {
        let eye = Vector3::zeros();
        let ahead = Vector3::z();
        assert!(pointed_at(&Tuning::DEFAULT, eye, ahead, Vector3::new(0.3, 0.0, 10.0)));
        assert!(!pointed_at(&Tuning::DEFAULT, eye, ahead, Vector3::new(1.0, 0.0, 10.0)), "off to the side");
        assert!(!pointed_at(&Tuning::DEFAULT, eye, ahead, Vector3::new(0.0, 0.0, -5.0)), "behind");
        assert!(!pointed_at(&Tuning::DEFAULT, eye, ahead, Vector3::new(0.0, 0.0, AIM_RANGE + 1.0)), "too far");
    }

    #[test]
    fn each_warning_holds_long_enough_to_be_said() {
        assert_eq!(stage_after(&Tuning::DEFAULT, 0, 1.0), 1.0, "its patience, before it first warns");
        assert!(stage_after(&Tuning::DEFAULT, 1, 1.0) >= WARNING, "then time to say it");
        assert_eq!(stage_after(&Tuning::DEFAULT, 1, 1.0), stage_after(&Tuning::DEFAULT, 2, 1.0));
        assert!(stage_after(&Tuning::DEFAULT, 0, 1.0) < stage_after(&Tuning::DEFAULT, 0, 2.0), "a less patient NPC is sooner");
    }
}
