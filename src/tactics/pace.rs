//! Pacing a chase: breaking into a sprint now and then, never at the start, and - for one with
//! breath to keep - easing off before it runs out, keeping some back for a last burst.

use crate::{navigation::grid::Rng, navigation::route::between};

/// How long, in seconds, an NPC on Alert runs before it breaks into a sprint, from least to
/// most - picked afresh each time, so the player cannot count on when - and how long each sprint
/// lasts. It never sprints the moment it turns on them.
pub const SPRINT_EVERY: (f32, f32) = (4.0, 10.0);
pub const SPRINT_FOR: (f32, f32) = (1.5, 3.0);
/// How one pacing itself goes after someone, by the share of its breath it has left: under
/// [`SPRINT_RESERVE`] it sprints no more, and under [`EASE_OFF`] it eases off to a jog until it is
/// back over [`EASED`] - so that it is never left winded, walking.
pub const SPRINT_RESERVE: f32 = 0.5;
pub const EASE_OFF: f32 = 0.2;
pub const EASED: f32 = 0.6;

tuning!(
    /// How an NPC paces a chase (see the module's constants, its defaults).
    {
        /// How long it runs before it breaks into a sprint, and how long each sprint lasts, in
        /// seconds, from least to most.
        sprint_every: (f32, f32) = SPRINT_EVERY,
        sprint_for: (f32, f32) = SPRINT_FOR,
        /// By the share of its breath left: under which it sprints no more, under which it eases
        /// off to a jog, and over which it is back to running.
        sprint_reserve: f32 = SPRINT_RESERVE,
        ease_off: f32 = EASE_OFF,
        eased: f32 = EASED,
    }
);

/// How long until an NPC sprints, `sprint_in` - below 0 before it has been picked - and how
/// long it has left `sprinting`, after another `dt` of `chasing` someone or not: a sprint comes
/// some while into a chase, never at its start, and again some while after each ends.
pub fn sprints(tuning: &Tuning, sprint_in: f32, sprinting: f32, chasing: bool, rng: &mut Rng, dt: f32) -> (f32, f32) {
    if !chasing {
        return (-1.0, 0.0);
    }
    if sprinting > 0.0 {
        return (sprint_in, sprinting - dt);
    }
    if sprint_in < 0.0 {
        return (between(rng, tuning.sprint_every), 0.0);
    }
    match sprint_in - dt {
        due if due <= 0.0 => (between(rng, tuning.sprint_every), between(rng, tuning.sprint_for)),
        due => (due, 0.0),
    }
}

/// Whether one `pacing` itself - with no one near enough for a last burst - eases off to a jog,
/// having been `easing` or not, with `left` of its breath, as a share of its most; and whether
/// it keeps from sprinting (see [`SPRINT_RESERVE`]).
pub fn pace(tuning: &Tuning, pacing: bool, easing: bool, left: f32) -> (bool, bool) {
    let ease_under = if easing { tuning.eased } else { tuning.ease_off };
    (pacing && left < ease_under, pacing && left < tuning.sprint_reserve)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_sprints_now_and_then_but_never_as_the_chase_starts() {
        let dt = 0.05;
        let mut rng = Rng::new(3);
        let (mut sprint_in, mut sprinting) = (-1.0, 0.0);
        let mut first = None;
        let mut sprints_begun = 0;
        for frame in 0..(60.0 / dt) as u32 {
            let was = sprinting > 0.0;
            (sprint_in, sprinting) = sprints(&Tuning::DEFAULT, sprint_in, sprinting, true, &mut rng, dt);
            if !was && sprinting > 0.0 {
                sprints_begun += 1;
                first.get_or_insert(frame as f32 * dt);
            }
        }
        let first = first.expect("it sprints");
        assert!(first >= SPRINT_EVERY.0 - dt, "not straight away: {first}");
        assert!(sprints_begun >= 3, "again and again: {sprints_begun}");
        // No longer chasing, it forgets, and starts over when it chases again.
        assert_eq!(sprints(&Tuning::DEFAULT, 2.0, 1.0, false, &mut rng, dt), (-1.0, 0.0));
    }

    #[test]
    fn one_pacing_itself_keeps_its_breath_for_a_last_burst() {
        // Plenty of breath: it sprints and runs as it likes.
        assert_eq!(pace(&Tuning::DEFAULT, true, false, 0.9), (false, false));
        // Under its reserve it sprints no more, but runs on.
        assert_eq!(pace(&Tuning::DEFAULT, true, false, 0.4), (false, true));
        // Low, it eases off to a jog, and keeps to it until well back over that.
        assert_eq!(pace(&Tuning::DEFAULT, true, false, 0.15), (true, true));
        assert_eq!(pace(&Tuning::DEFAULT, true, true, 0.4), (true, true));
        assert_eq!(pace(&Tuning::DEFAULT, true, true, 0.65), (false, false));
        // Near enough to catch them, or not pacing itself, it spends all it has.
        assert_eq!(pace(&Tuning::DEFAULT, false, true, 0.1), (false, false));
    }
}
