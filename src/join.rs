//! Joining in: an NPC of a kind that hunts - a sentry, say - that sees another of its kind after
//! the player goes after them too, calm or not: the one it sees shows it where the player is. It
//! sees the other as it would see the player standing, and one that sees one that has joined in
//! joins in as well, so a chase draws in every one of them that catches sight of it.

use crate::{
    alert::Alert,
    sight::{Sight, Stance},
};
use nalgebra::Vector3;

/// Whether an NPC at `feet`, facing `heading`, in `alert` - or calm, with none - could see
/// another at `other`, if nothing were in the way: as it would see the player standing there.
pub fn could_see_other(
    alert: Option<Alert>,
    feet: Vector3<f32>,
    heading: f32,
    other: Vector3<f32>,
    in_the_dark: bool,
) -> bool {
    Sight::default().could_see(alert, feet, heading, other, Stance::Standing, in_the_dark)
}

/// What deciding who joins a chase takes from an NPC.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Watch {
    /// Whether its kind joins in: a sentry.
    pub sentry: bool,
    pub alert: Option<Alert>,
    pub sees_player: bool,
    /// Down, or talking to the player.
    pub busy: bool,
    /// How long, having just turned hostile, it stands before going after the player, in seconds.
    pub windup: f32,
    pub feet: Vector3<f32>,
    pub heading: f32,
}

/// Which of `watches` join a chase: each sentry, not busy nor after the player already, that
/// could see a sentry after them - done standing, having just turned hostile - by
/// [`could_see_other`], `in_the_dark` or not, where `clear` says nothing is in the way from the
/// one, as an index, to the other.
pub fn joiners(
    watches: &[Watch],
    in_the_dark: bool,
    clear: impl Fn(usize, usize) -> bool,
) -> Vec<usize> {
    let after_player =
        |watch: &Watch| watch.alert == Some(Alert::Alert) && watch.sees_player && !watch.busy;
    let chasing: Vec<usize> = (0..watches.len())
        .filter(|&n| watches[n].sentry && after_player(&watches[n]) && watches[n].windup == 0.0)
        .collect();
    (0..watches.len())
        .filter(|&m| {
            let watch = &watches[m];
            watch.sentry
                && !watch.busy
                && !after_player(watch)
                && chasing.iter().any(|&n| {
                    n != m
                        && could_see_other(watch.alert, watch.feet, watch.heading, watches[n].feet, in_the_dark)
                        && clear(m, n)
                })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_sees_another_as_it_would_the_player_standing() {
        let sees = |alert, x: f32, z: f32, dark| {
            could_see_other(alert, Vector3::zeros(), 0.0, Vector3::new(x, 0.0, z), dark)
        };
        // Calm, it looks about it as it does wary: ahead, off to the side only near, out of the
        // corner of its eye, and not behind.
        assert!(sees(None, 0.0, 20.0, false), "ahead");
        assert!(!sees(None, 0.0, -10.0, false), "behind");
        assert!(sees(None, 5.0, 1.0, false), "near, off to the side");
        assert!(!sees(None, 15.0, 1.0, false), "far off to the side");
        assert!(!sees(None, 0.0, 20.0, true), "far off, in the dark");
        // Searching or after the player, the same: only in front.
        assert!(!sees(Some(Alert::Evasion), 0.0, -10.0, false));
        assert!(!sees(Some(Alert::Alert), 0.0, -10.0, false));
        assert!(sees(Some(Alert::Alert), 0.0, 10.0, false));
        assert!(!sees(Some(Alert::Alert), 0.0, Sight::default().range + 1.0, false));
    }

    #[test]
    fn a_sentry_that_sees_another_after_the_player_joins_in() {
        let watch = |sentry, alert, sees_player, z: f32| Watch {
            sentry,
            alert,
            sees_player,
            busy: false,
            windup: 0.0,
            feet: Vector3::new(0.0, 0.0, z),
            heading: 0.0,
        };
        let chaser = || watch(true, Some(Alert::Alert), true, 10.0);
        let open = |_, _| true;
        // Calm and searching sentries facing the chase join in; the chaser does not.
        let calm = watch(true, None, false, 0.0);
        let searching = watch(true, Some(Alert::Evasion), false, 0.0);
        assert_eq!(joiners(&[chaser(), calm], false, open), [1]);
        assert_eq!(joiners(&[chaser(), searching], false, open), [1]);
        // Not a sentry, facing away, with a wall between, or talking: it does not.
        assert!(joiners(&[chaser(), watch(false, None, false, 0.0)], false, open).is_empty());
        let facing_away = watch(true, None, false, 20.0);
        assert!(joiners(&[chaser(), facing_away], false, open).is_empty());
        let walled_off = joiners(&[chaser(), watch(true, None, false, 0.0)], false, |_, _| false);
        assert!(walled_off.is_empty());
        let talking = Watch { busy: true, ..watch(true, None, false, 0.0) };
        assert!(joiners(&[chaser(), talking], false, open).is_empty());
        // One still standing, having just turned hostile, is not yet after them; nor is one on
        // Alert that has lost sight of them.
        let winding_up = Watch { windup: 0.5, ..chaser() };
        assert!(joiners(&[winding_up, watch(true, None, false, 0.0)], false, open).is_empty());
        let lost = watch(true, Some(Alert::Alert), false, 10.0);
        assert!(joiners(&[lost, watch(true, None, false, 0.0)], false, open).is_empty());
    }
}
