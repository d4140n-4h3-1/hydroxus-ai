//! Picking between bounds. Routes themselves are the navmesh's (see
//! [`crate::navigation::walk`]); a route is kept the way an NPC follows it: the first point last,
//! so that the next one to make for is `route.last()`, and reaching it is `route.pop()`.

use crate::navigation::grid::Rng;

/// A number from `range.0` to `range.1`.
pub fn between(rng: &mut Rng, range: (f32, f32)) -> f32 {
    range.0 + (range.1 - range.0) * rng.below(1001) as f32 / 1000.0
}
