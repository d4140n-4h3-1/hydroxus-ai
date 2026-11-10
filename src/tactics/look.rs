//! Looking about: standing where it is, turning its head one way and then the other for a while,
//! as an NPC does where it has gone to look for someone - a searcher at each spot it gets to (see
//! [`crate::tactics::search`]), an investigator where it noticed something (see
//! [`crate::tactics::investigate`]).
//!
//! It sweeps once round a sine: out to one side, back across to the other, and back to where it
//! started, facing `from` again as it finishes. The game turns the NPC to the heading it is given
//! each step; what it sees as it does is the game's to look for (or, searching, a
//! [`crate::tactics::search::SearchMap::look`] to clear).

use crate::tactics::search;

/// One NPC looking about where it is.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct LookAbout {
    /// Which way it faced as it started.
    pub from: f32,
    /// How long it looks about in all, and how long it has left, in seconds.
    lasts: f32,
    left: f32,
    /// How far either way it turns, in radians.
    sweep: f32,
}

impl LookAbout {
    /// Looking about from `from`, for `lasts` seconds, turning as far as `sweep` radians either way.
    pub fn new(from: f32, lasts: f32, sweep: f32) -> Self {
        let lasts = if lasts.is_finite() { lasts.max(0.0) } else { 0.0 };
        Self { from, lasts, left: lasts, sweep }
    }

    /// A searcher looking about the spot it got to, facing `from`, as the search tuning has it
    /// (`look_about`, `look_sweep`).
    pub fn searching(tuning: &search::Tuning, from: f32) -> Self {
        Self::new(from, tuning.look_about, tuning.look_sweep)
    }

    /// Moves it on `dt` seconds: the way to face now, or None once it has looked about long
    /// enough.
    pub fn step(&mut self, dt: f32) -> Option<f32> {
        let dt = if dt.is_nan() { 0.0 } else { dt.max(0.0) };
        if self.left <= dt {
            self.left = 0.0;
            return None;
        }
        self.left -= dt;
        Some(self.heading())
    }

    /// The way it faces now.
    pub fn heading(&self) -> f32 {
        let turn = std::f32::consts::TAU * (self.lasts - self.left) / self.lasts.max(1.0e-3);
        self.from + self.sweep * turn.sin()
    }

    /// Whether it has looked about long enough.
    pub fn done(&self) -> bool {
        self.left <= 0.0
    }

    /// How long it has left, in seconds.
    pub fn left(&self) -> f32 {
        self.left
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::FRAC_PI_2;

    #[test]
    fn it_turns_out_to_one_side_across_to_the_other_and_back() {
        let mut look = LookAbout::new(0.0, 4.0, 1.0);
        let headings: Vec<f32> = std::iter::from_fn(|| look.step(0.01)).collect();
        assert!((398..=400).contains(&headings.len()), "for as long as it lasts: {}", headings.len());
        let (most, least) = headings.iter().fold((f32::MIN, f32::MAX), |(a, b), &h| (a.max(h), b.min(h)));
        assert!((most - 1.0).abs() < 1.0e-3 && (least + 1.0).abs() < 1.0e-3, "{most} {least}");
        // A quarter of the way, out at one side; three quarters, the other; at the end, back.
        assert!((headings[99] - 1.0).abs() < 1.0e-3);
        assert!((headings[299] + 1.0).abs() < 1.0e-3);
        assert!(headings.last().unwrap().abs() < 0.02);
        assert!(look.done() && look.step(0.01).is_none());
    }

    #[test]
    fn it_looks_about_as_long_whatever_the_frame_rate() {
        for dt in [1.0 / 30.0, 1.0 / 144.0, 0.25] {
            let mut look = LookAbout::new(FRAC_PI_2, 3.0, 1.2);
            let mut t = 0.0;
            while look.step(dt).is_some() {
                t += dt;
            }
            assert!(t <= 3.0 && t > 3.0 - dt - 1.0e-4, "{dt}: {t}");
        }
    }

    #[test]
    fn nonsense_is_no_time() {
        let mut look = LookAbout::new(0.0, f32::NAN, 1.0);
        assert!(look.done() && look.step(0.1).is_none());
        let mut look = LookAbout::new(0.0, 2.0, 1.0);
        assert_eq!(look.step(f32::NAN), Some(0.0));
        assert_eq!(look.step(-5.0), Some(0.0));
        assert_eq!(look.left(), 2.0);
    }

    #[test]
    fn a_searcher_looks_about_as_the_search_tuning_says() {
        let t = search::Tuning::DEFAULT;
        let look = LookAbout::searching(&t, 0.5);
        assert_eq!((look.from, look.left()), (0.5, search::LOOK_ABOUT));
    }
}
