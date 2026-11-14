//! Keeping a crowd spread out: NPCs going the same way, or after the same one, without bunching
//! into a clump.
//!
//! Three parts, each for the game to use as it likes:
//! - [`separation`]: a push away from whoever is inside an NPC's own space ([`Tuning::personal`]),
//!   the nearer the harder, for it to move by as it walks or stands - each keeps a little room.
//! - [`lane`]: where among many going to the same place one of them makes for - its own lane,
//!   side by side across the way they are coming from ([`Tuning::lane`] apart, no wider than
//!   [`Tuning::widest`]) - so that they arrive as a line, not a knot.
//! - [`share_out`]: which of the ones it knows of each of a team goes after - the nearest, unless
//!   [`Tuning::most_after`] of its team are after that one already - so that a big team does not all
//!   chase the one enemy.

use crate::{flat, navigation::steer::right_of};
use nalgebra::Vector3;

tuning! {
    /// How a crowd keeps spread out.
    {
        /// How much room an NPC keeps round it, in meters, and how fast at most it is pushed
        /// out of someone else's, in meters a second.
        personal: f32 = 2.5,
        push: f32 = 1.5,
        /// How far apart, in meters, lanes to the same place are, and how wide all of them may be.
        lane: f32 = 3.5,
        widest: f32 = 30.0,
        /// How many of a team at most go after the same one.
        most_after: u32 = 4,
    }
}

/// Which way, and how fast, in meters a second, an NPC at `feet` is pushed out of the space of
/// `others` - where each is - along the ground: away from each inside [`Tuning::personal`], the
/// nearer the harder, no faster than [`Tuning::push`] in all. Zero with room round it. Two at the
/// very same spot are pushed apart by `tiebreak`, a number of the NPC's own, so that they go
/// different ways.
pub fn separation(tuning: &Tuning, feet: Vector3<f32>, others: impl Iterator<Item = Vector3<f32>>, tiebreak: u32) -> Vector3<f32> {
    let mut push = Vector3::zeros();
    for there in others {
        let away = flat(feet - there);
        let distance = away.norm();
        if distance >= tuning.personal {
            continue;
        }
        let way = if distance > 1.0e-3 {
            away / distance
        } else {
            let angle = (tiebreak as f32) * 2.399_963; // the golden angle, so ties spread round
            Vector3::new(angle.sin(), 0.0, angle.cos())
        };
        push += way * (1.0 - distance / tuning.personal);
    }
    let strength = push.norm();
    if strength <= 1.0e-6 {
        return Vector3::zeros();
    }
    push / strength * tuning.push * strength.min(1.0)
}

/// Where one of `of` NPCs going from about `from` to `goal` makes for, the `slot`th of them
/// (from 0): its own lane, side by side with the others across the way they come,
/// [`Tuning::lane`] apart, the whole no wider than [`Tuning::widest`]. `goal` itself for one alone.
pub fn lane(tuning: &Tuning, goal: Vector3<f32>, from: Vector3<f32>, slot: usize, of: usize) -> Vector3<f32> {
    if of <= 1 {
        return goal;
    }
    let Some(ahead) = flat(goal - from).try_normalize(1.0e-3) else {
        return goal;
    };
    let width = (tuning.lane * (of - 1) as f32).min(tuning.widest);
    let across = width * ((slot % of) as f32 / (of - 1) as f32 - 0.5);
    goal + right_of(ahead) * across
}

/// Which of `known` - each by a number of its own and how far it is - one of a team goes after:
/// the nearest that fewer than [`Tuning::most_after`] of its team are after, by `after(n)`; or,
/// with every one of them taken, the nearest of all. None with none known.
pub fn share_out(tuning: &Tuning, known: impl Iterator<Item = (usize, f32)>, after: impl Fn(usize) -> u32) -> Option<usize> {
    let mut nearest: Option<(usize, f32)> = None;
    let mut free: Option<(usize, f32)> = None;
    for (n, distance) in known {
        if nearest.is_none_or(|(_, d)| distance < d) {
            nearest = Some((n, distance));
        }
        if after(n) < tuning.most_after && free.is_none_or(|(_, d)| distance < d) {
            free = Some((n, distance));
        }
    }
    free.or(nearest).map(|(n, _)| n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_is_pushed_out_of_others_space_the_nearer_the_harder() {
        let t = Tuning::DEFAULT;
        let me = Vector3::zeros();
        assert_eq!(separation(&t, me, [Vector3::new(5.0, 0.0, 0.0)].into_iter(), 0), Vector3::zeros(), "room round it");
        let near = separation(&t, me, [Vector3::new(0.4, 0.0, 0.0)].into_iter(), 0);
        let nearer = separation(&t, me, [Vector3::new(0.2, 0.0, 0.0)].into_iter(), 0);
        assert!(near.x < 0.0 && near.z.abs() < 1.0e-6, "away from them: {near:?}");
        assert!(nearer.norm() > near.norm());
        assert!(nearer.norm() <= t.push + 1.0e-6);
        // Hemmed in on both sides alike, it is not pushed either way.
        let between = separation(&t, me, [Vector3::new(0.5, 0.0, 0.0), Vector3::new(-0.5, 0.0, 0.0)].into_iter(), 0);
        assert!(between.norm() < 1.0e-5);
        // Two on the very same spot go different ways.
        let (a, b) = (separation(&t, me, [me].into_iter(), 1), separation(&t, me, [me].into_iter(), 2));
        assert!(a.norm() > 0.0 && (a - b).norm() > 0.1);
    }

    #[test]
    fn many_going_to_one_place_make_for_lanes_side_by_side() {
        let t = Tuning::DEFAULT;
        let (from, goal) = (Vector3::new(0.0, 0.0, -50.0), Vector3::zeros());
        assert_eq!(lane(&t, goal, from, 0, 1), goal);
        let lanes: Vec<Vector3<f32>> = (0..5).map(|k| lane(&t, goal, from, k, 5)).collect();
        // Across the way they come (along z), lane apart, centred on the goal.
        assert!(lanes.iter().all(|p| p.z.abs() < 1.0e-5));
        assert!((lanes[1].x - lanes[0].x).abs() - t.lane < 1.0e-4);
        assert!(lanes.iter().map(|p| p.x).sum::<f32>().abs() < 1.0e-3);
        // Fifty no wider than the widest.
        let wide: Vec<f32> = (0..50).map(|k| lane(&t, goal, from, k, 50).x).collect();
        let span = wide.iter().cloned().fold(f32::MIN, f32::max) - wide.iter().cloned().fold(f32::MAX, f32::min);
        assert!((span - t.widest).abs() < 1.0e-3, "{span}");
    }

    #[test]
    fn a_team_shares_out_who_goes_after_whom() {
        let t = Tuning::DEFAULT;
        let known = [(7, 10.0), (8, 20.0), (9, 30.0)];
        assert_eq!(share_out(&t, known.into_iter(), |_| 0), Some(7), "the nearest");
        assert_eq!(share_out(&t, known.into_iter(), |n| if n == 7 { t.most_after } else { 0 }), Some(8), "the nearest is taken");
        assert_eq!(share_out(&t, known.into_iter(), |_| t.most_after), Some(7), "all taken: the nearest anyway");
        assert_eq!(share_out(&t, std::iter::empty(), |_| 0), None);
    }
}
