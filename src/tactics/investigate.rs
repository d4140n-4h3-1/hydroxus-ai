//! Investigating: an NPC that has noticed something - not sure enough to go after anyone, but
//! sure something was there (see [`crate::perception::detection`]) - stops and looks that way a
//! moment, goes over to where it was, and looks about there; and then, finding nothing, goes back
//! to what it was doing. Seeing it again on its way, it makes for where it saw it last.
//!
//! The game moves the NPC: this says, each step, what it is doing ([`Doing`]) - facing that way,
//! going there, looking about, or done.

use crate::{flat, tactics::look::LookAbout};
use nalgebra::Vector3;

/// How long it stands looking that way before it goes over, in seconds; how near there counts as
/// there, in meters; how long it looks about there, in seconds, and how far round either way, in
/// radians; and how long it goes on with it at most, in seconds, before it gives it up.
pub const PAUSE: f32 = 0.8;
pub const REACH: f32 = 1.5;
pub const LOOK_FOR: f32 = 4.0;
pub const SWEEP: f32 = 70.0 * std::f32::consts::PI / 180.0;
pub const GIVE_UP: f32 = 25.0;

tuning!(
    /// How an NPC investigates what it has noticed (see the module's constants, its defaults).
    {
        pause: f32 = PAUSE,
        reach: f32 = REACH,
        look_for: f32 = LOOK_FOR,
        sweep: f32 = SWEEP,
        give_up: f32 = GIVE_UP,
    }
);

/// What an investigating NPC does this step.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Doing {
    /// Stands, facing `heading`: the way it noticed it.
    Face { heading: f32 },
    /// Goes to `at` - working out its way there `afresh`, the first time and whenever where it
    /// is going has moved.
    GoTo { at: Vector3<f32>, afresh: bool },
    /// Stands there, looking `heading`, about where it was.
    LookAround { heading: f32 },
    /// Done with it - it found nothing, or gave it up - and goes back to what it was doing.
    Done,
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
enum Step {
    Pause { left: f32 },
    Going { afresh: bool },
    Looking(LookAbout),
}

/// An NPC's investigation of something it noticed.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Investigation {
    /// Where it noticed it, last.
    pub at: Vector3<f32>,
    step: Step,
    /// How long it has been at it, in seconds.
    time: f32,
}

impl Investigation {
    /// Something noticed `at`, as `tuning` has an NPC go about it.
    pub fn new(tuning: &Tuning, at: Vector3<f32>) -> Self {
        Self { at, step: Step::Pause { left: tuning.pause }, time: 0.0 }
    }

    /// Noticed again `at`: it makes for there instead - straight on, if it was looking about
    /// somewhere else.
    pub fn saw_again(&mut self, tuning: &Tuning, at: Vector3<f32>) {
        let moved = flat(at - self.at).norm() > tuning.reach;
        self.at = at;
        match self.step {
            Step::Going { .. } | Step::Looking { .. } if moved => self.step = Step::Going { afresh: true },
            _ => (),
        }
    }

    /// Whether it is on its way there.
    pub fn going(&self) -> bool {
        matches!(self.step, Step::Going { .. })
    }

    /// Moves it on `dt` for an NPC at `feet`, facing `heading`: what it does.
    pub fn step(&mut self, tuning: &Tuning, feet: Vector3<f32>, heading: f32, dt: f32) -> Doing {
        self.time += dt;
        if self.time > tuning.give_up {
            return Doing::Done;
        }
        let to = flat(self.at - feet);
        let towards = if to.norm() > 1.0e-3 { to.x.atan2(to.z) } else { heading };
        match self.step {
            Step::Pause { left } if left > dt => {
                self.step = Step::Pause { left: left - dt };
                Doing::Face { heading: towards }
            }
            Step::Pause { .. } => {
                self.step = Step::Going { afresh: false };
                Doing::GoTo { at: self.at, afresh: true }
            }
            Step::Going { .. } if to.norm() <= tuning.reach => {
                self.step = Step::Looking(LookAbout::new(heading, tuning.look_for, tuning.sweep));
                Doing::LookAround { heading }
            }
            Step::Going { afresh } => {
                self.step = Step::Going { afresh: false };
                Doing::GoTo { at: self.at, afresh }
            }
            Step::Looking(mut look) => match look.step(dt) {
                Some(heading) => {
                    self.step = Step::Looking(look);
                    Doing::LookAround { heading }
                }
                None => Doing::Done,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Walks an NPC from `feet` through an investigation of `at` at 1.4 m/s, stepping `dt`:
    /// everything it did, in order, and where it ended up.
    fn walk(investigation: &mut Investigation, mut feet: Vector3<f32>, dt: f32) -> (Vec<Doing>, Vector3<f32>) {
        let t = Tuning::DEFAULT;
        let mut heading = 0.0;
        let mut did = Vec::new();
        for _ in 0..((GIVE_UP + 5.0) / dt) as usize {
            let doing = investigation.step(&t, feet, heading, dt);
            did.push(doing);
            match doing {
                Doing::Face { heading: h } | Doing::LookAround { heading: h } => heading = h,
                Doing::GoTo { at, .. } => {
                    let to = flat(at - feet);
                    heading = to.x.atan2(to.z);
                    feet += to.normalize() * (1.4 * dt).min(to.norm());
                }
                Doing::Done => break,
            }
        }
        (did, feet)
    }

    #[test]
    fn it_looks_that_way_goes_over_looks_about_and_is_done() {
        let at = Vector3::new(10.0, 0.0, 0.0);
        let mut inv = Investigation::new(&Tuning::DEFAULT, at);
        let (did, feet) = walk(&mut inv, Vector3::zeros(), 0.05);
        // First it faces that way, standing.
        assert!(matches!(did[0], Doing::Face { heading } if (heading - std::f32::consts::FRAC_PI_2).abs() < 1e-4));
        // Then it goes there - working its way out once - then looks about, and is done.
        let afresh = did.iter().filter(|d| matches!(d, Doing::GoTo { afresh: true, .. })).count();
        assert_eq!(afresh, 1);
        assert!(did.iter().any(|d| matches!(d, Doing::LookAround { .. })));
        assert_eq!(did.last(), Some(&Doing::Done));
        assert!(flat(feet - at).norm() <= REACH + 0.1, "it got there: {feet:?}");
        let looked = did.iter().filter(|d| matches!(d, Doing::LookAround { .. })).count() as f32 * 0.05;
        assert!((looked - LOOK_FOR).abs() < 0.2, "{looked}");
    }

    #[test]
    fn noticing_it_again_elsewhere_it_makes_for_there_instead() {
        let t = Tuning::DEFAULT;
        let mut inv = Investigation::new(&t, Vector3::new(10.0, 0.0, 0.0));
        for _ in 0..30 {
            inv.step(&t, Vector3::zeros(), 0.0, 0.05);
        }
        assert!(inv.going());
        inv.saw_again(&t, Vector3::new(0.0, 0.0, 12.0));
        assert_eq!(inv.step(&t, Vector3::zeros(), 0.0, 0.05), Doing::GoTo { at: Vector3::new(0.0, 0.0, 12.0), afresh: true });
        // Barely moved: no new way worked out.
        inv.saw_again(&t, Vector3::new(0.3, 0.0, 12.0));
        assert!(matches!(inv.step(&t, Vector3::zeros(), 0.0, 0.05), Doing::GoTo { afresh: false, .. }));
    }

    #[test]
    fn it_gives_up_on_somewhere_it_cannot_get_to() {
        let t = Tuning::DEFAULT;
        let mut inv = Investigation::new(&t, Vector3::new(100.0, 0.0, 0.0));
        let done = (0..((GIVE_UP + 1.0) / 0.1) as usize).any(|_| inv.step(&t, Vector3::zeros(), 0.0, 0.1) == Doing::Done);
        assert!(done);
    }
}
