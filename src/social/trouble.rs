//! Trouble: what NPCs know has gone on about town - one of them found lying, an alarm, gunfire -
//! whoever did it, and pass on to each other, as they do rumours of the player (see
//! [`crate::social::rumour`]: an NPC's mind keeps both). One that knows of trouble somewhere keeps
//! away from there, going about its business ([`Trouble::shunned`]) - the more strongly it knows
//! of it, the further - so that, as word gets round, the crowd thins out where it happened.
//!
//! Trouble fades as rumours do, but sooner ([`FADE_HALF_LIFE`]): a town gets over a shooting
//! before it forgets who did it. The same kind of trouble close by another an NPC knows of is the
//! same trouble ([`SAME_WITHIN`]): a gunfight is one, however many shots there were.

use nalgebra::Vector3;

/// How long trouble takes to fade to half as strong, in seconds.
pub const FADE_HALF_LIFE: f32 = 300.0;
/// How far apart, in meters, two of the same kind of trouble are still the same.
pub const SAME_WITHIN: f32 = 20.0;
/// How far round trouble, in meters, one that knows of it at full strength keeps away from it -
/// less, as less.
pub const SHUN_REACH: f32 = 30.0;
/// How strong trouble has to be known of to keep away from where it happened.
pub const SHUNNED: f32 = 0.2;

tuning!(
    /// How trouble is known of and kept away from (see the module's constants, its defaults).
    {
        /// How long it takes to fade halfway, in seconds.
        fade_half_life: f32 = FADE_HALF_LIFE,
        /// How far apart two of the same kind are the same, in meters.
        same_within: f32 = SAME_WITHIN,
        /// How far round it one that knows of it keeps away, at full strength, in meters.
        shun_reach: f32 = SHUN_REACH,
        /// How strong it has to be to keep away from it.
        shunned: f32 = SHUNNED,
    }
);

/// What kind of trouble.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    /// One of them, found lying.
    Body,
    /// The alarm, sounded.
    Alarm,
    /// Shots, heard.
    Gunfire,
}

impl Kind {
    /// How strongly one that comes across it knows of it: a body seen, more surely than shots
    /// heard.
    pub fn strength(self) -> f32 {
        match self {
            Kind::Body => 1.0,
            Kind::Alarm => 0.8,
            Kind::Gunfire => 0.7,
        }
    }
}

/// Trouble, as one NPC knows of it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Trouble {
    /// Which trouble it is, the same for everyone who knows of it.
    pub id: u32,
    pub kind: Kind,
    /// Where it was.
    pub at: Vector3<f32>,
    /// How strong it is: as its kind says if come across, less told, less again the longer ago.
    pub strength: f32,
    /// Whether this one came across it itself.
    pub saw: bool,
}

impl Trouble {
    /// Whether this is `other`: the same, or the same kind close enough by.
    pub fn is(&self, tuning: &Tuning, other: &Trouble) -> bool {
        self.id == other.id
            || self.kind == other.kind && (self.at.x - other.at.x).hypot(self.at.z - other.at.z) < tuning.same_within
    }

    /// Where one that knows of it keeps away from, and how far round, in meters: none, known of
    /// too faintly to mind.
    pub fn shunned(&self, tuning: &Tuning) -> Option<(Vector3<f32>, f32)> {
        (self.strength >= tuning.shunned).then(|| (self.at, tuning.shun_reach * self.strength.min(1.0)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_kind_close_by_is_the_same_and_it_is_shunned_as_far_as_it_is_strong() {
        let t = Tuning::DEFAULT;
        let shots = Trouble { id: 1, kind: Kind::Gunfire, at: Vector3::zeros(), strength: 0.5, saw: true };
        let more = Trouble { id: 2, at: Vector3::new(10.0, 0.0, 0.0), ..shots };
        let far = Trouble { id: 3, at: Vector3::new(50.0, 0.0, 0.0), ..shots };
        let body = Trouble { id: 4, kind: Kind::Body, ..more };
        assert!(shots.is(&t, &more) && !shots.is(&t, &far) && !shots.is(&t, &body));
        assert_eq!(shots.shunned(&t), Some((Vector3::zeros(), SHUN_REACH * 0.5)));
        assert_eq!(Trouble { strength: 0.1, ..shots }.shunned(&t), None);
        assert!(Kind::Body.strength() > Kind::Gunfire.strength());
    }
}
