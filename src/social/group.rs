//! Walking about together: a pair, one keeping beside the other, or a group keeping about the one
//! leading it - who is in which ([`Groups`]), where each keeps about the one leading it
//! ([`about_leader`]), who speaks for it, and who leads it once the one that did is gone.

use crate::{flat, forward, navigation::grid::Rng, right_of};
use nalgebra::Vector3;

/// How far behind the one leading it one of its group can fall, in meters, before it waits for it.
pub const WAIT_FOR: f32 = 6.0;

/// An NPC's part in a pair or group: the one it keeps about - or, leading, one of those keeping
/// about it - whether it leads, and where about the one leading it it keeps (see
/// [`ABOUT_LEADER`]).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Membership {
    pub partner: Option<usize>,
    pub leads: bool,
    pub slot: usize,
}

/// Who walks about with whom: each NPC's [`Membership`], by the game's index for it.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Groups(Vec<Membership>);

impl Groups {
    /// No one, in no group.
    pub fn clear(&mut self) {
        self.0.clear();
    }

    /// Another NPC, by itself: its index.
    pub fn add(&mut self) -> usize {
        self.0.push(Membership::default());
        self.0.len() - 1
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The `n`th's part in its group, if it has one; by itself, none.
    pub fn get(&self, n: usize) -> Membership {
        self.0.get(n).copied().unwrap_or_default()
    }

    pub fn partner(&self, n: usize) -> Option<usize> {
        self.get(n).partner
    }

    pub fn leads(&self, n: usize) -> bool {
        self.get(n).leads
    }

    pub fn slot(&self, n: usize) -> usize {
        self.get(n).slot
    }

    /// Whether the `n`th walks about by itself: neither leading nor keeping about anyone.
    pub fn alone(&self, n: usize) -> bool {
        let m = self.get(n);
        !m.leads && m.partner.is_none()
    }

    /// Those leading a pair or group.
    pub fn leaders(&self) -> impl Iterator<Item = usize> + '_ {
        (0..self.0.len()).filter(|&n| self.0[n].leads)
    }

    /// The one the `n`th keeps about, if it keeps about one: not leading, but with a partner.
    pub fn leader_of(&self, n: usize) -> Option<usize> {
        let m = self.get(n);
        m.partner.filter(|_| !m.leads)
    }

    /// Whether the `m`th keeps about `leader`.
    fn keeps_about(&self, m: usize, leader: usize) -> bool {
        m != leader && !self.0[m].leads && self.0[m].partner == Some(leader)
    }

    /// The `n`th's group: the one leading it first, then those keeping about it - or the `n`th
    /// alone, walking about by itself.
    pub fn group_of(&self, n: usize) -> Vec<usize> {
        if n >= self.0.len() {
            return Vec::new();
        }
        let leader = if self.0[n].leads { n } else { self.0[n].partner.unwrap_or(n) };
        std::iter::once(leader)
            .chain((0..self.0.len()).filter(|&m| self.keeps_about(m, leader)))
            .collect()
    }

    /// Has `other` keep about `leader`, at `slot` (see [`ABOUT_LEADER`]): as a group is made.
    pub fn keep_about(&mut self, leader: usize, other: usize, slot: usize) {
        if self.0[leader].partner.is_none() {
            self.0[leader].partner = Some(other);
        }
        self.0[leader].leads = true;
        self.0[other] = Membership { partner: Some(leader), leads: false, slot };
    }

    /// Has each of `joining`, walking about by itself, keep about the `leader` - in the places
    /// about it not already kept by those of its group not `down`, as many as there are. Those
    /// that joined.
    pub fn join(&mut self, leader: usize, joining: &[usize], down: impl Fn(usize) -> bool) -> Vec<usize> {
        let mut kept: Vec<usize> = self.group_of(leader).into_iter().skip(1).filter(|&m| !down(m)).map(|m| self.0[m].slot).collect();
        let mut joined = Vec::new();
        for &m in joining {
            let Some(slot) = (0..ABOUT_LEADER.len()).find(|s| !kept.contains(s)) else {
                break;
            };
            kept.push(slot);
            self.0[m] = Membership { partner: Some(leader), leads: false, slot };
            let leader = &mut self.0[leader];
            leader.partner = leader.partner.or(Some(m));
            leader.leads = true;
            joined.push(m);
        }
        joined
    }

    /// Has the `n`th leave its pair or group, for good: another of those keeping about it leads
    /// the rest, if it led them.
    pub fn leave(&mut self, n: usize) {
        let Some(partner) = self.0.get_mut(n).and_then(|m| m.partner.take()) else {
            return;
        };
        if self.0[n].leads {
            let rest: Vec<usize> = (0..self.0.len()).filter(|&m| self.keeps_about(m, n)).collect();
            if let Some((&leader, others)) = rest.split_first() {
                self.0[leader] = Membership { partner: others.first().copied(), leads: !others.is_empty(), slot: 0 };
                for &m in others {
                    self.0[m].partner = Some(leader);
                }
            }
        } else if self.0[partner].partner == Some(n) {
            let next = (0..self.0.len()).find(|&m| self.keeps_about(m, partner));
            self.0[partner].partner = next;
            self.0[partner].leads = next.is_some();
        }
        self.0[n] = Membership::default();
    }

    /// Has the `n`th, the last of its group standing, walk about by itself.
    pub fn disband(&mut self, n: usize) {
        if let Some(m) = self.0.get_mut(n) {
            m.partner = None;
            m.leads = false;
        }
    }

    /// Whether the `n`th speaks for its pair or group: the first of them not `down` - the one
    /// leading it, unless that is down - or one walking about by itself.
    pub fn speaks_for(&self, n: usize, down: impl Fn(usize) -> bool) -> bool {
        self.group_of(n).into_iter().find(|&m| !down(m)) == Some(n)
    }
}

/// How many a group of a kind that walks about in groups of up to `most` is to be, picked by
/// `rng`: from two up to that, and no more than can keep about the one leading it (see
/// [`ABOUT_LEADER`]); one, for a kind that does not walk about in groups.
pub fn size(most: usize, rng: &mut Rng) -> usize {
    let most = most.min(ABOUT_LEADER.len() + 1);
    if most >= 2 {
        2 + rng.below(most - 1)
    } else {
        1
    }
}

/// Whether the one leading a group waits for one of it at `feet` - `about`, going about its
/// business, but `held` up, talking, say, or fallen [`WAIT_FOR`] behind the leader at `leader`.
pub fn waits_for(leader: Vector3<f32>, feet: Vector3<f32>, about: bool, held: bool) -> bool {
    about && (held || flat(feet - leader).norm() > WAIT_FOR)
}

/// A pair walking about together: how far to its partner's right the one keeping beside it walks,
/// and how far behind, in meters.
pub const BESIDE: (f32, f32) = (1.1, 0.3);

/// Where each of a group keeps about the one leading it, after the first: how far to its right,
/// in meters (to its left, less than none), and how far behind. The first of them is a pair's.
pub const ABOUT_LEADER: [(f32, f32); 5] = [
    (BESIDE.0, BESIDE.1),
    (-BESIDE.0, BESIDE.1),
    (0.0, 1.5),
    (BESIDE.0, 1.9),
    (-BESIDE.0, 1.9),
];

/// Where the one keeping to `slot` of [`ABOUT_LEADER`] stands, about a leader at `feet` facing
/// `heading`.
pub fn about_leader(feet: Vector3<f32>, heading: f32, slot: usize) -> Vector3<f32> {
    let (right, behind) = ABOUT_LEADER[slot.min(ABOUT_LEADER.len() - 1)];
    feet + right_of(heading) * right - forward(heading) * behind
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn its_right_is_to_the_right() {
        // Facing +z, the right is -x; facing +x, it is +z.
        assert!((right_of(0.0) - Vector3::new(-1.0, 0.0, 0.0)).norm() < 1.0e-6);
        assert!((right_of(std::f32::consts::FRAC_PI_2) - Vector3::new(0.0, 0.0, 1.0)).norm() < 1.0e-6);
        assert!(right_of(0.7).dot(&forward(0.7)).abs() < 1.0e-6);
    }

    #[test]
    fn a_pair_walks_on_the_right_a_step_behind_and_a_group_fans_out_behind() {
        let at = |slot| about_leader(Vector3::zeros(), 0.0, slot);
        // Facing +z, the right is -x.
        assert!(at(0).x < 0.0 && at(0).z < 0.0, "{:?}", at(0));
        assert!(at(1).x > 0.0, "the next on the left");
        assert!((0..ABOUT_LEADER.len()).all(|slot| at(slot).z < 0.0), "all behind");
        assert_eq!(at(9), at(ABOUT_LEADER.len() - 1), "more keep to the last");
    }

    #[test]
    fn a_group_is_made_joined_left_and_led_on_by_another() {
        let mut groups = Groups::default();
        for _ in 0..5 {
            groups.add();
        }
        groups.keep_about(0, 1, 0);
        groups.keep_about(0, 2, 1);
        assert_eq!(groups.group_of(2), vec![0, 1, 2]);
        assert!(groups.leads(0) && groups.alone(3));
        assert!(groups.speaks_for(0, |_| false) && !groups.speaks_for(1, |_| false));
        assert!(groups.speaks_for(1, |m| m == 0), "the leader down, the next speaks for it");
        // Joining, it takes the first place about the leader not kept.
        assert_eq!(groups.join(0, &[3], |_| false), vec![3]);
        assert_eq!(groups.slot(3), 2);
        // The leader leaves: the first of the rest leads them.
        groups.leave(0);
        assert!(groups.alone(0));
        assert_eq!(groups.group_of(2), vec![1, 2, 3]);
        assert!(groups.leads(1) && groups.slot(1) == 0);
        // One keeping about it leaves; the last two keep on as a pair, then the last alone.
        groups.leave(3);
        groups.leave(2);
        assert_eq!(groups.group_of(1), vec![1]);
        assert!(!groups.leads(1));
    }

    #[test]
    fn a_groups_size_is_two_up_to_its_most_and_waiting_is_for_one_held_or_behind() {
        let mut rng = Rng::new(2);
        assert!((0..50).all(|_| (2..=4).contains(&size(4, &mut rng))));
        assert_eq!(size(1, &mut rng), 1);
        assert!((0..50).all(|_| size(40, &mut rng) <= ABOUT_LEADER.len() + 1));
        let leader = Vector3::zeros();
        assert!(waits_for(leader, Vector3::new(WAIT_FOR + 1.0, 0.0, 0.0), true, false));
        assert!(waits_for(leader, Vector3::new(1.0, 0.0, 0.0), true, true));
        assert!(!waits_for(leader, Vector3::new(1.0, 0.0, 0.0), true, false));
        assert!(!waits_for(leader, Vector3::new(WAIT_FOR + 1.0, 0.0, 0.0), false, true));
    }
}
