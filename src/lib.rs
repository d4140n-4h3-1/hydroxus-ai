//! AI for Hydroxus games: NPCs that get about a level, see what is in front of them, hear what
//! carries to them, and hunt the player through Metal Gear's alert phases.
//!
//! It is in three parts, and a few things besides:
//!
//! - [`perception`]: what an NPC takes in - what it sees ([`perception::sight`]), hears
//!   ([`perception::hearing`]), the bolts it notices flying by ([`perception::bolts`]), and what
//!   it can tell of a shooting it saw ([`perception::witness`]).
//! - [`tactics`]: what it does about it - searching for someone lost ([`tactics::search`]),
//!   joining a chase ([`tactics::join`]), pacing itself ([`tactics::pace`]), cutting off and
//!   running ([`tactics::chase`]), answering a weapon pointed at it ([`tactics::threat`]),
//!   shooting ([`tactics::fire`]) and firing blind ([`tactics::blind`]), and taking cover
//!   ([`tactics::cover`]); and fighting as a team ([`tactics::team`]).
//! - [`navigation`]: getting about - the walk grid ([`navigation::grid`]) and what can be got to
//!   on it ([`navigation::reach`]), routes over it ([`navigation::route`]), going about one's
//!   business in a city ([`navigation::wander`]), over floors and by ferry
//!   ([`navigation::trip`]), by train ([`navigation::ride`]), making way for others
//!   ([`navigation::steer`]), and driving ([`navigation::drive`]).
//! - [`alert`]: Metal Gear's phases - Alert, Evasion, Caution - and how one leads to the next,
//!   which what an NPC perceives and what it does both turn on.
//! - [`group`]: walking about together, a pair or a group about the one leading it.
//! - [`crowd`]: a city's crowd, always about the player, turning up again out of sight.
//! - [`talk`]: who the player can talk to.
//!
//! The crate never touches a scene. The game casts the rays that say whether anything is in the
//! way, moves the nodes and plays the animations; this decides what an NPC makes of what is
//! round it. Its vectors are nalgebra's, as the engine's are.
//!
//! Directions along the ground are headings, in radians, left positive from the world's +z.

pub mod alert;
pub mod crowd;
pub mod group;
pub mod navigation;
pub mod perception;
pub mod tactics;
pub mod talk;

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

/// Which way is to the right of one facing `heading`, along the ground: facing +z, -x.
pub fn right_of(heading: f32) -> Vector3<f32> {
    Vector3::new(-heading.cos(), 0.0, heading.sin())
}

/// The heading of `way`, along the ground; none if it goes nowhere along it.
pub fn heading_of(way: Vector3<f32>) -> Option<f32> {
    (flat(way).norm() > 1.0e-3).then(|| way.x.atan2(way.z))
}

/// Everything a game usually needs, in one import.
pub mod prelude {
    pub use crate::{
        alert::{Alert, CAUTION, EVASION},
        flat, forward, right_of,
        heading_of,
        navigation::{
            grid::{Routes, Rng, WalkGrid},
            route::{between, plan, plan_avoiding, route_to, route_to_weighted, spot_near},
            steer::{make_way, step_aside},
        },
        perception::{
            hearing::Heard,
            sight::{Sight, Stance},
        },
        tactics::{search::SearchMap, threat::Threat},
    };
}
