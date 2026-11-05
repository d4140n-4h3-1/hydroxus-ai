//! Perception: what an NPC makes out of what is round it - what it sees, what it hears, the bolts
//! it notices going by, and what it can tell of a shooting it saw.
//!
//! None of it casts a ray. Whether anything is in the way is the game's to find out; this says
//! whether, nothing being in the way, the NPC would take it in.
//!
//! - [`sight`]: clearly in front, near out of the corner of its eye, nothing behind.
//! - [`detection`]: becoming aware of someone gradually, as Fallout's sneaking has it.
//! - [`hearing`]: how far a noise carries, along the corridors rather than through the walls.
//! - [`bolts`]: a bolt flying by, noticed in front of it or felt close by anywhere.
//! - [`witness`]: whether one that saw a shooting knows who did it.

pub mod bolts;
pub mod detection;
pub mod hearing;
pub mod sight;
pub mod witness;
