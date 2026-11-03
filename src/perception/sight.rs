//! What an NPC can see.
//!
//! It sees clearly in a cone in front of it, whatever it is doing: calm, searching, or after the
//! player. Out of the corner of its eye, further round to either side, it sees too, but only
//! near: [`Sight::peripheral`] of as far. It has no sense of what is behind it, even right behind
//! it, so it can be crept up on, or slipped round while it is looking the other way - and on
//! Alert, a player who gets round behind it is lost to it, and it has to search.
//!
//! Hunting ([`Alert::Alert`]), it sees as far as it sees at all, however low the player is.
//! Otherwise it sees less far the lower they are: a player crouched from half as far, one
//! crawling from less than a third. In the dark it sees a good deal less far either way.
//!
//! This only says whether they are where it could see them. Whether anything is in the way is
//! the game's to find out, with a ray from its eyes.

use crate::{alert::Alert, flat, forward};
use nalgebra::Vector3;

/// How someone holds themselves, which is how easily they are seen.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Stance {
    #[default]
    Standing,
    Crouching,
    Crawling,
}

/// How an NPC sees.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sight {
    /// How far off it sees, in meters, at most.
    pub range: f32,
    /// How far to either side of straight ahead it sees clearly, in radians.
    pub cone: f32,
    /// How far to either side of straight ahead it sees at all, out of the corner of its eye, in
    /// radians; and how much of as far as it sees clearly, it sees that way.
    pub side: f32,
    pub peripheral: f32,
    /// How much of [`Sight::range`] it sees someone crouched and crawling from, when it is not
    /// after them.
    pub crouched: f32,
    pub crawling: f32,
    /// How much of how far it would see, it sees in the dark.
    pub dark: f32,
}

impl Default for Sight {
    fn default() -> Self {
        Self {
            range: 30.0,
            cone: 55.0f32.to_radians(),
            side: 100.0f32.to_radians(),
            peripheral: 0.35,
            crouched: 0.5,
            crawling: 0.3,
            dark: 0.35,
        }
    }
}

impl Sight {
    /// How far it sees someone in `stance`, in `alert` - or calm, with none - in the dark or not.
    pub fn reach(&self, alert: Option<Alert>, stance: Stance, in_the_dark: bool) -> f32 {
        let lit = if in_the_dark { self.dark } else { 1.0 };
        let low = match (alert, stance) {
            (Some(Alert::Alert), _) | (_, Stance::Standing) => 1.0,
            (_, Stance::Crouching) => self.crouched,
            (_, Stance::Crawling) => self.crawling,
        };
        self.range * lit * low
    }

    /// Whether an NPC at `feet`, facing `heading`, in `alert` - or calm, with none - could see
    /// someone at `them` holding themselves in `stance`, if nothing were in the way: in front of
    /// it, within its cone, and near enough.
    pub fn could_see(
        &self,
        alert: Option<Alert>,
        feet: Vector3<f32>,
        heading: f32,
        them: Vector3<f32>,
        stance: Stance,
        in_the_dark: bool,
    ) -> bool {
        let to = them - feet;
        let across = flat(to).norm();
        // Straight above or below its feet: not in front of it.
        if across <= 1.0e-3 {
            return false;
        }
        let ahead = flat(to).dot(&forward(heading));
        let reach = self.reach(alert, stance, in_the_dark);
        let reach = if ahead >= across * self.cone.cos() {
            reach
        } else if ahead >= across * self.side.cos() {
            reach * self.peripheral
        } else {
            return false;
        };
        to.norm() <= reach
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Facing +z, from the origin.
    fn sees(alert: Option<Alert>, x: f32, z: f32, stance: Stance, dark: bool) -> bool {
        Sight::default().could_see(alert, Vector3::zeros(), 0.0, Vector3::new(x, 0.0, z), stance, dark)
    }

    #[test]
    fn it_sees_only_in_front_whatever_it_is_doing() {
        for alert in [None, Some(Alert::Caution), Some(Alert::Evasion), Some(Alert::Alert)] {
            assert!(sees(alert, 0.0, 20.0, Stance::Standing, false), "ahead, {alert:?}");
            assert!(!sees(alert, 0.0, -10.0, Stance::Standing, false), "behind, {alert:?}");
            assert!(!sees(alert, 0.0, -0.5, Stance::Standing, false), "right behind it, {alert:?}");
            assert!(!sees(alert, 1.0, -1.0, Stance::Standing, false), "behind its shoulder, {alert:?}");
        }
    }

    #[test]
    fn out_of_the_corner_of_its_eye_it_sees_only_near() {
        for alert in [None, Some(Alert::Caution), Some(Alert::Evasion), Some(Alert::Alert)] {
            assert!(sees(alert, 1.0, 0.0, Stance::Standing, false), "right beside it, {alert:?}");
            assert!(sees(alert, 6.0, 1.0, Stance::Standing, false), "off to the side, {alert:?}");
            assert!(!sees(alert, 15.0, 1.0, Stance::Standing, false), "far off to the side, {alert:?}");
            // Just past where it sees clearly, a little further round, it sees only near.
            assert!(sees(alert, 15.0, 11.0, Stance::Standing, false), "in its cone, {alert:?}");
            assert!(!sees(alert, 15.0, 9.0, Stance::Standing, false), "just out of it, {alert:?}");
        }
        // Lower, and in the dark, less far again.
        assert!(sees(None, 4.0, 1.0, Stance::Crouching, false));
        assert!(!sees(None, 6.0, 1.0, Stance::Crouching, false));
        assert!(!sees(None, 4.0, 1.0, Stance::Standing, true));
        assert!(sees(None, 3.0, 1.0, Stance::Standing, true));
    }

    #[test]
    fn it_sees_less_far_the_lower_they_are_unless_it_is_after_them() {
        let calm = Some(Alert::Caution);
        assert!(!sees(calm, 0.0, 20.0, Stance::Crouching, false), "crouched, far off");
        assert!(sees(calm, 0.0, 12.0, Stance::Crouching, false), "crouched, nearer");
        assert!(!sees(Some(Alert::Evasion), 0.0, 12.0, Stance::Crawling, false), "crawling");
        assert!(sees(Some(Alert::Alert), 0.0, 20.0, Stance::Crawling, false), "hunting");
        assert!(!sees(Some(Alert::Alert), 0.0, 31.0, Stance::Standing, false), "too far");
    }

    #[test]
    fn in_the_dark_it_sees_less_far() {
        assert!(sees(None, 0.0, 20.0, Stance::Standing, false));
        assert!(!sees(None, 0.0, 20.0, Stance::Standing, true));
        assert!(sees(None, 0.0, 8.0, Stance::Standing, true));
        assert!(!sees(Some(Alert::Alert), 0.0, 20.0, Stance::Standing, true), "even after them");
    }
}
