//! AI for Hydroxus games: NPCs that get about a level, see what is in front of them, hear what
//! carries to them, and hunt the player through Metal Gear's alert phases.
//!
//! It is in four parts, and its alert phases:
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
//! - [`social`]: NPCs among each other - walking about in pairs and groups
//!   ([`social::group`]), meeting and talking and being talked to ([`social::talk`]), telling a
//!   sentry of trouble and sounding the alarm ([`social::report`]), and a city's crowd
//!   ([`social::crowd`]).
//! - [`alert`]: Metal Gear's phases - Alert, Evasion, Caution - and how one leads to the next,
//!   which what an NPC perceives and what it does both turn on.
//!
//! The crate never touches a scene. The game casts the rays that say whether anything is in the
//! way, moves the nodes and plays the animations; this decides what an NPC makes of what is
//! round it. Its vectors are nalgebra's, as the engine's are.
//!
//! Directions along the ground are headings, in radians, left positive from the world's +z.
//!
//! The numbers each module's behaviour turns on are its `Tuning`, gathered in [`Tuning`] - Maze's
//! by default, a game's own if it likes, and with the `serde` feature read from a file. What came
//! of what an NPC did is an event for the game to say, play or show as it likes (a sentry's
//! [`tactics::patrol::Event`], say), never a line of the crate's choosing.

/// A module's tuning: the numbers its behaviour turns on, as fields, with `DEFAULT` - and
/// [`Default`] - the values the module documents, Maze's. With the `serde` feature, it can be read
/// from a file, any field left out taking its default.
macro_rules! tuning {
    ($(#[$meta:meta])* { $($(#[$field_meta:meta])* $field:ident: $ty:ty = $default:expr),* $(,)? }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq)]
        #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
        #[cfg_attr(feature = "serde", serde(default))]
        pub struct Tuning {
            $($(#[$field_meta])* pub $field: $ty,)*
        }

        impl Tuning {
            /// The module's own numbers.
            pub const DEFAULT: Self = Self { $($field: $default,)* };
        }

        impl Default for Tuning {
            fn default() -> Self {
                Self::DEFAULT
            }
        }
    };
}

pub mod alert;
pub mod navigation;
pub mod perception;
pub mod social;
pub mod tactics;

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

/// Every module's tuning in one: the numbers the crate's behaviour turns on, each module's as its
/// own `Tuning` documents them. [`Tuning::DEFAULT`] - and [`Default`] - are the modules' own,
/// Maze's. A game keeps one, changes what it likes, and hands each module its part; with the
/// `serde` feature it can read it from a file, anything left out taking its default.
///
/// Those that keep their tuning with them take it as they are made: a [`WalkGrid`]'s ground
/// ([`WalkGrid::with_tuning`]), a [`NavMesh`]'s making and walking ([`NavMesh::build_with`],
/// [`NavMesh::walk`]), a traffic [`Network`]'s ([`Network::from_roads_with`]), a sentry's
/// [`Patrol::tuning`], a [`SearchMap`]'s ([`SearchMap::with`]), and so on.
///
/// [`WalkGrid`]: navigation::grid::WalkGrid
/// [`WalkGrid::with_tuning`]: navigation::grid::WalkGrid::with_tuning
/// [`NavMesh`]: navigation::navmesh::NavMesh
/// [`NavMesh::build_with`]: navigation::navmesh::NavMesh::build_with
/// [`NavMesh::walk`]: navigation::navmesh::NavMesh::walk
/// [`Network`]: navigation::traffic::Network
/// [`Network::from_roads_with`]: navigation::traffic::Network::from_roads_with
/// [`Patrol::tuning`]: tactics::patrol::Patrol::tuning
/// [`SearchMap`]: tactics::search::SearchMap
/// [`SearchMap::with`]: tactics::search::SearchMap::with
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(default))]
pub struct Tuning {
    /// Metal Gear's alert phases: how long each lasts.
    pub alert: crate::alert::Tuning,
    /// What an NPC sees.
    pub sight: crate::perception::sight::Sight,
    /// Bolts flying by, felt and noticed.
    pub bolts: crate::perception::bolts::Tuning,
    /// What a witness can tell of a shooting.
    pub witness: crate::perception::witness::Tuning,
    /// Where someone lost could be.
    pub search: crate::tactics::search::Tuning,
    /// Pacing a chase.
    pub pace: crate::tactics::pace::Tuning,
    /// Cutting off and running.
    pub chase: crate::tactics::chase::Tuning,
    /// A weapon pointed at an NPC.
    pub threat: crate::tactics::threat::Tuning,
    /// Shooting.
    pub fire: crate::tactics::fire::Tuning,
    /// Firing blind.
    pub blind: crate::tactics::blind::Tuning,
    /// Taking cover.
    pub cover: crate::tactics::cover::Tuning,
    /// Fighting as a team.
    pub team: crate::tactics::team::Tuning,
    /// A flying sentry's patrol.
    pub patrol: crate::tactics::patrol::Tuning,
    /// The ground as a walker takes it.
    pub grid: crate::navigation::grid::Tuning,
    /// How a navmesh is made.
    pub navmesh: crate::navigation::navmesh::Tuning,
    /// How a navmesh is walked.
    pub walk: crate::navigation::walk::Tuning,
    /// Going about one's business in a city.
    pub wander: crate::navigation::wander::Tuning,
    /// Making way for others.
    pub steer: crate::navigation::steer::Tuning,
    /// Taking a ferry.
    pub trip: crate::navigation::trip::Tuning,
    /// Taking a train.
    pub ride: crate::navigation::ride::Tuning,
    /// Getting by others, and stopping for them, at the controls.
    pub drive: crate::navigation::drive::Tuning,
    /// A city's streets and traffic.
    pub traffic: crate::navigation::traffic::Tuning,
    /// Flying free.
    pub flight: crate::navigation::flight::Tuning,
    /// Keeping together in a group.
    pub group: crate::social::group::Tuning,
    /// Talking, and how far it carries.
    pub talk: crate::social::talk::Tuning,
    /// Telling a sentry, and hearing the alarm.
    pub report: crate::social::report::Tuning,
    /// A city's crowd.
    pub crowd: crate::social::crowd::Tuning,
    /// Regard for the player.
    pub opinion: crate::social::opinion::Tuning,
    /// Rumours of the player.
    pub rumour: crate::social::rumour::Tuning,
    /// What NPCs talk about.
    pub chat: crate::social::chat::Tuning,
}

impl Tuning {
    /// Every module's own numbers.
    pub const DEFAULT: Self = Self {
        alert: crate::alert::Tuning::DEFAULT,
        sight: crate::perception::sight::Sight::DEFAULT,
        bolts: crate::perception::bolts::Tuning::DEFAULT,
        witness: crate::perception::witness::Tuning::DEFAULT,
        search: crate::tactics::search::Tuning::DEFAULT,
        pace: crate::tactics::pace::Tuning::DEFAULT,
        chase: crate::tactics::chase::Tuning::DEFAULT,
        threat: crate::tactics::threat::Tuning::DEFAULT,
        fire: crate::tactics::fire::Tuning::DEFAULT,
        blind: crate::tactics::blind::Tuning::DEFAULT,
        cover: crate::tactics::cover::Tuning::DEFAULT,
        team: crate::tactics::team::Tuning::DEFAULT,
        patrol: crate::tactics::patrol::Tuning::DEFAULT,
        grid: crate::navigation::grid::Tuning::DEFAULT,
        navmesh: crate::navigation::navmesh::Tuning::DEFAULT,
        walk: crate::navigation::walk::Tuning::DEFAULT,
        wander: crate::navigation::wander::Tuning::DEFAULT,
        steer: crate::navigation::steer::Tuning::DEFAULT,
        trip: crate::navigation::trip::Tuning::DEFAULT,
        ride: crate::navigation::ride::Tuning::DEFAULT,
        drive: crate::navigation::drive::Tuning::DEFAULT,
        traffic: crate::navigation::traffic::Tuning::DEFAULT,
        flight: crate::navigation::flight::Tuning::DEFAULT,
        group: crate::social::group::Tuning::DEFAULT,
        talk: crate::social::talk::Tuning::DEFAULT,
        report: crate::social::report::Tuning::DEFAULT,
        crowd: crate::social::crowd::Tuning::DEFAULT,
        opinion: crate::social::opinion::Tuning::DEFAULT,
        rumour: crate::social::rumour::Tuning::DEFAULT,
        chat: crate::social::chat::Tuning::DEFAULT,
    };
}

impl Default for Tuning {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Everything a game usually needs, in one import.
pub mod prelude {
    pub use crate::{
        alert::{Alert, CAUTION, EVASION},
        flat, forward, right_of,
        heading_of,
        navigation::{
            grid::{Rng, WalkGrid},
            navmesh::NavMesh,
            route::between,
            walk::{plan, plan_avoiding, route_to, route_to_weighted, spot_near},
            steer::{make_way, step_aside},
        },
        perception::{
            hearing::Heard,
            sight::{Sight, Stance},
        },
        tactics::{search::SearchMap, threat::Threat},
    };
}

#[cfg(all(test, feature = "serde"))]
mod tests {
    use super::*;

    #[test]
    fn a_tuning_read_from_a_file_takes_the_defaults_for_what_it_leaves_out() {
        let tuning: Tuning = serde_json::from_str(r#"{ "patrol": { "engage": 15.0 }, "alert": { "caution": 90.0 } }"#).unwrap();
        assert_eq!(tuning.patrol.engage, 15.0);
        assert_eq!(tuning.alert.caution, 90.0);
        assert_eq!(tuning.patrol.search, tactics::patrol::SEARCH);
        assert_eq!(tuning.alert.evasion, alert::EVASION);
        assert_eq!(tuning.sight, perception::sight::Sight::DEFAULT);
        let back: Tuning = serde_json::from_str(&serde_json::to_string(&tuning).unwrap()).unwrap();
        assert_eq!(back, tuning);
    }
}
