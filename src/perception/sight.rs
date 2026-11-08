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
//! In fog ([`Fog`]) it sees only as far as enough of someone shows through it - as the fog is
//! drawn, so it sees about as far as the player does - and the less of them shows through, the
//! less visible they are: one standing in the fog at the edge of what it sees takes a long while
//! to make out. Fog takes nothing from what it hears ([`crate::perception::hearing`]). What there
//! is to see by - dark or light, fog or none - is a [`Seeing`]; a plain `bool`, in the dark or
//! not, is one with no fog.
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

/// Fog, as the engine draws it: of each meter beyond `start`, `density` of what is left shows
/// through no more, a density that falls off by a factor `e` every `1 / height_falloff` meters
/// above `base_height` - fog lying low.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Fog {
    pub density: f32,
    pub start: f32,
    pub base_height: f32,
    pub height_falloff: f32,
}

/// How high above someone's feet they are looked at, and look from, through fog: about head
/// height.
const LOOKED_AT: f32 = 1.5;

impl Fog {
    /// How much of someone standing at `them` shows through the fog to an NPC standing at `feet`:
    /// 1 where there is none in between, down towards 0 the more there is.
    pub fn clear(&self, feet: Vector3<f32>, them: Vector3<f32>) -> f32 {
        let distance = ((them - feet).norm() - self.start).max(0.0);
        let above = (0.5 * (feet.y + them.y) + LOOKED_AT - self.base_height).max(0.0);
        let density = self.density * (-self.height_falloff * above).exp();
        (-density * distance).exp()
    }
}

/// What there is to see by: whether it is dark, and the fog, if there is any. From a `bool`, in
/// the dark or not, with no fog.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Seeing {
    pub dark: bool,
    pub fog: Option<Fog>,
}

impl From<bool> for Seeing {
    fn from(dark: bool) -> Self {
        Self { dark, fog: None }
    }
}

impl Seeing {
    /// How much of someone at `them` shows through the fog to an NPC at `feet`: 1 with none.
    pub fn clear(&self, feet: Vector3<f32>, them: Vector3<f32>) -> f32 {
        self.fog.map_or(1.0, |fog| fog.clear(feet, them))
    }
}

/// How an NPC sees: its tuning, as the other modules' [`Tuning`](crate::Tuning)s are.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(default))]
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
    /// How visible someone is at the very edge of how far it sees, of how visible right in front
    /// of it (see [`Sight::visibility`]); and how much less so out of the corner of its eye.
    pub edge: f32,
    pub glimpse: f32,
    /// How little of someone has to show through fog for it to make them out at all, from 0 to 1
    /// (see [`Fog::clear`]).
    pub faint: f32,
}

impl Sight {
    /// How an NPC sees unless the game says otherwise.
    pub const DEFAULT: Self = Self {
        range: 30.0,
        cone: 55.0f32.to_radians(),
        side: 100.0f32.to_radians(),
        peripheral: 0.35,
        crouched: 0.5,
        crawling: 0.3,
        dark: 0.35,
        edge: 0.25,
        glimpse: 0.5,
        faint: 0.25,
    };
}

impl Default for Sight {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl Sight {
    /// How far it sees someone in `stance`, in `alert` - or calm, with none - in the dark or not,
    /// leaving fog aside, which [`Sight::could_see`] takes into account.
    pub fn reach(&self, alert: Option<Alert>, stance: Stance, in_the_dark: bool) -> f32 {
        let lit = if in_the_dark { self.dark } else { 1.0 };
        let low = match (alert, stance) {
            (Some(Alert::Alert), _) | (_, Stance::Standing) => 1.0,
            (_, Stance::Crouching) => self.crouched,
            (_, Stance::Crawling) => self.crawling,
        };
        self.range * lit * low
    }

    /// How visible someone at `them` holding themselves in `stance` is to an NPC at `feet`, facing
    /// `heading`, in `alert` - or calm, with none - by what there is to see by (`seeing`: in the
    /// dark or not, and fog), if nothing were in the way: from 0, where it could not see them at
    /// all (see [`Sight::could_see`]), to 1, right in front of it. Less the further off they are,
    /// of as far as it sees them - [`Sight::edge`] at the very edge - and [`Sight::glimpse`] as
    /// much out of the corner of its eye. Crouched, crawling or in the dark, it sees them less
    /// far, and so they are less visible anywhere; in fog, only as visible as how much of them
    /// shows through it. How quickly it becomes aware of them (see
    /// [`crate::perception::detection`]).
    pub fn visibility(
        &self,
        alert: Option<Alert>,
        feet: Vector3<f32>,
        heading: f32,
        them: Vector3<f32>,
        stance: Stance,
        seeing: impl Into<Seeing>,
    ) -> f32 {
        let seeing = seeing.into();
        if !self.could_see(alert, feet, heading, them, stance, seeing) {
            return 0.0;
        }
        let to = them - feet;
        let across = flat(to).norm();
        let clearly = flat(to).dot(&forward(heading)) >= across * self.cone.cos();
        let reach = self.reach(alert, stance, seeing.dark) * if clearly { 1.0 } else { self.peripheral };
        let near = (1.0 - to.norm() / reach.max(1.0e-3)).clamp(0.0, 1.0);
        let shown = (self.edge + (1.0 - self.edge) * near) * if clearly { 1.0 } else { self.glimpse };
        shown * seeing.clear(feet, them)
    }

    /// Whether an NPC at `feet`, facing `heading`, in `alert` - or calm, with none - could see
    /// someone at `them` holding themselves in `stance`, by what there is to see by (`seeing`: in
    /// the dark or not, and fog), if nothing were in the way: in front of it, within its cone,
    /// near enough, and with enough of them showing through any fog ([`Sight::faint`]).
    pub fn could_see(
        &self,
        alert: Option<Alert>,
        feet: Vector3<f32>,
        heading: f32,
        them: Vector3<f32>,
        stance: Stance,
        seeing: impl Into<Seeing>,
    ) -> bool {
        let seeing = seeing.into();
        let to = them - feet;
        let across = flat(to).norm();
        // Straight above or below its feet: not in front of it.
        if across <= 1.0e-3 {
            return false;
        }
        let ahead = flat(to).dot(&forward(heading));
        let reach = self.reach(alert, stance, seeing.dark);
        let reach = if ahead >= across * self.cone.cos() {
            reach
        } else if ahead >= across * self.side.cos() {
            reach * self.peripheral
        } else {
            return false;
        };
        to.norm() <= reach && seeing.clear(feet, them) >= self.faint
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

    /// The game's foggy maps' fog.
    const FOG: Fog = Fog { density: 0.035, start: 3.0, base_height: 0.0, height_falloff: 0.03 };

    fn in_fog() -> Seeing {
        Seeing { dark: false, fog: Some(FOG) }
    }

    #[test]
    fn in_fog_it_sees_only_as_far_as_enough_shows_through() {
        let far = Sight { range: 60.0, ..Sight::default() };
        let at = |z: f32| Vector3::new(0.0, 0.0, z);
        let hunting = Some(Alert::Alert);
        assert!(far.could_see(hunting, Vector3::zeros(), 0.0, at(55.0), Stance::Standing, false), "clear");
        assert!(!far.could_see(hunting, Vector3::zeros(), 0.0, at(55.0), Stance::Standing, in_fog()), "lost in the fog");
        assert!(far.could_see(hunting, Vector3::zeros(), 0.0, at(30.0), Stance::Standing, in_fog()), "nearer");
        // Where it can see them at all, they are the less visible the more fog is in between.
        let clear = far.visibility(hunting, Vector3::zeros(), 0.0, at(20.0), Stance::Standing, false);
        let foggy = far.visibility(hunting, Vector3::zeros(), 0.0, at(20.0), Stance::Standing, in_fog());
        assert!(foggy < clear * 0.65 && foggy > clear * 0.4, "{foggy} of {clear}");
        let near = far.visibility(hunting, Vector3::zeros(), 0.0, at(2.0), Stance::Standing, in_fog());
        let near_clear = far.visibility(hunting, Vector3::zeros(), 0.0, at(2.0), Stance::Standing, false);
        assert_eq!(near, near_clear, "right in front of it, before the fog starts");
    }

    #[test]
    fn fog_lies_low_and_thins_above() {
        let low = FOG.clear(Vector3::zeros(), Vector3::new(0.0, 0.0, 30.0));
        let high = FOG.clear(Vector3::new(0.0, 40.0, 0.0), Vector3::new(0.0, 40.0, 30.0));
        assert!(high > low, "{high} over {low}");
        assert_eq!(Seeing::from(true), Seeing { dark: true, fog: None });
        assert_eq!(Seeing::default().clear(Vector3::zeros(), Vector3::new(0.0, 0.0, 500.0)), 1.0);
    }

    #[test]
    fn in_the_dark_it_sees_less_far() {
        assert!(sees(None, 0.0, 20.0, Stance::Standing, false));
        assert!(!sees(None, 0.0, 20.0, Stance::Standing, true));
        assert!(sees(None, 0.0, 8.0, Stance::Standing, true));
        assert!(!sees(Some(Alert::Alert), 0.0, 20.0, Stance::Standing, true), "even after them");
    }
}
