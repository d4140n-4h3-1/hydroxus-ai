//! Being threatened: an NPC with a weapon pointed at it stands for it a while, then warns whoever
//! holds it, warns them again, and then does something about it - or, with the weapon off it
//! long enough, calms down again. How long it stands for it at first is up to its kind.

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
pub fn pointed_at(eye: Vector3<f32>, ahead: Vector3<f32>, target: Vector3<f32>) -> bool {
    let to = target - eye;
    let along = to.dot(&ahead);
    along > 0.0 && along < AIM_RANGE && (to - ahead * along).norm() < AIM_WIDTH
}

/// How long an NPC that stands `patience` seconds of a weapon has it pointed at it, in seconds,
/// after it has warned whoever holds it `warned` times, before it goes on to the next stage:
/// first its patience, then that and as long again as it takes to warn them.
pub fn stage_after(warned: u8, patience: f32) -> f32 {
    if warned == 0 {
        patience
    } else {
        patience + WARNING
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_weapon_is_pointed_at_what_is_in_the_middle_of_the_view() {
        let eye = Vector3::zeros();
        let ahead = Vector3::z();
        assert!(pointed_at(eye, ahead, Vector3::new(0.3, 0.0, 10.0)));
        assert!(!pointed_at(eye, ahead, Vector3::new(1.0, 0.0, 10.0)), "off to the side");
        assert!(!pointed_at(eye, ahead, Vector3::new(0.0, 0.0, -5.0)), "behind");
        assert!(!pointed_at(eye, ahead, Vector3::new(0.0, 0.0, AIM_RANGE + 1.0)), "too far");
    }

    #[test]
    fn each_warning_holds_long_enough_to_be_said() {
        assert_eq!(stage_after(0, 1.0), 1.0, "its patience, before it first warns");
        assert!(stage_after(1, 1.0) >= WARNING, "then time to say it");
        assert_eq!(stage_after(1, 1.0), stage_after(2, 1.0));
        assert!(stage_after(0, 1.0) < stage_after(0, 2.0), "a less patient NPC is sooner");
    }
}
