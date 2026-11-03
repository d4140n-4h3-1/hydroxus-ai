//! Rumours: what NPCs have seen the player do, and told each other. Each NPC has a mind
//! ([`Minds`]) - its regard for the player (see [`crate::social::opinion`]) and what it knows of
//! them. Seeing a deed done, it knows it at full strength and its regard moves the deed's full
//! weight; told it by another, at [`RETELL`] of the teller's strength, its regard moving
//! [`HEARSAY`] of that. Rumours fade ([`FADE_HALF_LIFE`]); one too faint to be worth telling
//! ([`TELLABLE`]) is not told on, and one fainter still ([`FORGOTTEN`]) is forgotten. An NPC that
//! turns up afresh - one of a city's crowd, say - has heard what the town is talking about, at
//! [`NEWS`] of its strength.

use crate::social::opinion::{attitude, Attitude, Deed, REGARD_HALF_LIFE};
use nalgebra::Vector3;

/// How strong a rumour is once told on, of the teller's; how much a rumour moves a hearer's
/// regard, of what seeing the deed would; and how strong the town's talk is to one turning up.
pub const RETELL: f32 = 0.7;
pub const HEARSAY: f32 = 0.6;
pub const NEWS: f32 = 0.5;
/// How strong a rumour has to be to be told on, and under which it is forgotten.
pub const TELLABLE: f32 = 0.25;
pub const FORGOTTEN: f32 = 0.1;
/// How long a rumour takes to fade to half as strong, in seconds.
pub const FADE_HALF_LIFE: f32 = 900.0;
/// How many rumours an NPC keeps in mind at once, the strongest.
pub const KEPT: usize = 5;
/// How far a hearer's regard moves towards a speaker's, of the difference, hearing what it
/// thinks of the player.
pub const SWAYED: f32 = 0.15;

/// Something the player did, as one NPC knows it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rumour {
    /// Which deed it is, the same for everyone who knows of it.
    pub id: u32,
    pub deed: Deed,
    /// Where it was done.
    pub at: Vector3<f32>,
    /// How strong it is: 1 seen, less told, less again the longer ago.
    pub strength: f32,
    /// Whether this one saw it done itself.
    pub saw: bool,
}

/// One NPC's mind: what it thinks of the player, and what it knows of them.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Mind {
    pub regard: f32,
    known: Vec<Rumour>,
}

impl Mind {
    /// How it takes the player.
    pub fn attitude(&self) -> Attitude {
        attitude(self.regard)
    }

    /// What it knows of them, strongest first.
    pub fn known(&self) -> &[Rumour] {
        &self.known
    }

    pub fn knows(&self, id: u32) -> bool {
        self.known.iter().any(|r| r.id == id)
    }

    fn feel(&mut self, by: f32) {
        self.regard = (self.regard + by).clamp(-1.0, 1.0);
    }

    fn learn(&mut self, rumour: Rumour) {
        self.known.retain(|r| r.id != rumour.id);
        self.known.push(rumour);
        self.known.sort_by(|a, b| b.strength.total_cmp(&a.strength));
        self.known.truncate(KEPT);
    }
}

/// Everyone's minds, by the game's index for each NPC.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Minds {
    minds: Vec<Mind>,
    next: u32,
}

impl Minds {
    /// No one, knowing nothing.
    pub fn clear(&mut self) {
        self.minds.clear();
    }

    /// Another NPC, neither for the player nor against them, knowing nothing: its index.
    pub fn add(&mut self) -> usize {
        self.minds.push(Mind::default());
        self.minds.len() - 1
    }

    /// No more than the first `len`, as the game's own list of them is cut back.
    pub fn truncate(&mut self, len: usize) {
        self.minds.truncate(len);
    }

    pub fn len(&self) -> usize {
        self.minds.len()
    }

    pub fn is_empty(&self) -> bool {
        self.minds.is_empty()
    }

    /// The `n`th's mind.
    pub fn mind(&self, n: usize) -> Option<&Mind> {
        self.minds.get(n)
    }

    /// The `n`th's regard for the player, and how it takes them: neutral, for one not there.
    pub fn regard(&self, n: usize) -> f32 {
        self.minds.get(n).map_or(0.0, |m| m.regard)
    }

    pub fn attitude(&self, n: usize) -> Attitude {
        attitude(self.regard(n))
    }

    /// `saw`, each of them, the player do `deed` at `at`: their regard moves its whole weight,
    /// and they know of it, at full strength, if it is talked about. Its id, for the rumour.
    pub fn saw(&mut self, saw: &[usize], deed: Deed, at: Vector3<f32>) -> u32 {
        let id = self.next;
        self.next += 1;
        for &n in saw {
            let Some(mind) = self.minds.get_mut(n) else {
                continue;
            };
            mind.feel(deed.weight());
            if deed.spreads() {
                mind.learn(Rumour { id, deed, at, strength: 1.0, saw: true });
            }
        }
        id
    }

    /// The rumour the `teller` knows and the `hearer` does not that is strongest - strong enough
    /// to be worth telling - if there is one.
    pub fn news(&self, teller: usize, hearer: usize) -> Option<Rumour> {
        let (t, h) = (self.minds.get(teller)?, self.minds.get(hearer)?);
        t.known.iter().copied().filter(|r| r.strength >= TELLABLE && !h.knows(r.id)).max_by(|a, b| a.strength.total_cmp(&b.strength))
    }

    /// The `teller` tells the `hearer` of `rumour`: it knows it, at [`RETELL`] of its strength,
    /// and its regard moves [`HEARSAY`] of the deed's weight, as strong as it heard it. Nothing,
    /// if it knew it already.
    pub fn tell(&mut self, hearer: usize, rumour: Rumour) {
        let Some(mind) = self.minds.get_mut(hearer) else {
            return;
        };
        if mind.knows(rumour.id) {
            return;
        }
        let strength = rumour.strength * RETELL;
        mind.feel(rumour.deed.weight() * HEARSAY * strength);
        mind.learn(Rumour { strength, saw: false, ..rumour });
    }

    /// The `speaker` says what it thinks of the player to the `hearer`, whose regard moves
    /// [`SWAYED`] of the way towards its.
    pub fn share_regard(&mut self, speaker: usize, hearer: usize) {
        let said = self.regard(speaker);
        if let Some(mind) = self.minds.get_mut(hearer) {
            mind.regard += (said - mind.regard) * SWAYED;
        }
    }

    /// Has the `n`th be another NPC, turned up afresh: neither for the player nor against them,
    /// but having heard the strongest of what the town talks about - the strongest rumour of
    /// each deed anyone knows of, up to [`KEPT`] - at [`NEWS`] of its strength.
    pub fn renew(&mut self, n: usize) {
        if n >= self.minds.len() {
            return;
        }
        let mut talk: Vec<Rumour> = Vec::new();
        for (m, mind) in self.minds.iter().enumerate() {
            if m == n {
                continue;
            }
            for &r in &mind.known {
                match talk.iter_mut().find(|t| t.id == r.id) {
                    Some(t) if t.strength < r.strength => *t = r,
                    Some(_) => (),
                    None => talk.push(r),
                }
            }
        }
        talk.sort_by(|a, b| b.strength.total_cmp(&a.strength));
        self.minds[n] = Mind::default();
        for r in talk.into_iter().take(KEPT) {
            let strength = r.strength * NEWS;
            if strength >= FORGOTTEN {
                let mind = &mut self.minds[n];
                mind.feel(r.deed.weight() * HEARSAY * strength);
                mind.learn(Rumour { strength, saw: false, ..r });
            }
        }
    }

    /// Moves everyone on `dt`: rumours fading, and forgotten once too faint; regard easing back.
    pub fn step(&mut self, dt: f32) {
        let fade = 0.5f32.powf(dt / FADE_HALF_LIFE);
        let ease = 0.5f32.powf(dt / REGARD_HALF_LIFE);
        for mind in &mut self.minds {
            mind.regard *= ease;
            for r in &mut mind.known {
                r.strength *= fade;
            }
            mind.known.retain(|r| r.strength >= FORGOTTEN);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minds(n: usize) -> Minds {
        let mut minds = Minds::default();
        (0..n).for_each(|_| {
            minds.add();
        });
        minds
    }

    #[test]
    fn what_is_seen_is_known_and_told_on_weaker_each_time_and_each_hearer_minds_it_less() {
        let mut minds = minds(6);
        minds.saw(&[0], Deed::Shooting, Vector3::zeros());
        assert_eq!(minds.attitude(0), Attitude::Hostile);
        let seen = minds.news(0, 1).unwrap();
        minds.tell(1, seen);
        let heard = minds.mind(1).unwrap().known()[0];
        assert!(!heard.saw && (heard.strength - RETELL).abs() < 1e-6);
        assert!(minds.regard(1) < 0.0 && minds.regard(1) > minds.regard(0));
        assert_eq!(minds.news(1, 0), None, "it knows it already");
        // Told on down the line, weaker each time and minded less, till it is too faint to tell:
        // 0.7, then 0.49, then 0.34, and 0.24 would be under TELLABLE.
        minds.tell(2, minds.news(1, 2).unwrap());
        minds.tell(3, minds.news(2, 3).unwrap());
        assert!(minds.regard(1) < minds.regard(2) && minds.regard(2) < minds.regard(3));
        assert!((minds.mind(3).unwrap().known()[0].strength - RETELL.powi(3)).abs() < 1e-6);
        minds.tell(4, minds.news(3, 4).unwrap());
        assert_eq!(minds.news(4, 5), None, "too faint to tell on");
        assert!(minds.news(3, 5).is_some(), "though the one before could still tell it");
        // Told again what it knows, nothing changes.
        let before = minds.regard(1);
        minds.tell(1, seen);
        assert_eq!(minds.regard(1), before);
        assert_eq!(minds.mind(1).unwrap().known().len(), 1);
    }

    #[test]
    fn a_bribe_is_felt_but_not_talked_about() {
        let mut minds = minds(2);
        minds.saw(&[0], Deed::Bribing, Vector3::zeros());
        assert!(minds.regard(0) > 0.0);
        assert_eq!(minds.news(0, 1), None);
    }

    #[test]
    fn rumours_fade_and_are_forgotten_and_regard_eases_back() {
        let mut minds = minds(2);
        minds.saw(&[0], Deed::Robbing, Vector3::zeros());
        minds.step(FADE_HALF_LIFE);
        let r = minds.mind(0).unwrap().known()[0];
        assert!((r.strength - 0.5).abs() < 1e-3);
        assert!(minds.regard(0) > Deed::Robbing.weight());
        minds.step(FADE_HALF_LIFE * 2.0);
        assert_eq!(minds.news(0, 1), None, "too faint to tell");
        minds.step(FADE_HALF_LIFE * 3.0);
        assert!(minds.mind(0).unwrap().known().is_empty());
    }

    #[test]
    fn one_turning_up_afresh_has_heard_what_the_town_talks_of_and_regard_is_swayed_by_talk() {
        let mut minds = minds(3);
        minds.saw(&[0, 1], Deed::Shooting, Vector3::zeros());
        minds.renew(2);
        let heard = minds.mind(2).unwrap().known();
        assert_eq!(heard.len(), 1);
        assert!((heard[0].strength - NEWS).abs() < 1e-6 && minds.regard(2) < 0.0);
        let before = minds.regard(2);
        minds.share_regard(0, 2);
        assert!(minds.regard(2) < before);
    }
}
