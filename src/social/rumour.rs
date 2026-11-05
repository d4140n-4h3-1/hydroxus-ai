//! Rumours: what NPCs have seen the player do, and told each other. Each NPC has a mind
//! ([`Minds`]) - its regard for the player (see [`crate::social::opinion`]) and what it knows of
//! them. Seeing a deed done, it knows it at full strength and its regard moves the deed's full
//! weight; told it by another, at [`RETELL`] of the teller's strength, its regard moving
//! [`HEARSAY`] of that. Rumours fade ([`FADE_HALF_LIFE`]); one too faint to be worth telling
//! ([`TELLABLE`]) is not told on, and one fainter still ([`FORGOTTEN`]) is forgotten. An NPC that
//! turns up afresh - one of a city's crowd, say - has heard what the town is talking about, at
//! [`NEWS`] of its strength.
//!
//! A deed counts by who it was done to (see [`crate::social::faction`]): each NPC of a faction
//! takes it as its faction takes harm done to the victim's - a gang member seeing one of security
//! shot down may think the better of the player for it. Each faction's standing with the player
//! outlasts its NPCs, and one turning up afresh starts from it.
//!
//! A mind keeps what trouble it knows of as well (see [`crate::social::trouble`]), told on the
//! same way: one of them found lying, an alarm, gunfire.

use crate::social::{
    faction::{Faction, Factions},
    opinion::{self, attitude, Attitude, Deed},
    trouble::{self, Kind, Trouble},
};
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

tuning!(
    /// How rumours of the player go round (see the module's constants, its defaults).
    {
        /// How strong a rumour is retold, of how strong it was; how much hearsay moves regard, of
        /// seeing it; and how strong news is to one turned up afresh.
        retell: f32 = RETELL,
        hearsay: f32 = HEARSAY,
        news: f32 = NEWS,
        /// How strong a rumour has to be to be told, and under which it is forgotten.
        tellable: f32 = TELLABLE,
        forgotten: f32 = FORGOTTEN,
        /// How long a rumour takes to fade halfway, in seconds.
        fade_half_life: f32 = FADE_HALF_LIFE,
        /// How many rumours each keeps.
        kept: usize = KEPT,
        /// How far one is swayed towards another's regard, talking.
        swayed: f32 = SWAYED,
    }
);

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
    /// The faction of the one it was done to; none, for no one in particular - a robbery - or one
    /// of no faction.
    pub victim: Option<Faction>,
}

/// One NPC's mind: what it thinks of the player, what it knows of them, its faction, and what
/// trouble it knows of.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Mind {
    pub regard: f32,
    known: Vec<Rumour>,
    faction: Option<Faction>,
    troubles: Vec<Trouble>,
}

impl Mind {
    /// How it takes the player, as `opinion` has regard taken.
    pub fn attitude(&self, opinion: &opinion::Tuning) -> Attitude {
        attitude(opinion, self.regard)
    }

    /// What it knows of them, strongest first.
    pub fn known(&self) -> &[Rumour] {
        &self.known
    }

    pub fn knows(&self, id: u32) -> bool {
        self.known.iter().any(|r| r.id == id)
    }

    /// Its faction, if it has one.
    pub fn faction(&self) -> Option<Faction> {
        self.faction
    }

    /// What trouble it knows of, strongest first.
    pub fn troubles(&self) -> &[Trouble] {
        &self.troubles
    }

    /// Whether it knows of `trouble` already: it, or the same kind close by.
    pub fn knows_of(&self, tuning: &trouble::Tuning, trouble: &Trouble) -> bool {
        self.troubles.iter().any(|t| t.is(tuning, trouble))
    }

    fn feel(&mut self, by: f32) {
        self.regard = (self.regard + by).clamp(-1.0, 1.0);
    }

    fn learn(&mut self, rumour: Rumour, kept: usize) {
        self.known.retain(|r| r.id != rumour.id);
        self.known.push(rumour);
        self.known.sort_by(|a, b| b.strength.total_cmp(&a.strength));
        self.known.truncate(kept);
    }

    /// Knows of `trouble`: as the same trouble it knew of already, at the stronger of the two.
    fn hear_of(&mut self, tuning: &trouble::Tuning, trouble: Trouble, kept: usize) {
        if let Some(known) = self.troubles.iter_mut().find(|t| t.is(tuning, &trouble)) {
            if trouble.strength > known.strength {
                *known = Trouble { id: known.id, ..trouble };
            }
        } else {
            self.troubles.push(trouble);
        }
        self.troubles.sort_by(|a, b| b.strength.total_cmp(&a.strength));
        self.troubles.truncate(kept);
    }
}

/// Everyone's minds, by the game's index for each NPC; and every faction's standing.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Minds {
    minds: Vec<Mind>,
    /// How rumours go round, how regard is taken, and how trouble is known of.
    pub tuning: Tuning,
    pub opinion: opinion::Tuning,
    pub trouble: trouble::Tuning,
    /// Each faction's standing with the player, and how each takes harm done to the others.
    pub factions: Factions,
    next: u32,
}

impl Minds {
    /// No one, knowing nothing.
    pub fn clear(&mut self) {
        self.minds.clear();
    }

    /// Another NPC, of no faction, neither for the player nor against them, knowing nothing: its
    /// index.
    pub fn add(&mut self) -> usize {
        self.add_in(None)
    }

    /// Another NPC, of `faction`, knowing nothing but thinking of the player as its faction does
    /// (see [`Factions::inherited`]): its index.
    pub fn add_in(&mut self, faction: Option<Faction>) -> usize {
        self.minds.push(Mind { regard: self.factions.inherited(faction), faction, ..Mind::default() });
        self.minds.len() - 1
    }

    /// Has the `n`th be of `faction`.
    pub fn set_faction(&mut self, n: usize, faction: Option<Faction>) {
        if let Some(mind) = self.minds.get_mut(n) {
            mind.faction = faction;
        }
    }

    /// Moves the `n`th's regard for the player `by`: a grudge, say, for one of its own shot down.
    pub fn feel(&mut self, n: usize, by: f32) {
        if let Some(mind) = self.minds.get_mut(n) {
            mind.feel(by);
        }
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
        attitude(&self.opinion, self.regard(n))
    }

    /// `saw`, each of them, the player do `deed` at `at`, to no one in particular: as
    /// [`Minds::saw_done`]. Its id, for the rumour.
    pub fn saw(&mut self, saw: &[usize], deed: Deed, at: Vector3<f32>) -> u32 {
        self.saw_done(saw, deed, at, None).0
    }

    /// `saw`, each of them, the player do `deed` at `at` to one of `victim` - to no one in
    /// particular, with none: their regard moves its whole weight, as their faction takes harm done
    /// to the victim's, and they know of it, at full strength, if it is talked about; and, seen by
    /// any, so does every faction's standing (see [`Factions::hear_of`]). Its id, for the rumour,
    /// and each faction whose attitude to the player changed, with how it takes them now.
    pub fn saw_done(&mut self, saw: &[usize], deed: Deed, at: Vector3<f32>, victim: Option<Faction>) -> (u32, Vec<(Faction, Attitude)>) {
        let id = self.next;
        self.next += 1;
        for &n in saw {
            let Some(mind) = self.minds.get(n) else {
                continue;
            };
            let relation = self.factions.relation(mind.faction, victim);
            let mind = &mut self.minds[n];
            mind.feel(deed.weight() * relation);
            if deed.spreads() {
                mind.learn(Rumour { id, deed, at, strength: 1.0, saw: true, victim }, self.tuning.kept);
            }
        }
        let changed = match saw.iter().any(|&n| n < self.minds.len()) {
            true => self.factions.hear_of(&self.opinion, deed, victim),
            false => Vec::new(),
        };
        (id, changed)
    }

    /// The rumour the `teller` knows and the `hearer` does not that is strongest - strong enough
    /// to be worth telling - if there is one.
    pub fn news(&self, teller: usize, hearer: usize) -> Option<Rumour> {
        let (t, h) = (self.minds.get(teller)?, self.minds.get(hearer)?);
        t.known.iter().copied().filter(|r| r.strength >= self.tuning.tellable && !h.knows(r.id)).max_by(|a, b| a.strength.total_cmp(&b.strength))
    }

    /// The `teller` tells the `hearer` of `rumour`: it knows it, at [`RETELL`] of its strength,
    /// and its regard moves [`HEARSAY`] of the deed's weight, as strong as it heard it. Nothing,
    /// if it knew it already.
    pub fn tell(&mut self, hearer: usize, rumour: Rumour) {
        let tuning = self.tuning;
        let Some(mind) = self.minds.get(hearer) else {
            return;
        };
        if mind.knows(rumour.id) {
            return;
        }
        let relation = self.factions.relation(mind.faction, rumour.victim);
        let mind = &mut self.minds[hearer];
        let strength = rumour.strength * tuning.retell;
        mind.feel(rumour.deed.weight() * tuning.hearsay * strength * relation);
        mind.learn(Rumour { strength, saw: false, ..rumour }, tuning.kept);
    }

    /// The `speaker` says what it thinks of the player to the `hearer`, whose regard moves
    /// [`SWAYED`] of the way towards its.
    pub fn share_regard(&mut self, speaker: usize, hearer: usize) {
        let said = self.regard(speaker);
        if let Some(mind) = self.minds.get_mut(hearer) {
            mind.regard += (said - mind.regard) * self.tuning.swayed;
        }
    }

    /// Has the `n`th be another NPC of the same faction, turned up afresh: thinking of the player
    /// as its faction does (see [`Factions::inherited`]), and having heard the strongest of what
    /// the town talks about - the strongest rumour of each deed anyone knows of, and the strongest
    /// of each trouble, up to [`KEPT`] of each - at [`NEWS`] of its strength. Its faction's
    /// standing has what it hears in it already: the rumours tell it what, and move it no more -
    /// but for one of no faction, which takes them to heart as hearsay.
    pub fn renew(&mut self, n: usize) {
        if n >= self.minds.len() {
            return;
        }
        let mut talk: Vec<Rumour> = Vec::new();
        let mut troubles: Vec<Trouble> = Vec::new();
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
            for &t in &mind.troubles {
                match troubles.iter_mut().find(|known| known.id == t.id) {
                    Some(known) if known.strength < t.strength => *known = t,
                    Some(_) => (),
                    None => troubles.push(t),
                }
            }
        }
        talk.sort_by(|a, b| b.strength.total_cmp(&a.strength));
        troubles.sort_by(|a, b| b.strength.total_cmp(&a.strength));
        let faction = self.minds[n].faction;
        self.minds[n] = Mind { regard: self.factions.inherited(faction), faction, ..Mind::default() };
        let tuning = self.tuning;
        for r in talk.into_iter().take(tuning.kept) {
            let strength = r.strength * tuning.news;
            if strength >= tuning.forgotten {
                // Of no faction, it takes what it hears to heart itself, with no standing to have
                // done it for it.
                if faction.is_none() {
                    self.minds[n].feel(r.deed.weight() * tuning.hearsay * strength);
                }
                self.minds[n].learn(Rumour { strength, saw: false, ..r }, tuning.kept);
            }
        }
        let trouble = self.trouble;
        for t in troubles.into_iter().take(tuning.kept) {
            let strength = t.strength * tuning.news;
            if strength >= tuning.forgotten {
                self.minds[n].hear_of(&trouble, Trouble { strength, saw: false, ..t }, tuning.kept);
            }
        }
    }

    /// `saw`, each of them, come across trouble of `kind` at `at` - one of them lying there, the
    /// alarm, gunfire - and know of it as strongly as its kind says (see [`Kind::strength`]); as
    /// the same trouble, if they knew of the same kind close by. Its id.
    pub fn came_across(&mut self, saw: &[usize], kind: Kind, at: Vector3<f32>) -> u32 {
        let id = self.next;
        self.next += 1;
        let trouble = Trouble { id, kind, at, strength: kind.strength(), saw: true };
        let (tuning, kept) = (self.trouble, self.tuning.kept);
        for &n in saw {
            if let Some(mind) = self.minds.get_mut(n) {
                mind.hear_of(&tuning, trouble, kept);
            }
        }
        id
    }

    /// The trouble the `teller` knows of and the `hearer` does not that is strongest - strong
    /// enough to be worth telling - if there is any.
    pub fn trouble_news(&self, teller: usize, hearer: usize) -> Option<Trouble> {
        let (t, h) = (self.minds.get(teller)?, self.minds.get(hearer)?);
        t.troubles
            .iter()
            .copied()
            .filter(|t| t.strength >= self.tuning.tellable && !h.knows_of(&self.trouble, t))
            .max_by(|a, b| a.strength.total_cmp(&b.strength))
    }

    /// The `hearer` is told of `trouble`: it knows of it, at [`RETELL`] of its strength. Nothing,
    /// if it knew of it already.
    pub fn tell_trouble(&mut self, hearer: usize, trouble: Trouble) {
        let (tuning, retell, kept) = (self.trouble, self.tuning.retell, self.tuning.kept);
        if let Some(mind) = self.minds.get_mut(hearer).filter(|mind| !mind.knows_of(&tuning, &trouble)) {
            mind.hear_of(&tuning, Trouble { strength: trouble.strength * retell, saw: false, ..trouble }, kept);
        }
    }

    /// Where the `n`th keeps away from, going about its business, for the trouble it knows of:
    /// each spot and how far round it, in meters (see [`Trouble::shunned`]).
    pub fn shunned(&self, n: usize) -> Vec<(Vector3<f32>, f32)> {
        self.minds.get(n).map_or(Vec::new(), |mind| mind.troubles.iter().filter_map(|t| t.shunned(&self.trouble)).collect())
    }

    /// Moves everyone on `dt`: rumours and trouble fading, and forgotten once too faint; regard
    /// easing back, towards what its faction thinks of the player; and every faction's standing
    /// easing back to nothing, more slowly.
    pub fn step(&mut self, dt: f32) {
        let fade = 0.5f32.powf(dt / self.tuning.fade_half_life);
        let fade_trouble = 0.5f32.powf(dt / self.trouble.fade_half_life);
        let ease = 0.5f32.powf(dt / self.opinion.regard_half_life);
        let forgotten = self.tuning.forgotten;
        self.factions.step(dt);
        for mind in &mut self.minds {
            let towards = self.factions.inherited(mind.faction);
            mind.regard = towards + (mind.regard - towards) * ease;
            for r in &mut mind.known {
                r.strength *= fade;
            }
            mind.known.retain(|r| r.strength >= forgotten);
            for t in &mut mind.troubles {
                t.strength *= fade_trouble;
            }
            mind.troubles.retain(|t| t.strength >= forgotten);
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

    #[test]
    fn a_deed_is_taken_by_who_it_was_done_to_and_a_faction_remembers_it_past_its_members() {
        const SECURITY: Faction = 0;
        const GANG: Faction = 1;
        let mut minds = Minds { factions: Factions::new(2, Default::default()), ..Minds::default() };
        minds.factions.relate(GANG, Some(SECURITY), -0.6);
        let guard = minds.add_in(Some(SECURITY));
        let thug = minds.add_in(Some(GANG));
        let (_, changed) = minds.saw_done(&[guard, thug], Deed::Shooting, Vector3::zeros(), Some(SECURITY));
        assert!(minds.regard(guard) < 0.0 && minds.regard(thug) > 0.0, "the gang is glad of it");
        assert!(changed.contains(&(SECURITY, Attitude::Wary)), "{changed:?}");
        // One of security turning up afresh is wary of the player from the first.
        let new = minds.add_in(Some(SECURITY));
        assert!(minds.regard(new) < 0.0);
        minds.renew(guard);
        assert!((minds.regard(guard) - minds.regard(new)).abs() < 1e-6, "as its faction takes them");
        // Told of it, the gang's hearer is glad, too.
        let other = minds.add_in(Some(GANG));
        let before = minds.regard(other);
        minds.tell(other, minds.news(thug, other).unwrap());
        assert!(minds.regard(other) > before);
        // In time a member's regard eases back towards its faction's, not to nothing.
        minds.step(opinion::REGARD_HALF_LIFE * 10.0);
        let towards = minds.factions.inherited(Some(GANG));
        assert!(towards > 0.0 && (minds.regard(thug) - towards).abs() < 0.01);
    }

    #[test]
    fn trouble_is_told_on_kept_away_from_and_forgotten_sooner_than_the_player() {
        let mut minds = minds(4);
        minds.came_across(&[0], Kind::Gunfire, Vector3::zeros());
        // More shots close by are the same gunfight.
        minds.came_across(&[0], Kind::Gunfire, Vector3::new(5.0, 0.0, 0.0));
        assert_eq!(minds.mind(0).unwrap().troubles().len(), 1);
        minds.came_across(&[0], Kind::Body, Vector3::new(40.0, 0.0, 0.0));
        let news = minds.trouble_news(0, 1).unwrap();
        assert_eq!(news.kind, Kind::Body, "the strongest first");
        minds.tell_trouble(1, news);
        assert!(minds.mind(1).unwrap().troubles()[0].strength < 1.0 && !minds.mind(1).unwrap().troubles()[0].saw);
        assert_eq!(minds.trouble_news(1, 0), None, "it knows of it already");
        let shunned = minds.shunned(0);
        assert_eq!(shunned.len(), 2);
        assert!(shunned.iter().any(|&(at, reach)| at.x == 40.0 && reach == trouble::SHUN_REACH));
        // One turning up afresh has heard of it, at the town's strength.
        minds.renew(3);
        assert_eq!(minds.mind(3).unwrap().troubles().len(), 2);
        minds.step(trouble::FADE_HALF_LIFE * 4.0);
        assert!(minds.mind(0).unwrap().troubles().is_empty(), "a town gets over it");
    }
}
