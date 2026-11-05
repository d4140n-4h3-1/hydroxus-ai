//! Shooting: an armed NPC shoots at whoever it is after from as far as [`FIRE_RANGE`] meters,
//! coming on to [`ENGAGE`] and standing there; once every so many seconds, as [`FIRE_EVERY`]
//! says; wide by up to [`SPREAD`] meters every ten meters away; and only facing within
//! [`FIRE_CONE`] radians of them. A gang is wilder: it comes on to [`GANG_ENGAGE`] meters before it
//! stands to shoot, and fires as often as [`GANG_FIRE_EVERY`] says. A rifle fires its shots
//! [`RIFLE_SPREAD`] times as wide, in bursts: three-round ([`BURST_SHOTS`]), as most are set to,
//! or a long string on full auto ([`AUTO_BURST`]) - as fast as it fires, and then as long a pause
//! as a pistol's between shots, give or take.

use crate::{flat, navigation::grid::Rng, navigation::route::between, tactics::blind};
use nalgebra::Vector3;

pub const FIRE_RANGE: f32 = 24.0;
pub const ENGAGE: f32 = 11.0;
pub const FIRE_EVERY: (f32, f32) = (0.7, 1.3);
pub const GANG_ENGAGE: f32 = 6.0;
pub const GANG_FIRE_EVERY: (f32, f32) = (0.45, 0.9);
pub const SPREAD: f32 = 0.55;
pub const RIFLE_SPREAD: f32 = 1.5;
pub const BURST_SHOTS: u8 = 3;
pub const AUTO_BURST: u8 = 8;
pub const FIRE_CONE: f32 = 0.3;

tuning!(
    /// How an armed NPC shoots (see the module's constants, its defaults).
    {
        /// How far off it fires from, and how near it comes before it stands to shoot, in meters.
        fire_range: f32 = FIRE_RANGE,
        engage: f32 = ENGAGE,
        /// How long between its shots, in seconds, from least to most.
        fire_every: (f32, f32) = FIRE_EVERY,
        /// One of a gang: how near it comes, and how long between its shots.
        gang_engage: f32 = GANG_ENGAGE,
        gang_fire_every: (f32, f32) = GANG_FIRE_EVERY,
        /// How wide its shots go, in meters every ten meters away, and how many times as wide
        /// from a rifle.
        spread: f32 = SPREAD,
        rifle_spread: f32 = RIFLE_SPREAD,
        /// How many shots a rifle fires in a burst, and in a string on full auto.
        burst_shots: u8 = BURST_SHOTS,
        auto_burst: u8 = AUTO_BURST,
        /// How nearly it has to face whoever it fires at, in radians.
        fire_cone: f32 = FIRE_CONE,
    }
);

/// Whether one at `feet` facing `heading` faces `at` nearly enough to fire at it.
pub fn facing(tuning: &Tuning, feet: Vector3<f32>, heading: f32, at: Vector3<f32>) -> bool {
    let to = flat(at - feet);
    let off = (to.x.atan2(to.z) - heading + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
        - std::f32::consts::PI;
    off.abs() < tuning.fire_cone
}

/// How long between shots, in seconds, from least to most: firing `blind` (see
/// [`crate::tactics::blind`], as `blinded` says), one of a `gang`, or otherwise.
pub fn every(tuning: &Tuning, blinded: &blind::Tuning, blind: bool, gang: bool) -> (f32, f32) {
    match (blind, gang) {
        (true, _) => blinded.blind_fire_every,
        (false, true) => tuning.gang_fire_every,
        (false, false) => tuning.fire_every,
    }
}

/// How far off a shot from `from` at `at` goes, picked by `rng`: wider the further, and wider
/// from a `rifle` - across, and a little less up and down.
pub fn miss(tuning: &Tuning, from: Vector3<f32>, at: Vector3<f32>, rifle: bool, rng: &mut Rng) -> Vector3<f32> {
    let wide = tuning.spread * if rifle { tuning.rifle_spread } else { 1.0 } * (at - from).norm() / 10.0;
    Vector3::new(
        between(rng, (-wide, wide)),
        between(rng, (-0.6 * wide, 0.6 * wide)),
        between(rng, (-wide, wide)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_fires_only_facing_them_and_wider_the_further_and_from_a_rifle() {
        let feet = Vector3::zeros();
        assert!(facing(&Tuning::DEFAULT, feet, 0.0, Vector3::new(0.5, 0.0, 10.0)));
        assert!(!facing(&Tuning::DEFAULT, feet, 0.0, Vector3::new(5.0, 0.0, 10.0)));
        assert!(facing(&Tuning::DEFAULT, feet, std::f32::consts::PI, Vector3::new(0.0, 0.0, -10.0)));
        let mut rng = Rng::new(3);
        let widest = |to: f32, rifle: bool, rng: &mut Rng| {
            (0..200).map(|_| miss(&Tuning::DEFAULT, feet, Vector3::new(0.0, 0.0, to), rifle, rng).x.abs()).fold(0.0, f32::max)
        };
        assert!(widest(10.0, false, &mut rng) <= SPREAD);
        assert!(widest(40.0, false, &mut rng) > SPREAD);
        assert!(widest(10.0, true, &mut rng) > SPREAD);
        assert_eq!(every(&Tuning::DEFAULT, &blind::Tuning::DEFAULT, true, true), blind::BLIND_FIRE_EVERY);
    }
}
