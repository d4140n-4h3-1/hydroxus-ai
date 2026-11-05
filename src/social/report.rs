//! Telling of trouble. One that saw the player do something - shoot someone, say - goes to tell
//! a sentry: it makes for the nearest standing within [`REPORT_REACH`], and tells it once within
//! [`REPORT_NEAR`] of it. With none about, it hides, and looks again every [`REPORT_LOOK_EVERY`]
//! seconds, till [`HIDE_FOR`] is up and it gives up. One that sounds the alarm is heard by those
//! within [`ALARM_RANGE`] of it.

use crate::flat;
use nalgebra::Vector3;

/// How far off, in meters, a witness looks for a sentry to tell, and how near it has to get to
/// tell it; how long it hides, with none about, looking for one now and then, before it gives up,
/// and how often it looks, in seconds.
pub const REPORT_REACH: f32 = 150.0;
pub const REPORT_NEAR: f32 = 2.5;
pub const HIDE_FOR: f32 = 30.0;
pub const REPORT_LOOK_EVERY: f32 = 1.0;
/// How far off, in meters, those that answer an alarm hear it.
pub const ALARM_RANGE: f32 = 40.0;

tuning!(
    /// How a witness tells a sentry, and how far an alarm carries (see the module's constants,
    /// its defaults).
    {
        /// How far off a witness looks for a sentry, and how near it has to get to tell it, in
        /// meters; how long it hides, with none about, and how often it looks, in seconds.
        report_reach: f32 = REPORT_REACH,
        report_near: f32 = REPORT_NEAR,
        hide_for: f32 = HIDE_FOR,
        report_look_every: f32 = REPORT_LOOK_EVERY,
        /// How far off an alarm is heard, in meters.
        alarm_range: f32 = ALARM_RANGE,
    }
);

/// One that has seen what the player did, on its way to tell a sentry of it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Witness {
    /// Where it was done, for the sentry to search from; and whether it knows who did it.
    pub at: Vector3<f32>,
    pub knows: bool,
    /// The sentry it is making for, if it has found one; how long until it looks for one again,
    /// and how long it has left before it gives up, in seconds.
    to: Option<usize>,
    look_in: f32,
    left: f32,
    /// How it goes about telling.
    tuning: Tuning,
}

/// What a witness does, a step on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Telling {
    /// Makes for `sentry` - `afresh`, if it was not making for it already.
    MakingFor { sentry: usize, afresh: bool },
    /// Has told `sentry`, and is done.
    Told(usize),
    /// Hides from where it was done, with no sentry about.
    Hiding,
    /// Has given up looking for one, and is done.
    GaveUp,
}

impl Witness {
    /// One that saw it done `at`, and `knows` who did it or not.
    pub fn new(at: Vector3<f32>, knows: bool) -> Self {
        Self::with(at, knows, Tuning::DEFAULT)
    }

    /// One that saw it done `at`, and `knows` who did it or not, going about telling as `tuning`
    /// says.
    pub fn with(at: Vector3<f32>, knows: bool, tuning: Tuning) -> Self {
        Self { at, knows, to: None, look_in: 0.0, left: tuning.hide_for, tuning }
    }

    /// The sentry it is making for, if any.
    pub fn sentry(&self) -> Option<usize> {
        self.to
    }

    /// Moves it on `dt`, at `feet`, with `sentries` standing where they are, by index: still
    /// making for its own while it stands, or else, now and then, the nearest within reach.
    pub fn step(&mut self, dt: f32, feet: Vector3<f32>, sentries: &[(usize, Vector3<f32>)]) -> Telling {
        let mine = self.to.and_then(|to| sentries.iter().find(|&&(s, _)| s == to)).copied();
        self.look_in -= dt;
        let to = match mine {
            Some(to) => Some(to),
            None if self.look_in <= 0.0 => {
                self.look_in = self.tuning.report_look_every;
                sentries
                    .iter()
                    .copied()
                    .filter(|&(_, at)| flat(at - feet).norm() < self.tuning.report_reach)
                    .min_by(|a, b| flat(a.1 - feet).norm().total_cmp(&flat(b.1 - feet).norm()))
            }
            None => None,
        };
        match to {
            Some((s, at)) => {
                let afresh = self.to != Some(s);
                self.to = Some(s);
                if flat(at - feet).norm() < self.tuning.report_near {
                    Telling::Told(s)
                } else {
                    Telling::MakingFor { sentry: s, afresh }
                }
            }
            None => {
                self.to = None;
                self.left -= dt;
                if self.left <= 0.0 {
                    Telling::GaveUp
                } else {
                    Telling::Hiding
                }
            }
        }
    }
}

/// Whether one at `feet` hears an alarm sounded at `from`.
pub fn hears_alarm(tuning: &Tuning, from: Vector3<f32>, feet: Vector3<f32>) -> bool {
    (feet - from).norm() < tuning.alarm_range
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_witness_makes_for_the_nearest_sentry_and_tells_it() {
        let mut witness = Witness::new(Vector3::zeros(), true);
        let sentries = [(4, Vector3::new(50.0, 0.0, 0.0)), (7, Vector3::new(20.0, 0.0, 0.0))];
        assert_eq!(witness.step(0.1, Vector3::zeros(), &sentries), Telling::MakingFor { sentry: 7, afresh: true });
        assert_eq!(witness.step(0.1, Vector3::new(5.0, 0.0, 0.0), &sentries), Telling::MakingFor { sentry: 7, afresh: false });
        assert_eq!(witness.step(0.1, Vector3::new(19.0, 0.0, 0.0), &sentries), Telling::Told(7));
    }

    #[test]
    fn with_none_about_it_hides_looks_again_now_and_then_and_gives_up() {
        let mut witness = Witness::new(Vector3::zeros(), false);
        let far = [(1, Vector3::new(REPORT_REACH + 10.0, 0.0, 0.0))];
        assert_eq!(witness.step(0.5, Vector3::zeros(), &far), Telling::Hiding);
        // Its sentry gone, it looks for another, but not till it is time to look again.
        let near = [(2, Vector3::new(10.0, 0.0, 0.0))];
        assert_eq!(witness.step(0.2, Vector3::zeros(), &near), Telling::Hiding);
        assert_eq!(witness.step(REPORT_LOOK_EVERY, Vector3::zeros(), &near), Telling::MakingFor { sentry: 2, afresh: true });
        let mut alone = Witness::new(Vector3::zeros(), false);
        let gave_up = (0..(HIDE_FOR as usize + 2)).any(|_| alone.step(1.0, Vector3::zeros(), &[]) == Telling::GaveUp);
        assert!(gave_up);
        assert!(hears_alarm(&Tuning::DEFAULT, Vector3::zeros(), Vector3::new(ALARM_RANGE - 1.0, 0.0, 0.0)));
        assert!(!hears_alarm(&Tuning::DEFAULT, Vector3::zeros(), Vector3::new(ALARM_RANGE + 1.0, 0.0, 0.0)));
    }
}
