//! What an NPC thinks of the player: its regard, from -1 (it hates them) to 1 (it likes them),
//! moved by what it sees them do - and, less, by what it hears they did (see
//! [`crate::social::rumour`]) - and easing back to nothing as time goes by.

/// Something the player did, as NPCs take it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Deed {
    /// Shot someone down.
    Shooting,
    /// Robbed a bank, a store, a vault.
    Robbing,
    /// Pointed a weapon at someone.
    Threatening,
    /// Went about with a weapon drawn, near someone.
    Brandishing,
    /// Paid someone off.
    Bribing,
    /// Talked someone round.
    Persuading,
}

impl Deed {
    /// How much seeing it done to it, or before its eyes, moves an NPC's regard.
    pub fn weight(self) -> f32 {
        match self {
            Deed::Shooting => -0.6,
            Deed::Robbing => -0.3,
            Deed::Threatening => -0.25,
            Deed::Brandishing => -0.05,
            Deed::Bribing => 0.15,
            Deed::Persuading => 0.05,
        }
    }

    /// Whether it is talked about: told on to others as a rumour. A bribe, being talked round, or a
    /// weapon only drawn, is kept to oneself.
    pub fn spreads(self) -> bool {
        matches!(self, Deed::Shooting | Deed::Robbing | Deed::Threatening)
    }
}

/// How an NPC takes the player, by its regard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Attitude {
    Friendly,
    Neutral,
    Wary,
    Hostile,
}

/// The regard over which an NPC is friendly, and under which it is wary, and hostile.
pub const FRIENDLY: f32 = 0.3;
pub const WARY: f32 = -0.2;
pub const HOSTILE: f32 = -0.55;
/// How many points of chance a talk check is the easier for an NPC that likes the player all it
/// can - and the harder for one that hates them.
pub const SWAY: f32 = 20.0;
/// How long its regard takes to ease halfway back to nothing, in seconds.
pub const REGARD_HALF_LIFE: f32 = 1200.0;

tuning!(
    /// How an NPC's regard for the player turns into how it takes them (see the module's
    /// constants, its defaults).
    {
        /// The regard over which it is friendly, and under which it is wary, and hostile.
        friendly: f32 = FRIENDLY,
        wary: f32 = WARY,
        hostile: f32 = HOSTILE,
        /// How many points of chance on a talk check regard all one way or the other is worth.
        sway: f32 = SWAY,
        /// How long its regard takes to ease halfway back to nothing, in seconds.
        regard_half_life: f32 = REGARD_HALF_LIFE,
    }
);

/// How an NPC with `regard` takes the player.
pub fn attitude(tuning: &Tuning, regard: f32) -> Attitude {
    if regard > tuning.friendly {
        Attitude::Friendly
    } else if regard < tuning.hostile {
        Attitude::Hostile
    } else if regard < tuning.wary {
        Attitude::Wary
    } else {
        Attitude::Neutral
    }
}

/// How many points of chance an NPC's `regard` puts on a talk check, either way.
pub fn sway(tuning: &Tuning, regard: f32) -> i32 {
    (regard.clamp(-1.0, 1.0) * tuning.sway).round() as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regard_makes_an_attitude_and_sways_a_talk_check() {
        assert_eq!(attitude(&Tuning::DEFAULT, 0.0), Attitude::Neutral);
        assert_eq!(attitude(&Tuning::DEFAULT, 0.5), Attitude::Friendly);
        assert_eq!(attitude(&Tuning::DEFAULT, -0.3), Attitude::Wary);
        assert_eq!(attitude(&Tuning::DEFAULT, -0.8), Attitude::Hostile);
        assert_eq!(sway(&Tuning::DEFAULT, 1.0), 20);
        assert_eq!(sway(&Tuning::DEFAULT, -0.5), -10);
        assert!(Deed::Shooting.weight() < Deed::Robbing.weight() && Deed::Bribing.weight() > 0.0);
        assert!(Deed::Shooting.spreads() && !Deed::Bribing.spreads());
    }
}
