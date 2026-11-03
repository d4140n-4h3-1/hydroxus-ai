//! Navigation: getting about - over the walkable ground and up and down its floors, by ferry and
//! by train, round others in the way, and at the controls of a vehicle.
//!
//! - [`grid`]: the level's walkable ground as a grid of cells, and the cheapest ways across it.
//! - [`navmesh`]: the walkable floor as convex polygons built from the grid, routes over them
//!   pulled taut, and what can be reached from somewhere.
//! - [`reach`]: which cells can be got to at all, a bit each.
//! - [`route`]: routes over that ground, as points on the floor - to somewhere, or a trip away.
//! - [`wander`]: going about one's business in a city, off the roads and the gangs' turf.
//! - [`trip`]: getting about over more than one floor, and across by ferry where that is quicker.
//! - [`ride`]: taking a train from one station to another.
//! - [`steer`]: making way for others in a corridor, walking or standing about.
//! - [`drive`]: riding a vehicle round a course, and through traffic.

pub mod drive;
pub mod grid;
pub mod navmesh;
pub mod reach;
pub mod ride;
pub mod route;
pub mod steer;
pub mod trip;
pub mod wander;
