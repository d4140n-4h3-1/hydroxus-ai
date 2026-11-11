//! Relationships between NPCs: what one thinks of another, from -1 (an enemy) to 1 (a friend) -
//! its affinity. Where [`crate::social::opinion`] is what NPCs think of the player, this is what
//! they think of each other, and it decides what they tell each other and how far they believe it.
//!
//! Affinity starts from their factions (see [`crate::social::faction`]): one of its own kind,
//! the tuning's `kin`; one of another, as its faction takes harm done to that one's, times the
//! tuning's `outsider` - a rival gang's man is an enemy, a passer-by of no faction a stranger well
//! enough met. On top of that is what has passed between the two ([`Relations`]): time spent
//! together ([`Relations::together`]), help ([`Relations::helped`]), harm
//! ([`Relations::harmed`]), each easing back over the tuning's `half_life`. It need not be the
//! same both ways.
//!
//! Affinity is put to use two ways. One tells another what it knows only if it thinks well enough
//! of it ([`shares_with`]): enemies keep news to themselves. And it trusts the word of one it has
//! not yet tested by how it thinks of it ([`trust`], for [`crate::social::word::Trust::introduce`]):
//! a friend is believed sooner than a stranger, a stranger sooner than an enemy - until their word
//! proves right or wrong.

use crate::social::{
    faction::{Faction, Factions},
    word,
};

/// Affinity for one of its own faction, and how much of its faction's relation to another's it
/// starts with for one of that.
pub const KIN: f32 = 0.4;
pub const OUTSIDER: f32 = 0.3;
/// The affinity under which one keeps what it knows from another.
pub const SHARE_ABOVE: f32 = -0.2;
/// How far affinity at its most moves the trust one starts with in another's word.
pub const TRUST_SWAY: f32 = 0.3;
/// How fast time together builds affinity, per second, and how far it goes.
pub const TOGETHER: f32 = 0.002;
pub const MOST_TOGETHER: f32 = 0.4;
/// How far being helped, and being harmed, moves it.
pub const HELPED: f32 = 0.3;
pub const HARMED: f32 = 0.6;
/// How long what has passed between them takes to ease halfway back, in seconds.
pub const HALF_LIFE: f32 = 3600.0;

tuning!(
    /// How NPCs come to think of each other (see the module's constants, its defaults).
    {
        /// Affinity for its own kind, and how much of its faction's relation to another's.
        kin: f32 = KIN,
        outsider: f32 = OUTSIDER,
        /// Under which it keeps what it knows to itself.
        share_above: f32 = SHARE_ABOVE,
        /// How far affinity moves the trust it starts with.
        trust_sway: f32 = TRUST_SWAY,
        /// How fast time together builds affinity, per second, and how far.
        together: f32 = TOGETHER,
        most_together: f32 = MOST_TOGETHER,
        /// How far being helped, and harmed, moves it.
        helped: f32 = HELPED,
        harmed: f32 = HARMED,
        /// How long what has passed takes to ease halfway back, in seconds.
        half_life: f32 = HALF_LIFE,
    }
);

/// One NPC, as relationships know it: the game's number for it, and its faction.
pub type Who = (u32, Option<Faction>);

/// What has passed between one NPC and another, as the first has it.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Bond {
    pub of: u32,
    pub toward: u32,
    /// How far it moves the first's affinity for the second, as it was at `noted`.
    pub by: f32,
    pub noted: f32,
    /// How much of `by` is from time together, as it was at `noted`.
    pub together: f32,
}

impl Bond {
    fn fade(tuning: &Tuning, value: f32, noted: f32, now: f32) -> f32 {
        value * 0.5f32.powf((now - noted).max(0.0) / tuning.half_life.max(1.0e-3))
    }

    /// How far it moves affinity at `now`.
    pub fn now(&self, tuning: &Tuning, now: f32) -> f32 {
        Self::fade(tuning, self.by, self.noted, now)
    }
}

/// The affinity one of `of` starts with for one of `toward`, from their factions alone.
pub fn base(tuning: &Tuning, factions: &Factions, of: Option<Faction>, toward: Option<Faction>) -> f32 {
    match (of, toward) {
        (Some(a), Some(b)) if a == b => tuning.kin,
        (Some(_), _) => factions.relation(of, toward) * tuning.outsider,
        (None, _) => 0.0,
    }
}

/// Whether one with `affinity` for another tells it what it knows.
pub fn shares_with(tuning: &Tuning, affinity: f32) -> bool {
    affinity >= tuning.share_above
}

/// How far one with `affinity` for another trusts its word before it has tested it: the word
/// tuning's `trust`, moved by `trust_sway` of the affinity.
pub fn trust(tuning: &Tuning, words: &word::Tuning, affinity: f32) -> f32 {
    (words.trust + affinity.clamp(-1.0, 1.0) * tuning.trust_sway).clamp(0.0, 1.0)
}

/// What has passed between NPCs, each way.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Relations {
    bonds: Vec<Bond>,
}

impl Relations {
    /// Nothing passed between anyone.
    pub fn new() -> Self {
        Self::default()
    }

    /// What `of` thinks of `toward` at `now`, from -1 to 1: its faction's start, and what has
    /// passed between them.
    pub fn affinity(&self, tuning: &Tuning, factions: &Factions, of: Who, toward: Who, now: f32) -> f32 {
        let passed = self.bond(of.0, toward.0).map_or(0.0, |b| b.now(tuning, now));
        (base(tuning, factions, of.1, toward.1) + passed).clamp(-1.0, 1.0)
    }

    /// What has passed between `of` and `toward`, as `of` has it.
    pub fn bond(&self, of: u32, toward: u32) -> Option<&Bond> {
        self.bonds.iter().find(|b| b.of == of && b.toward == toward)
    }

    fn entry(&mut self, tuning: &Tuning, of: u32, toward: u32, now: f32) -> &mut Bond {
        let at = match self.bonds.iter().position(|b| b.of == of && b.toward == toward) {
            Some(n) => n,
            None => {
                self.bonds.push(Bond { of, toward, by: 0.0, noted: now, together: 0.0 });
                self.bonds.len() - 1
            }
        };
        let b = &mut self.bonds[at];
        b.by = b.now(tuning, now);
        b.together = Bond::fade(tuning, b.together, b.noted, now);
        b.noted = now;
        b
    }

    fn sane(dt: f32, now: f32) -> bool {
        dt.is_finite() && now.is_finite()
    }

    /// `a` and `b` spent `dt` seconds together, up to `now`: each thinks the better of the other,
    /// up to the tuning's `most_together` from that alone.
    pub fn together(&mut self, tuning: &Tuning, a: u32, b: u32, dt: f32, now: f32) {
        if a == b || !Self::sane(dt, now) {
            return;
        }
        for (of, toward) in [(a, b), (b, a)] {
            let bond = self.entry(tuning, of, toward, now);
            let gain = (tuning.together * dt.max(0.0)).min((tuning.most_together - bond.together).max(0.0));
            bond.together += gain;
            bond.by = (bond.by + gain).clamp(-2.0, 2.0);
        }
    }

    /// `of` was helped by `by` at `now`: it thinks the better of it.
    pub fn helped(&mut self, tuning: &Tuning, of: u32, by: u32, now: f32) {
        if of != by && now.is_finite() {
            let bond = self.entry(tuning, of, by, now);
            bond.by = (bond.by + tuning.helped).clamp(-2.0, 2.0);
        }
    }

    /// `of` was harmed by `by` at `now`: it thinks the worse of it.
    pub fn harmed(&mut self, tuning: &Tuning, of: u32, by: u32, now: f32) {
        if of != by && now.is_finite() {
            let bond = self.entry(tuning, of, by, now);
            bond.by = (bond.by - tuning.harmed).clamp(-2.0, 2.0);
        }
    }

    /// Forgets what has passed between any two that has eased back under `least` at `now`.
    pub fn forget(&mut self, tuning: &Tuning, now: f32, least: f32) {
        self.bonds.retain(|b| b.now(tuning, now).abs() >= least);
    }

    /// Everything that has passed between anyone.
    pub fn bonds(&self) -> &[Bond] {
        &self.bonds
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T: Tuning = Tuning::DEFAULT;
    const SECURITY: Faction = 0;
    const GANG: Faction = 1;
    const RIVALS: Faction = 2;

    fn town() -> Factions {
        let mut f = Factions::new(3, Default::default());
        f.relate(GANG, Some(RIVALS), -1.0);
        f.relate(RIVALS, Some(GANG), -1.0);
        f.relate(GANG, Some(SECURITY), -0.5);
        f
    }

    #[test]
    fn affinity_starts_from_their_factions() {
        let (f, r) = (town(), Relations::new());
        let aff = |a: Who, b: Who| r.affinity(&T, &f, a, b, 0.0);
        assert_eq!(aff((1, Some(GANG)), (2, Some(GANG))), KIN);
        assert_eq!(aff((1, Some(GANG)), (3, Some(RIVALS))), -OUTSIDER);
        assert_eq!(aff((1, Some(GANG)), (4, Some(SECURITY))), -0.5 * OUTSIDER);
        // Security has no quarrel with the gang the gang has with it: it need not be the same both ways.
        assert_eq!(aff((4, Some(SECURITY)), (1, Some(GANG))), OUTSIDER);
        assert_eq!(aff((5, None), (1, Some(GANG))), 0.0);
        // Enemies keep what they know to themselves; the rest tell.
        assert!(!shares_with(&T, aff((1, Some(GANG)), (3, Some(RIVALS)))));
        assert!(shares_with(&T, aff((1, Some(GANG)), (4, Some(SECURITY)))));
        assert!(shares_with(&T, aff((5, None), (1, Some(GANG)))));
    }

    #[test]
    fn what_passes_between_them_moves_it_and_eases_back() {
        let f = town();
        let mut r = Relations::new();
        let (a, b) = ((1, Some(GANG)), (3, Some(RIVALS)));
        // Ten minutes together: each thinks better of the other, but no further than time
        // together goes.
        for n in 0..600 {
            r.together(&T, 1, 3, 1.0, n as f32);
        }
        let after = r.affinity(&T, &f, a, b, 600.0);
        assert!(after > -OUTSIDER + 0.3 && after <= -OUTSIDER + MOST_TOGETHER + 1.0e-4, "{after}");
        assert!((r.affinity(&T, &f, b, a, 600.0) - after).abs() < 1.0e-4);
        // Two help it out of trouble: friends.
        r.helped(&T, 1, 3, 600.0);
        r.helped(&T, 1, 3, 600.0);
        assert!(r.affinity(&T, &f, a, b, 600.0) > 0.3);
        // An hour on, what passed has eased halfway back.
        let passed = r.affinity(&T, &f, a, b, 600.0) - base(&T, &f, a.1, b.1);
        let later = r.affinity(&T, &f, a, b, 600.0 + HALF_LIFE) - base(&T, &f, a.1, b.1);
        assert!((later - passed * 0.5).abs() < 1.0e-4, "{later} {passed}");
        // Harmed by one of its own: it thinks the worse of it, and that one doesn't change its mind.
        r.harmed(&T, 7, 8, 0.0);
        let kin = (Some(GANG), Some(GANG));
        assert!(r.affinity(&T, &f, (7, kin.0), (8, kin.1), 0.0) < 0.0);
        assert_eq!(r.affinity(&T, &f, (8, kin.0), (7, kin.1), 0.0), KIN);
        r.forget(&T, 1.0e6, 0.01);
        assert!(r.bonds().is_empty());
    }

    #[test]
    fn friends_are_believed_sooner_than_strangers_and_strangers_than_enemies() {
        let w = word::Tuning::DEFAULT;
        let friend = trust(&T, &w, 0.8);
        let stranger = trust(&T, &w, 0.0);
        let enemy = trust(&T, &w, -0.8);
        assert!(friend > stranger && stranger > enemy, "{friend} {stranger} {enemy}");
        assert_eq!(stranger, w.trust);
        assert!((0.0..=1.0).contains(&trust(&T, &w, f32::INFINITY)));
    }

    #[test]
    fn nonsense_changes_nothing() {
        let mut r = Relations::new();
        r.together(&T, 1, 2, f32::NAN, 0.0);
        r.helped(&T, 1, 1, 0.0);
        r.harmed(&T, 1, 2, f32::INFINITY);
        assert!(r.bonds().is_empty());
    }
}
