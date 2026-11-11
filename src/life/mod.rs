//! Life: an NPC's everyday life, when no one is after anyone - where it goes and what it does with
//! its day. Where [`crate::tactics`] is what it does about trouble, and [`crate::social`] what it
//! makes of others, this is what it would be doing anyway: up and off to work, out to the shops,
//! home again, a while in the park - so that a city goes on about its business round the player,
//! and an NPC that drops everything to give chase or run has something to go back to.
//!
//! As everywhere in the crate, what the game's NPCs do and where is the game's own: an activity is
//! the game's kind, a place its own number or spot. This only keeps time.
//!
//! - [`routine`]: an NPC's day, by the hour - what it does from when - each NPC a little early or
//!   late, so that a street of them does not all set off on the stroke of the hour.
//!
//! To come: what an NPC needs as its day wears on - rest, supplies, company - and the errands those
//! send it on: to a shop, a vending machine, a friend.

pub mod routine;
