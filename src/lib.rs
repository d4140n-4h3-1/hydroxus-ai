//! Stealth AI for Hydroxus games: NPCs that walk a level, see what is in front of them, hear
//! what carries to them, and hunt the player through Metal Gear's alert phases.
//!
//! - [`grid`]: the level's walkable ground as a grid of cells, and the cheapest ways across it.
//! - [`route`]: routes over that ground, as points on the floor - to somewhere, or a trip away.
//! - [`sight`]: what an NPC can see. It sees only in front of it, whatever it is doing - on Alert
//!   too, and right next to it too - so it can be crept up on from behind, or slipped round.
//! - [`hearing`]: how far a noise carries, along the corridors rather than through the walls.
//! - [`alert`]: Metal Gear's phases - Alert, Evasion, Caution - and how one leads to the next.
//! - [`steer`]: making way for others in a corridor, walking or standing about.
//! - [`search`]: where someone lost from sight could be by now, and who looks where for them.
//!
//! The crate never touches a scene. The game casts the rays that say whether anything is in the
//! way, moves the nodes and plays the animations; this decides what an NPC makes of what is
//! round it. Its vectors are nalgebra's, as the engine's are.
//!
//! Directions along the ground are headings, in radians, left positive from the world's +z.

pub mod alert;
pub mod grid;
pub mod hearing;
pub mod route;
pub mod search;
pub mod sight;
pub mod steer;

pub use nalgebra;

use nalgebra::Vector3;

/// `vector` along the ground.
pub fn flat(vector: Vector3<f32>) -> Vector3<f32> {
    Vector3::new(vector.x, 0.0, vector.z)
}

/// Which way `heading` faces, along the ground.
pub fn forward(heading: f32) -> Vector3<f32> {
    Vector3::new(heading.sin(), 0.0, heading.cos())
}

/// The heading of `way`, along the ground; none if it goes nowhere along it.
pub fn heading_of(way: Vector3<f32>) -> Option<f32> {
    (flat(way).norm() > 1.0e-3).then(|| way.x.atan2(way.z))
}

/// Everything a game usually needs, in one import.
pub mod prelude {
    pub use crate::{
        alert::{Alert, CAUTION, EVASION},
        flat, forward,
        grid::{Routes, Rng, WalkGrid},
        heading_of,
        hearing::Heard,
        route::{between, plan, route_to, route_to_weighted},
        search::SearchMap,
        sight::{Sight, Stance},
        steer::{make_way, step_aside},
    };
}
