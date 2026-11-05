//! Navigation: getting about - over the walkable ground and up and down its floors, by ferry and
//! by train, round others in the way, and at the controls of a vehicle.
//!
//! - [`flight`]: flying free, through the air at any height, rather than along the floor.
//! - [`grid`]: what a survey of a level finds, as a grid of cells: what the navmesh is made from.
//! - [`navmesh`]: the walkable floor as convex polygons built from the grid - all that is kept of
//!   the ground once it is made - routes over them pulled taut, and doors opened and shut on it.
//! - [`reach`]: which polygons can be got to at all, a bit each.
//! - [`route`]: picking between bounds; how a route is kept.
//! - [`wander`]: going about one's business in a city, off the roads and the gangs' turf.
//! - [`walk`]: getting about over the navmesh - routes, trips, spots near, the floor at a height.
//! - [`trip`]: getting about over more than one floor, and across by ferry where that is quicker.
//! - [`ride`]: taking a train from one station to another.
//! - [`steer`]: making way for others in a corridor, walking or standing about.
//! - [`drive`]: riding a vehicle round a course, and through traffic.
//! - [`traffic`]: a city's streets, their stop lights, and the bikes going about them.

pub mod drive;
pub mod flight;
pub mod grid;
pub mod navmesh;
pub mod reach;
pub mod ride;
pub mod route;
pub mod steer;
pub mod traffic;
pub mod trip;
pub mod walk;
pub mod wander;
