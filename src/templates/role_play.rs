//! Reusable NPC roles with different temperaments and daily routines.
//!
//! Use [`RolePlay::role`] and [`RolePlay::traits`] when setting up the NPC's
//! decision-making. Use its routine to pick an activity during downtime. The game
//! maps activities to places and actions, and decides when an interruption ends.

use crate::{
    life::routine::{self, Block, Routine},
    tactics::{conduct::Role, priority::Traits},
};

/// A job and temperament for a role-playing NPC.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum RolePlay {
    Guard,
    Worker,
    Merchant,
    Scout,
    Bandit,
    Medic,
    Courier,
}

/// A routine activity for the game to carry out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum Activity {
    Rest,
    Patrol,
    Work,
    Trade,
    Scout,
    Socialize,
    Raid,
    /// Tend to patients or injured allies.
    Treat,
    /// Carry a message or supplies to a recipient.
    Deliver,
}

impl RolePlay {
    /// Choices for a character or NPC setup menu.
    pub const ALL: [Self; 7] = [
        Self::Guard,
        Self::Worker,
        Self::Merchant,
        Self::Scout,
        Self::Bandit,
        Self::Medic,
        Self::Courier,
    ];

    /// The conduct profile used when the NPC reacts to events.
    pub const fn role(self) -> Role {
        match self {
            Self::Guard | Self::Scout => Role::Sentry,
            Self::Worker | Self::Merchant | Self::Medic | Self::Courier => Role::Citizen,
            Self::Bandit => Role::Gang,
        }
    }

    /// The starting personality. Games can customize individual NPCs from here.
    pub const fn traits(self) -> Traits {
        match self {
            Self::Guard => Traits {
                observant: 0.75,
                caution: 0.65,
                ..Traits::NEUTRAL
            },
            Self::Worker => Traits {
                aggression: 0.25,
                caution: 0.75,
                ..Traits::NEUTRAL
            },
            Self::Merchant => Traits {
                aggression: 0.2,
                sociability: 0.85,
                ..Traits::NEUTRAL
            },
            Self::Medic => Traits {
                aggression: 0.2,
                caution: 0.7,
                sociability: 0.75,
                ..Traits::NEUTRAL
            },
            Self::Courier => Traits {
                aggression: 0.25,
                observant: 0.7,
                independence: 0.75,
                ..Traits::NEUTRAL
            },
            Self::Bandit => Traits {
                aggression: 0.8,
                caution: 0.6,
                sociability: 0.65,
                ..Traits::NEUTRAL
            },
            Self::Scout => Traits {
                curiosity: 0.8,
                observant: 0.85,
                independence: 0.75,
                ..Traits::NEUTRAL
            },
        }
    }

    /// An NPC's daily routine in game hours, shifted by a stable offset from its ID.
    /// Query [`Routine::at`] with the current game hour when the NPC is free to
    /// resume its routine after an interruption.
    pub fn routine(self, tuning: &routine::Tuning, id: u32) -> Routine<Activity> {
        let (start, duty, end) = match self {
            Self::Guard => (6.0, Activity::Patrol, 18.0),
            Self::Worker => (8.0, Activity::Work, 17.0),
            Self::Merchant => (9.0, Activity::Trade, 18.0),
            Self::Scout => (5.0, Activity::Scout, 16.0),
            Self::Bandit => (10.0, Activity::Raid, 19.0),
            Self::Medic => (7.0, Activity::Treat, 19.0),
            Self::Courier => (6.0, Activity::Deliver, 17.0),
        };
        Routine::new([
            Block {
                from: start,
                doing: duty,
            },
            Block {
                from: end,
                doing: Activity::Socialize,
            },
            Block {
                from: 22.0,
                doing: Activity::Rest,
            },
        ])
        .expect("the role has a routine with fixed hours")
        .shifted(routine::offset(tuning, id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profiles_have_distinct_duties_and_can_resume_by_game_time() {
        let expected = [
            Activity::Patrol,
            Activity::Work,
            Activity::Trade,
            Activity::Scout,
            Activity::Raid,
            Activity::Treat,
            Activity::Deliver,
        ];
        assert_eq!(RolePlay::Bandit.role(), Role::Gang);
        assert_eq!(RolePlay::Medic.role(), Role::Citizen);
        assert_eq!(RolePlay::Courier.role(), Role::Citizen);
        let tuning = routine::Tuning::DEFAULT;
        for (role, duty) in RolePlay::ALL.into_iter().zip(expected) {
            let day = role.routine(&tuning, 42);
            assert_eq!(*day.at(0.0), Activity::Rest);
            assert_eq!(*day.at(12.0), duty);
            assert_eq!(*day.at(20.0), Activity::Socialize);
            assert_eq!(day, role.routine(&tuning, 42));
            assert_ne!(day, role.routine(&tuning, 43));
        }
    }
}
