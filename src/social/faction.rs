//! Factions: what a whole kind of NPC - security, the citizens, a gang - thinks of the player,
//! beyond what any one of them does. Each NPC's own regard comes and goes with it (see
//! [`crate::social::rumour`]); a faction's standing outlasts them all, so that one turning up
//! afresh - one of a city's crowd, say - starts from what its kind thinks of the player, not from
//! nothing.
//!
//! A deed counts by who it was done to. Each faction takes harm done to another as its
//! [`Factions::relation`] to it says: 1, as harm done to itself; less, as less; less than nothing,
//! as good news - a gang hearing that the player shot down one of security, say. Harm done to no
//! one in particular - a robbery - each takes as its relation to nobody says, 1 unless the game
//! says otherwise: a gang need not mind a bank robbed.
//!
//! The standing moves [`SHARE`] of what a deed moves a witness's regard by, once it is seen and
//! talked about (see [`Deed::spreads`]), and eases back to nothing over [`STANDING_HALF_LIFE`] -
//! far more slowly than any one NPC's regard.

use crate::social::opinion::{self, attitude, Attitude, Deed};
use std::collections::HashMap;

/// A faction, by the game's own number for it.
pub type Faction = u16;

/// How far a faction's standing moves, of how far a deed moves one that sees it done; how much
/// of its standing one of it starts with, turning up; and how long the standing takes to ease
/// halfway back to nothing, in seconds.
pub const SHARE: f32 = 0.5;
pub const INHERIT: f32 = 0.8;
pub const STANDING_HALF_LIFE: f32 = 3600.0;

tuning!(
    /// How a faction's standing with the player goes (see the module's constants, its defaults).
    {
        /// How far it moves, of how far a deed moves one that sees it done.
        share: f32 = SHARE,
        /// How much of it one of the faction starts with, turning up.
        inherit: f32 = INHERIT,
        /// How long it takes to ease halfway back to nothing, in seconds.
        standing_half_life: f32 = STANDING_HALF_LIFE,
    }
);

/// Every faction's standing with the player, and how each takes what is done to the others.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Factions {
    standing: Vec<f32>,
    /// How the first takes harm done to the second, or to nobody with none, where it is not 1.
    relations: HashMap<(Faction, Option<Faction>), f32>,
    pub tuning: Tuning,
}

impl Factions {
    /// `count` factions, each neither for the player nor against them, each taking harm done to
    /// anyone as done to itself.
    pub fn new(count: usize, tuning: Tuning) -> Self {
        Self { standing: vec![0.0; count], relations: HashMap::new(), tuning }
    }

    pub fn len(&self) -> usize {
        self.standing.len()
    }

    pub fn is_empty(&self) -> bool {
        self.standing.is_empty()
    }

    /// Has `faction` take harm done to `to` - nobody in particular, with none - as `relation`
    /// says: 1 as done to itself, 0 as nothing to it, -1 as good news.
    pub fn relate(&mut self, faction: Faction, to: Option<Faction>, relation: f32) {
        self.relations.insert((faction, to), relation.clamp(-1.0, 1.0));
    }

    /// How one of `faction` takes harm done to one of `to`: 1 as done to its own, from one of no
    /// faction or done to one of its own.
    pub fn relation(&self, faction: Option<Faction>, to: Option<Faction>) -> f32 {
        match faction {
            Some(f) if Some(f) != to => self.relations.get(&(f, to)).copied().unwrap_or(1.0),
            _ => 1.0,
        }
    }

    /// What `faction` thinks of the player, from -1 to 1: nothing, for none.
    pub fn standing(&self, faction: Option<Faction>) -> f32 {
        faction.and_then(|f| self.standing.get(f as usize)).copied().unwrap_or(0.0)
    }

    /// How `faction` takes the player, by its standing.
    pub fn attitude(&self, opinion: &opinion::Tuning, faction: Option<Faction>) -> Attitude {
        attitude(opinion, self.standing(faction))
    }

    /// The regard one of `faction` starts with, turning up: [`INHERIT`] of its standing.
    pub fn inherited(&self, faction: Option<Faction>) -> f32 {
        self.standing(faction) * self.tuning.inherit
    }

    /// `deed` done to one of `victim` - to nobody in particular, with none - was seen, and is
    /// talked about: every faction's standing moves [`SHARE`] of its weight, as it takes harm done
    /// to the victim's. Nothing, for a deed not talked about. Each faction whose attitude to the
    /// player changed, with how it takes them now.
    pub fn hear_of(&mut self, opinion: &opinion::Tuning, deed: Deed, victim: Option<Faction>) -> Vec<(Faction, Attitude)> {
        if !deed.spreads() {
            return Vec::new();
        }
        let mut changed = Vec::new();
        for f in 0..self.standing.len() {
            let faction = f as Faction;
            let before = attitude(opinion, self.standing[f]);
            let by = deed.weight() * self.relation(Some(faction), victim) * self.tuning.share;
            self.standing[f] = (self.standing[f] + by).clamp(-1.0, 1.0);
            let after = attitude(opinion, self.standing[f]);
            if after != before {
                changed.push((faction, after));
            }
        }
        changed
    }

    /// Moves every standing on `dt`, easing back to nothing.
    pub fn step(&mut self, dt: f32) {
        let ease = 0.5f32.powf(dt / self.tuning.standing_half_life);
        for standing in &mut self.standing {
            *standing *= ease;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECURITY: Faction = 0;
    const CITIZENS: Faction = 1;
    const GANG: Faction = 2;

    fn city() -> Factions {
        let mut factions = Factions::new(3, Tuning::DEFAULT);
        factions.relate(SECURITY, Some(GANG), -0.4);
        factions.relate(GANG, Some(SECURITY), -0.6);
        factions.relate(CITIZENS, Some(GANG), 0.3);
        factions.relate(GANG, None, 0.0);
        factions
    }

    #[test]
    fn a_deed_counts_by_who_it_was_done_to() {
        let opinion = opinion::Tuning::DEFAULT;
        let mut factions = city();
        factions.hear_of(&opinion, Deed::Shooting, Some(GANG));
        assert!(factions.standing(Some(GANG)) < factions.standing(Some(CITIZENS)), "its own mind it most");
        assert!(factions.standing(Some(CITIZENS)) < 0.0, "the citizens a little: it is a shooting");
        assert!(factions.standing(Some(SECURITY)) > 0.0, "security is glad of it");
        // Shooting one of security, the gang is glad, security is not.
        let mut factions = city();
        let changed = factions.hear_of(&opinion, Deed::Shooting, Some(SECURITY));
        assert!(factions.standing(Some(GANG)) > 0.0 && factions.standing(Some(SECURITY)) < 0.0);
        assert!(changed.contains(&(SECURITY, Attitude::Wary)), "{changed:?}");
        // A robbery, done to no one in particular: the gang does not mind it.
        let mut factions = city();
        factions.hear_of(&opinion, Deed::Robbing, None);
        assert_eq!(factions.standing(Some(GANG)), 0.0);
        assert!(factions.standing(Some(CITIZENS)) < 0.0);
    }

    #[test]
    fn what_is_not_talked_about_moves_no_standing_and_it_eases_back_slowly() {
        let opinion = opinion::Tuning::DEFAULT;
        let mut factions = city();
        factions.hear_of(&opinion, Deed::Bribing, None);
        assert_eq!(factions.standing(Some(CITIZENS)), 0.0);
        factions.hear_of(&opinion, Deed::Robbing, None);
        let before = factions.standing(Some(CITIZENS));
        factions.step(STANDING_HALF_LIFE);
        assert!((factions.standing(Some(CITIZENS)) - before / 2.0).abs() < 1e-5);
        assert!((factions.inherited(Some(CITIZENS)) - before / 2.0 * INHERIT).abs() < 1e-5);
        assert_eq!(factions.standing(None), 0.0);
        assert_eq!(factions.relation(Some(GANG), Some(GANG)), 1.0);
        assert_eq!(factions.relation(None, Some(GANG)), 1.0);
    }
}
