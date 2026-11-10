//! Tactics: what an NPC does about what it has taken in - hunting someone down, looking for them
//! once lost, joining in, keeping its breath, answering a weapon pointed at it, shooting back,
//! running and taking cover.
//!
//! - [`search`]: where someone lost from sight could be by now, and who looks where for them.
//! - [`join`]: one of a kind that hunts joining in a chase it sees another of its kind in.
//! - [`pace`]: sprinting now and then in a chase, and keeping breath for a last burst.
//! - [`chase`]: cutting someone off rather than running after them, and running from a pursuer.
//! - [`threat`]: a weapon pointed at an NPC: its warnings, then its doing something about it.
//! - [`investigate`]: going to look where something was noticed, and looking about there.
//! - [`look`]: looking about where it is, one way and then the other - searching or investigating.
//! - [`fire`]: when and how wide an armed NPC shoots: from how far, how often, facing how nearly.
//! - [`blind`]: shot at by someone it cannot see, firing blind back where the shots came from.
//! - [`cover`]: a bystander running from a shooting, to somewhere it cannot be seen from.
//! - [`team`]: fighting as a team: shared sightings, targets, cover, flanking, covering fire.
//! - [`patrol`]: a flying sentry's patrol, search and Alert, and its flight along its route.
//! - [`priority`]: weighing what to do by urgency, threat, confidence, commitment and traits.
//! - [`utility`]: utility scoring - weighing what an NPC could do against what it takes in.
//! - [`conduct`]: what an NPC on foot does next, by utility, by its role: chasing, searching,
//!   telling a sentry, sheltering, mourning, keeping away, following, going about its business.

pub mod blind;
pub mod chase;
pub mod conduct;
pub mod cover;
pub mod fire;
pub mod investigate;
pub mod join;
pub mod look;
pub mod pace;
pub mod patrol;
pub mod priority;
pub mod search;
pub mod team;
pub mod threat;
pub mod utility;
