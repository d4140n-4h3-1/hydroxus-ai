//! What an NPC does next, by utility (see [`crate::tactics::utility`]): of everything it could be
//! doing - chasing the player, searching, investigating what it noticed, fighting one of the other
//! side, telling a sentry, sheltering from a shootout, mourning one of its own, backing off from a
//! weapon drawn, showing the player what it thinks of them, keeping by its partner, going about its
//! business - each scored against what it takes in, the best done.
//!
//! What it is held to is not chosen: down, being carried, in a foot chase or caught, talking, just
//! turned hostile - a game sees to those first. Of the rest, each [`Conduct`] is only open to it
//! as its situation has it ([`Input`], each 0 or 1 but [`Input::Nerve`] and [`Input::Wariness`]):
//! no searching with no one to search for. What its [`Role`] makes of them weighs them: security
//! goes after the player before all, a citizen thinks of itself first, a gang of its quarrel.
//!
//! Danger comes before company: a droid caught by a shootout shelters from it before it stops to
//! look at the player, mourns one of its own, or backs off from a weapon - and the more nerve it
//! has, the likelier it stays by one of its own down rather than take cover; the warier it is, the
//! less it stops to look at anyone.

use crate::tactics::utility::{Action, Curve, Reasoner, Situation};

/// What an NPC can be doing, by choice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Conduct {
    /// After the player, seen.
    Chase,
    /// After one of the other side.
    Fight,
    /// After whoever shot at it, unseen.
    ShootBack,
    /// Going to look at what it noticed.
    Investigate,
    /// Searching for the player, lost.
    Search,
    /// Going to tell a sentry what it saw.
    Report,
    /// Sheltering from a shootout.
    Shelter,
    /// Kneeling by one of its own, down.
    Mourn,
    /// Backing off from a weapon drawn too near.
    BackAway,
    /// Watching whoever has a weapon drawn.
    Watch,
    /// Keeping away from the player, passing.
    Avoid,
    /// Stopping to look at the player, passing: a greeting, a nod, a glare.
    Regard,
    /// Keeping by its partner, or about the one leading its group.
    Follow,
    /// Going about its business.
    GoAbout,
}

/// What an NPC takes in, to choose by: each 1 if so and 0 if not, but nerve and wariness, from 0
/// to 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Input {
    /// It is after the player, seeing them.
    Hunting,
    /// It has one of the other side to go after.
    Foe,
    /// It was shot at by someone it has not seen, on a side.
    ShotAt,
    /// It noticed something, and is going to look.
    Investigating,
    /// It has lost the player, and searches.
    Searching,
    /// It saw something and has a sentry to tell.
    Reporting,
    /// A shootout near it, it has somewhere to shelter.
    Sheltering,
    /// One of its own is down, and it has a mind to go to it.
    Mourning,
    /// A weapon drawn too near: it would back off.
    Afraid,
    /// A weapon drawn: it would watch whoever holds it.
    Watching,
    /// Wary of the player passing, near: it would keep away.
    Avoiding,
    /// The player passing: it would stop to look at them.
    Regarding,
    /// It walks with others, and is not the one leading.
    Partnered,
    /// Its nerve, from 0 to 1.
    Nerve,
    /// How wary it is, from 0 to 1, from what it has been through (see
    /// [`crate::alert::Wariness`]).
    Wariness,
}

/// What kind of NPC it is, which weighs its choices.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Role {
    /// Security: after the player before all.
    #[default]
    Sentry,
    /// A citizen: itself first, then telling security.
    Citizen,
    /// A gang member: its quarrel first.
    Gang,
    /// One of a side, in a battle: the other side first.
    Fighter,
}

/// Its situation, by input, to choose from: as [`Situation`] reads it.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Reading {
    pub hunting: bool,
    pub foe: bool,
    pub shot_at: bool,
    pub investigating: bool,
    pub searching: bool,
    pub reporting: bool,
    pub sheltering: bool,
    pub mourning: bool,
    pub afraid: bool,
    pub watching: bool,
    pub avoiding: bool,
    pub regarding: bool,
    pub partnered: bool,
    pub nerve: f32,
    pub wariness: f32,
}

impl Situation<Input> for Reading {
    fn input(&self, input: Input) -> f32 {
        let yes = |b: bool| if b { 1.0 } else { 0.0 };
        match input {
            Input::Hunting => yes(self.hunting),
            Input::Foe => yes(self.foe),
            Input::ShotAt => yes(self.shot_at),
            Input::Investigating => yes(self.investigating),
            Input::Searching => yes(self.searching),
            Input::Reporting => yes(self.reporting),
            Input::Sheltering => yes(self.sheltering),
            Input::Mourning => yes(self.mourning),
            Input::Afraid => yes(self.afraid),
            Input::Watching => yes(self.watching),
            Input::Avoiding => yes(self.avoiding),
            Input::Regarding => yes(self.regarding),
            Input::Partnered => yes(self.partnered),
            Input::Nerve => self.nerve,
            Input::Wariness => self.wariness,
        }
    }
}

/// Only so, open to it.
const OPEN: Curve = Curve::Above { at: 0.5 };

/// How much more what it is doing already counts: a tenth.
pub const INERTIA: f32 = 0.1;

/// How an NPC of `role` chooses what to do.
pub fn reasoner(role: Role) -> Reasoner<Conduct, Input> {
    // Each conduct's worth at best, by role, in the order security has them; then what it turns on.
    let worth = |conduct: Conduct| -> f32 {
        use Conduct::*;
        let sentry = match conduct {
            Chase => 1.0,
            Fight => 0.95,
            ShootBack => 0.9,
            Investigate => 0.85,
            Search => 0.8,
            Report => 0.75,
            Shelter => 0.7,
            Mourn => 0.8,
            BackAway => 0.55,
            Watch => 0.5,
            Avoid => 0.45,
            Regard => 0.4,
            Follow => 0.2,
            GoAbout => 0.05,
        };
        match (role, conduct) {
            // A citizen: telling security, and getting out of the way, before the rest.
            (Role::Citizen, Report) => 0.97,
            (Role::Citizen, Shelter) => 0.96,
            (Role::Citizen, BackAway) => 0.7,
            // A gang member: its quarrel with the other side before the player.
            (Role::Gang, Fight) => 1.0,
            (Role::Gang, Chase) => 0.97,
            // One of a side: the other side first, and its own shots back.
            (Role::Fighter, Fight) => 1.0,
            (Role::Fighter, ShootBack) => 0.98,
            (Role::Fighter, Chase) => 0.96,
            _ => sentry,
        }
    };
    let open = |conduct: Conduct, input: Input| Action::new(conduct, worth(conduct)).when(input, OPEN);
    Reasoner::new(
        vec![
            open(Conduct::Chase, Input::Hunting),
            open(Conduct::Fight, Input::Foe),
            open(Conduct::ShootBack, Input::ShotAt),
            open(Conduct::Investigate, Input::Investigating),
            open(Conduct::Search, Input::Searching),
            open(Conduct::Report, Input::Reporting),
            open(Conduct::Shelter, Input::Sheltering),
            // The more nerve, the likelier it stays by one of its own; with little, it shelters
            // first, if there is shooting.
            open(Conduct::Mourn, Input::Mourning).when(Input::Nerve, Curve::Linear { from: -1.0, to: 1.0 }),
            open(Conduct::BackAway, Input::Afraid),
            open(Conduct::Watch, Input::Watching),
            open(Conduct::Avoid, Input::Avoiding),
            // The warier it is, the less it stops to look at anyone.
            open(Conduct::Regard, Input::Regarding).when(Input::Wariness, Curve::Linear { from: 1.0, to: 0.0 }),
            open(Conduct::Follow, Input::Partnered),
            Action::new(Conduct::GoAbout, worth(Conduct::GoAbout)),
        ],
        INERTIA,
    )
}

/// Reasoners for every role, to choose with.
#[derive(Debug, Clone, PartialEq)]
pub struct Reasoners {
    sentry: Reasoner<Conduct, Input>,
    citizen: Reasoner<Conduct, Input>,
    gang: Reasoner<Conduct, Input>,
    fighter: Reasoner<Conduct, Input>,
}

impl Default for Reasoners {
    fn default() -> Self {
        Self { sentry: reasoner(Role::Sentry), citizen: reasoner(Role::Citizen), gang: reasoner(Role::Gang), fighter: reasoner(Role::Fighter) }
    }
}

impl Reasoners {
    /// The reasoner for `role`, to change as a designer would.
    pub fn of(&mut self, role: Role) -> &mut Reasoner<Conduct, Input> {
        match role {
            Role::Sentry => &mut self.sentry,
            Role::Citizen => &mut self.citizen,
            Role::Gang => &mut self.gang,
            Role::Fighter => &mut self.fighter,
        }
    }

    /// What one of `role` in `reading`, doing `current` already, does: going about its business,
    /// with nothing else open to it.
    pub fn choose(&mut self, role: Role, reading: &Reading, current: Option<Conduct>) -> Conduct {
        self.of(role).choose(reading, current).unwrap_or(Conduct::GoAbout)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn choose(role: Role, reading: Reading) -> Conduct {
        Reasoners::default().choose(role, &reading, None)
    }

    #[test]
    fn only_what_is_open_is_chosen_and_with_nothing_it_goes_about_its_business() {
        assert_eq!(choose(Role::Sentry, Reading::default()), Conduct::GoAbout);
        assert_eq!(choose(Role::Sentry, Reading { partnered: true, ..Reading::default() }), Conduct::Follow);
        assert_eq!(choose(Role::Sentry, Reading { searching: true, partnered: true, ..Reading::default() }), Conduct::Search);
        assert_eq!(choose(Role::Sentry, Reading { hunting: true, searching: true, ..Reading::default() }), Conduct::Chase);
    }

    #[test]
    fn danger_comes_before_company_and_nerve_keeps_one_by_its_own() {
        let caught = Reading { sheltering: true, regarding: true, afraid: true, ..Reading::default() };
        assert_eq!(choose(Role::Citizen, caught), Conduct::Shelter, "not stopping to look, nor backing off");
        let grieving = |nerve: f32| Reading { sheltering: true, mourning: true, nerve, ..Reading::default() };
        assert_eq!(choose(Role::Sentry, grieving(0.2)), Conduct::Shelter, "little nerve: cover first");
        assert_eq!(choose(Role::Sentry, grieving(1.0)), Conduct::Mourn, "all of it: it stays by its own");
        assert_eq!(choose(Role::Citizen, grieving(1.0)), Conduct::Shelter, "a citizen thinks of itself first");
    }

    #[test]
    fn each_role_has_its_own_first_thought() {
        let both = Reading { hunting: true, foe: true, ..Reading::default() };
        assert_eq!(choose(Role::Sentry, both), Conduct::Chase);
        assert_eq!(choose(Role::Gang, both), Conduct::Fight);
        assert_eq!(choose(Role::Fighter, both), Conduct::Fight);
        let saw = Reading { reporting: true, investigating: true, ..Reading::default() };
        assert_eq!(choose(Role::Citizen, saw), Conduct::Report);
        assert_eq!(choose(Role::Sentry, saw), Conduct::Investigate);
    }

    #[test]
    fn the_warier_it_is_the_less_it_stops_to_look() {
        let passing = |wariness: f32| Reading { regarding: true, partnered: true, wariness, ..Reading::default() };
        assert_eq!(choose(Role::Citizen, passing(0.0)), Conduct::Regard);
        assert_eq!(choose(Role::Citizen, passing(1.0)), Conduct::Follow, "it keeps by its partner instead");
    }
}
