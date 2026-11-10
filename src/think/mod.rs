//! Think: what an NPC learns, and how it comes to do things differently for it - what has worked
//! for it and what has not, what the player tends to do, and where the player keeps turning up.
//!
//! Nothing here chooses or acts by itself. Each part keeps what has been learned, fading with
//! time so that the player can change their ways and be met afresh, and weighs what the rest of
//! the crate already does: a [`crate::tactics::utility::Reasoner`]'s choice, a
//! [`crate::tactics::search::SearchMap`]'s guess, or one of a game's own considerations. What is
//! learned can be told on, weaker for being told, so that a squad or a faction learns as one -
//! and, with the `serde` feature, kept from one game to the next.
//!
//! - [`lesson`]: what has worked: each thing an NPC could do, weighed up or down by how it turned
//!   out the times it did it.
//! - [`habit`]: the player's habits: how often they do each of the things a game tells apart - the
//!   way they come in, the trick they use - and how sure an NPC can be of what they will do next.
//! - [`haunt`]: where the player keeps turning up: the places they were seen and found, for a
//!   search to look first and a patrol to go by.
//! - [`memory`]: what one NPC remembers having happened, one record each - places, characters,
//!   encounters, events, dangers, groups, investigations - fading by kind, and told on.
//! - [`experience`]: what an NPC has been through: one place for a game to say how what it did
//!   turned out, taken to its lessons, the player's habits and haunts, and weighing what it does.

pub mod experience;
pub mod habit;
pub mod haunt;
pub mod lesson;
pub mod memory;

/// How much of `had` is left after `dt` seconds, halving every `half_life`.
fn fade(had: f32, half_life: f32, dt: f32) -> f32 {
    had * 0.5f32.powf(dt.max(0.0) / half_life.max(1.0e-3))
}
