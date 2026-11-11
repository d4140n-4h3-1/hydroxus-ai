//! Suspicion: thinking the player did something no one saw them do. An NPC that learns of
//! trouble - one of them found lying, gunfire, a vault found empty - and saw the player near there about
//! then suspects them of it ([`Suspicions::trouble`]): the nearer and the closer in time, the more.
//! Suspicion is never certainty ([`MOST`]): seeing the deed done is a rumour (see
//! [`crate::social::rumour`]), and once it is known one way or the other the game settles it
//! ([`Suspicions::settle`]).
//!
//! Each NPC keeps a few of its own recent sightings of the player ([`Suspicions::saw_player`]) for
//! this. Suspicion is told on as rumours are, weaker each time and weaker again from a source the
//! hearer trusts little ([`Suspicions::tell`], [`Suspicions::hear`]). It fades
//! ([`HALF_LIFE`]), and an alibi takes it away: the player seen far off when it happened, by the
//! NPC itself or by someone it believes ([`Suspicions::alibi`]) - two observations that conflict,
//! weighed by how far each is believed.
//!
//! What it suspects makes it warier of the player than its regard alone would
//! ([`Suspicions::regard`]); the regard itself, what it thinks of them for what it knows they
//! did, is left alone.

use nalgebra::Vector3;

/// How far from where the trouble was the player has to have been seen to be suspected of it, in
/// meters, and how long before or after, in seconds.
pub const NEAR: f32 = 25.0;
pub const WITHIN: f32 = 60.0;
/// How far off the player has to have been seen, about when it happened, to be clear of it
/// altogether, in meters; and how close to when it happened that sighting has to be, in seconds.
pub const FAR: f32 = 80.0;
pub const ALIBI_WITHIN: f32 = 20.0;
/// The most an NPC suspects anyone of anything: never sure.
pub const MOST: f32 = 0.8;
/// How much of its suspicion an NPC passes on in telling it.
pub const RETOLD: f32 = 0.6;
/// How much of a suspicion an alibi believed altogether takes away.
pub const ALIBI: f32 = 0.9;
/// How long suspicion takes to fade halfway, in seconds.
pub const HALF_LIFE: f32 = 600.0;
/// Under which a suspicion is forgotten.
pub const FORGOTTEN: f32 = 0.05;
/// How many sightings of the player each keeps, the latest, and for how long, in seconds.
pub const KEPT: usize = 8;
pub const KEPT_FOR: f32 = 300.0;
/// How much regard suspicion at its strongest is worth, against the player.
pub const WARINESS: f32 = 0.5;

tuning!(
    /// How the player comes to be suspected, and cleared (see the module's constants, its
    /// defaults).
    {
        /// How near, in meters, and how close in time, in seconds, a sighting has to be.
        near: f32 = NEAR,
        within: f32 = WITHIN,
        /// How far off a sighting clears the player, in meters, and how close in time it has to be.
        far: f32 = FAR,
        alibi_within: f32 = ALIBI_WITHIN,
        /// The most it suspects anyone of anything.
        most: f32 = MOST,
        /// How much of it is passed on in the telling.
        retold: f32 = RETOLD,
        /// How much of it an alibi believed altogether takes away.
        alibi: f32 = ALIBI,
        /// How long it takes to fade halfway, in seconds, and under which it is forgotten.
        half_life: f32 = HALF_LIFE,
        forgotten: f32 = FORGOTTEN,
        /// How many sightings are kept, and for how long, in seconds.
        kept: usize = KEPT,
        kept_for: f32 = KEPT_FOR,
        /// How much regard suspicion at its strongest is worth, against the player.
        wariness: f32 = WARINESS,
    }
);

/// Where and when the player was seen.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Sighting {
    pub at: Vector3<f32>,
    pub when: f32,
}

/// Something an NPC suspects the player of.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Suspect {
    /// The trouble's own number, the game's: the same for everyone who suspects it.
    pub id: u32,
    /// Where and when it happened.
    pub at: Vector3<f32>,
    pub when: f32,
    /// How strongly it was suspected when last it changed, at `noted`; it fades from there.
    pub strength: f32,
    pub noted: f32,
    /// How many times it has been passed on: 0, it put it together itself.
    pub hops: u8,
}

impl Suspect {
    /// How strongly it is suspected at `now`.
    pub fn now(&self, tuning: &Tuning, now: f32) -> f32 {
        let ago = (now - self.noted).max(0.0);
        (self.strength * 0.5f32.powf(ago / tuning.half_life.max(1.0e-3))).max(0.0)
    }

    /// Whether it put it together itself, rather than being told.
    pub fn own(&self) -> bool {
        self.hops == 0
    }
}

/// What one NPC suspects the player of, and where it has seen them lately.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Suspicions {
    sightings: Vec<Sighting>,
    suspects: Vec<Suspect>,
}

fn sane(at: Vector3<f32>, when: f32) -> bool {
    at.iter().all(|v| v.is_finite()) && when.is_finite()
}

impl Suspicions {
    /// Suspecting nothing, having seen no one.
    pub fn new() -> Self {
        Self::default()
    }

    /// It saw the player at `at`, at time `now`. Sightings older than the tuning's `kept_for`
    /// are let go, and only the latest `kept` are kept.
    pub fn saw_player(&mut self, tuning: &Tuning, at: Vector3<f32>, now: f32) {
        if !sane(at, now) {
            return;
        }
        // Seen again where it saw them a moment ago: the same sighting, later.
        match self.sightings.last_mut() {
            Some(last) if now - last.when < 1.0 && (at - last.at).norm() < 2.0 => *last = Sighting { at, when: now },
            _ => self.sightings.push(Sighting { at, when: now }),
        }
        self.sightings.retain(|s| now - s.when <= tuning.kept_for);
        let over = self.sightings.len().saturating_sub(tuning.kept);
        self.sightings.drain(..over);
    }

    /// It learns of trouble `id` at `at`, which happened at `when` - as near as it can tell -
    /// `weight` from 0 to 1 for how much it matters (see [`crate::social::trouble::Kind::strength`]).
    /// If it saw the player near there about then, it suspects them, as strongly as the closest
    /// sighting says: full within no time and no distance, nothing at the tuning's `near` or
    /// `within`. Under the tuning's `forgotten`, it is not kept. What it saw of them far off about
    /// then clears them as an alibi would. Its suspicion of `id` at `now`, after.
    pub fn trouble(&mut self, tuning: &Tuning, id: u32, at: Vector3<f32>, when: f32, weight: f32, now: f32) -> f32 {
        if !sane(at, when) || !now.is_finite() || !weight.is_finite() {
            return self.of(tuning, id, now);
        }
        let closeness = self
            .sightings
            .iter()
            .map(|s| {
                let place = 1.0 - (s.at - at).norm() / tuning.near.max(1.0e-3);
                let time = 1.0 - (s.when - when).abs() / tuning.within.max(1.0e-3);
                place.max(0.0) * time.max(0.0)
            })
            .fold(0.0f32, f32::max);
        let strength = tuning.most.clamp(0.0, 1.0) * weight.clamp(0.0, 1.0) * closeness;
        // Too faint to keep, it is not kept: else it would be forgotten and found afresh in turn.
        if strength >= tuning.forgotten {
            self.keep(tuning, Suspect { id, at, when, strength, noted: now, hops: 0 }, now);
            let own: Vec<Sighting> = self.sightings.clone();
            for s in own {
                self.clear_of(tuning, id, s.at, s.when, 1.0, now);
            }
        }
        self.of(tuning, id, now)
    }

    /// Keeps `suspect`, unless it already suspects the same more strongly.
    fn keep(&mut self, tuning: &Tuning, suspect: Suspect, now: f32) -> bool {
        let suspect = Suspect { strength: suspect.strength.min(tuning.most.clamp(0.0, 1.0)), ..suspect };
        match self.suspects.iter_mut().find(|s| s.id == suspect.id) {
            Some(had) if had.now(tuning, now) >= suspect.strength => false,
            Some(had) => {
                *had = suspect;
                true
            }
            None => {
                self.suspects.push(suspect);
                true
            }
        }
    }

    /// What it would tell another of trouble `id` at `now`: weaker by the tuning's `retold`, one
    /// more hop. None, suspecting nothing of it.
    pub fn tell(&self, tuning: &Tuning, id: u32, now: f32) -> Option<Suspect> {
        let had = self.suspects.iter().find(|s| s.id == id)?;
        let strength = had.now(tuning, now) * tuning.retold.clamp(0.0, 1.0);
        (strength >= tuning.forgotten).then(|| Suspect { strength, noted: now, hops: had.hops.saturating_add(1), ..*had })
    }

    /// It is told `suspect` at `now` by one it trusts `trust`, from 0 to 1 (see
    /// [`crate::social::word::Trust`]): it suspects as much of it as it trusts the teller, unless
    /// it suspects more already. Whether that changed anything. What it saw of the player itself
    /// clears them as an alibi would.
    pub fn hear(&mut self, tuning: &Tuning, suspect: Suspect, trust: f32, now: f32) -> bool {
        if !sane(suspect.at, suspect.when) || !suspect.strength.is_finite() || !trust.is_finite() || !now.is_finite() {
            return false;
        }
        let strength = suspect.strength.max(0.0) * trust.clamp(0.0, 1.0);
        if strength < tuning.forgotten {
            return false;
        }
        let id = suspect.id;
        let kept = self.keep(tuning, Suspect { strength, noted: now, hops: suspect.hops.max(1), ..suspect }, now);
        if kept {
            let own: Vec<Sighting> = self.sightings.clone();
            for s in own {
                self.clear_of(tuning, id, s.at, s.when, 1.0, now);
            }
        }
        kept
    }

    /// The player was seen at `at` at time `when` - by someone else, it believes `belief` of it
    /// (see [`crate::social::word::Word::belief`]), or by itself, 1. Whatever it suspects them of
    /// that happened far from there about then, it suspects them the less: by the tuning's `alibi`
    /// at most, as far off as the tuning's `far` and as close in time as nothing. Each suspicion
    /// so lessened, with how strong it is now.
    pub fn alibi(&mut self, tuning: &Tuning, at: Vector3<f32>, when: f32, belief: f32, now: f32) -> Vec<(u32, f32)> {
        if !sane(at, when) || !belief.is_finite() || !now.is_finite() {
            return Vec::new();
        }
        let ids: Vec<u32> = self.suspects.iter().map(|s| s.id).collect();
        ids.into_iter().filter_map(|id| self.clear_of(tuning, id, at, when, belief, now).map(|left| (id, left))).collect()
    }

    /// The player seen at `at` at `when`, `belief` of it, against suspicion of `id`: how strong
    /// it is now, if that lessened it.
    fn clear_of(&mut self, tuning: &Tuning, id: u32, at: Vector3<f32>, when: f32, belief: f32, now: f32) -> Option<f32> {
        let s = self.suspects.iter_mut().find(|s| s.id == id)?;
        let span = (tuning.far - tuning.near).max(1.0e-3);
        let apart = (((at - s.at).norm() - tuning.near) / span).clamp(0.0, 1.0);
        let timely = (1.0 - (when - s.when).abs() / tuning.alibi_within.max(1.0e-3)).clamp(0.0, 1.0);
        let off = tuning.alibi.clamp(0.0, 1.0) * belief.clamp(0.0, 1.0) * apart * timely;
        if off <= 0.0 {
            return None;
        }
        s.strength = s.now(tuning, now) * (1.0 - off);
        s.noted = now;
        Some(s.strength)
    }

    /// It knows now whether the player did `id` - saw it done, or saw someone else do it: it no
    /// longer suspects; it knows.
    pub fn settle(&mut self, id: u32) {
        self.suspects.retain(|s| s.id != id);
    }

    /// How strongly it suspects the player of `id` at `now`: 0 for not at all.
    pub fn of(&self, tuning: &Tuning, id: u32, now: f32) -> f32 {
        self.suspects.iter().find(|s| s.id == id).map_or(0.0, |s| s.now(tuning, now))
    }

    /// The most it suspects the player of anything at `now`.
    pub fn most(&self, tuning: &Tuning, now: f32) -> f32 {
        self.suspects.iter().map(|s| s.now(tuning, now)).fold(0.0, f32::max)
    }

    /// `regard` for the player, as it takes them suspecting what it does at `now`: less by the
    /// tuning's `wariness` at its most suspicious. For how it takes them (see
    /// [`crate::social::opinion::attitude`]), not to keep as its regard.
    pub fn regard(&self, tuning: &Tuning, regard: f32, now: f32) -> f32 {
        (regard - self.most(tuning, now) * tuning.wariness.max(0.0)).clamp(-1.0, 1.0)
    }

    /// Forgets suspicion faded under the tuning's `forgotten` at `now`, and sightings older than
    /// its `kept_for`.
    pub fn forget(&mut self, tuning: &Tuning, now: f32) {
        self.suspects.retain(|s| s.now(tuning, now) >= tuning.forgotten);
        self.sightings.retain(|s| now - s.when <= tuning.kept_for);
    }

    /// What it suspects, in the order it came to.
    pub fn suspects(&self) -> &[Suspect] {
        &self.suspects
    }

    /// Where it has seen the player lately, oldest first.
    pub fn sightings(&self) -> &[Sighting] {
        &self.sightings
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T: Tuning = Tuning::DEFAULT;

    fn at(x: f32) -> Vector3<f32> {
        Vector3::new(x, 0.0, 0.0)
    }

    #[test]
    fn seen_near_trouble_about_then_is_suspected_the_more_the_nearer() {
        let mut close = Suspicions::new();
        close.saw_player(&T, at(2.0), 100.0);
        let mut farther = Suspicions::new();
        farther.saw_player(&T, at(15.0), 130.0);
        let mut nowhere = Suspicions::new();
        nowhere.saw_player(&T, at(40.0), 100.0);
        let mut long_before = Suspicions::new();
        long_before.saw_player(&T, at(0.0), 10.0);
        let c = close.trouble(&T, 1, at(0.0), 100.0, 1.0, 110.0);
        let f = farther.trouble(&T, 1, at(0.0), 100.0, 1.0, 110.0);
        assert!(c > 0.7 && c <= MOST, "{c}");
        assert!(f > 0.0 && f < c * 0.5, "{f} against {c}");
        assert_eq!(nowhere.trouble(&T, 1, at(0.0), 100.0, 1.0, 110.0), 0.0);
        assert_eq!(long_before.trouble(&T, 1, at(0.0), 100.0, 1.0, 110.0), 0.0);
        assert!(nowhere.suspects().is_empty());
        // Barely near enough: too faint to keep, so not kept - nor forgotten and found again.
        let mut barely = Suspicions::new();
        barely.saw_player(&T, at(24.0), 100.0);
        assert_eq!(barely.trouble(&T, 1, at(0.0), 100.0, 1.0, 110.0), 0.0);
        assert!(barely.suspects().is_empty());
        // A lesser kind of trouble, less.
        let mut minor = Suspicions::new();
        minor.saw_player(&T, at(2.0), 100.0);
        assert!(minor.trouble(&T, 1, at(0.0), 100.0, 0.3, 110.0) < c * 0.4);
    }

    #[test]
    fn suspicion_is_never_certainty_and_fades() {
        let mut s = Suspicions::new();
        s.saw_player(&T, at(0.0), 50.0);
        let had = s.trouble(&T, 3, at(0.0), 50.0, 1.0, 50.0);
        assert_eq!(had, MOST);
        assert!((s.of(&T, 3, 50.0 + HALF_LIFE) - MOST * 0.5).abs() < 1.0e-5);
        // As far faded in one look as in many.
        let mut stepped = s.clone();
        for n in 0..600 {
            stepped.forget(&T, 50.0 + n as f32);
        }
        assert!((stepped.of(&T, 3, 650.0) - s.of(&T, 3, 650.0)).abs() < 1.0e-6);
        s.forget(&T, 50.0 + HALF_LIFE * 5.0);
        assert!(s.suspects().is_empty());
    }

    #[test]
    fn told_on_it_is_weaker_and_weaker_still_from_one_trusted_little() {
        let mut saw = Suspicions::new();
        saw.saw_player(&T, at(0.0), 0.0);
        let own = saw.trouble(&T, 7, at(0.0), 0.0, 1.0, 0.0);
        let told = saw.tell(&T, 7, 0.0).unwrap();
        assert_eq!(told.hops, 1);
        let mut trusting = Suspicions::new();
        assert!(trusting.hear(&T, told, 1.0, 0.0));
        let mut doubting = Suspicions::new();
        assert!(doubting.hear(&T, told, 0.3, 0.0));
        let (t, d) = (trusting.of(&T, 7, 0.0), doubting.of(&T, 7, 0.0));
        assert!((t - own * RETOLD).abs() < 1.0e-6 && (d - t * 0.3).abs() < 1.0e-6, "{t} {d}");
        assert!(!trusting.suspects()[0].own());
        // Told it more weakly than it already suspects it: nothing changes.
        assert!(!saw.hear(&T, told, 1.0, 0.0));
        assert_eq!(saw.of(&T, 7, 0.0), own);
        // From one trusted not at all: nothing.
        let mut deaf = Suspicions::new();
        assert!(!deaf.hear(&T, told, 0.0, 0.0));
        assert!(Suspicions::new().tell(&T, 7, 0.0).is_none());
    }

    #[test]
    fn an_alibi_clears_the_player_as_far_as_it_is_believed() {
        let base = {
            let mut s = Suspicions::new();
            s.saw_player(&T, at(5.0), 95.0);
            s.trouble(&T, 2, at(0.0), 100.0, 1.0, 100.0);
            s
        };
        let had = base.of(&T, 2, 100.0);
        // Seen far off just as it happened, by one believed altogether: all but cleared.
        let mut sure = base.clone();
        let changed = sure.alibi(&T, at(200.0), 101.0, 1.0, 100.0);
        assert_eq!(changed.len(), 1);
        assert!(sure.of(&T, 2, 100.0) < had * 0.2, "{}", sure.of(&T, 2, 100.0));
        // By one half believed: about half as much off.
        let mut half = base.clone();
        half.alibi(&T, at(200.0), 101.0, 0.5, 100.0);
        assert!(half.of(&T, 2, 100.0) > sure.of(&T, 2, 100.0) && half.of(&T, 2, 100.0) < had * 0.7);
        // Seen near, or long before: no alibi at all.
        let mut near = base.clone();
        assert!(near.alibi(&T, at(10.0), 100.0, 1.0, 100.0).is_empty());
        let mut long_before = base.clone();
        assert!(long_before.alibi(&T, at(200.0), 30.0, 1.0, 100.0).is_empty());
        assert_eq!(near.of(&T, 2, 100.0), had);
    }

    #[test]
    fn what_it_saw_itself_counts_against_what_it_is_told() {
        // It saw the player far off just when it happened; told someone suspects them, it does
        // little.
        let mut told = Suspicions::new();
        told.saw_player(&T, at(0.0), 0.0);
        told.trouble(&T, 9, at(0.0), 0.0, 1.0, 0.0);
        let rumour = told.tell(&T, 9, 0.0).unwrap();
        let mut saw_elsewhere = Suspicions::new();
        saw_elsewhere.saw_player(&T, at(300.0), 1.0);
        saw_elsewhere.hear(&T, rumour, 1.0, 2.0);
        let mut saw_nothing = Suspicions::new();
        saw_nothing.hear(&T, rumour, 1.0, 2.0);
        assert!(saw_elsewhere.of(&T, 9, 2.0) < saw_nothing.of(&T, 9, 2.0) * 0.2);
    }

    #[test]
    fn suspicion_makes_it_warier_till_settled() {
        let mut s = Suspicions::new();
        assert_eq!(s.regard(&T, 0.1, 0.0), 0.1);
        s.saw_player(&T, at(0.0), 0.0);
        s.trouble(&T, 4, at(0.0), 0.0, 1.0, 0.0);
        let wary = s.regard(&T, 0.1, 0.0);
        assert!((wary - (0.1 - MOST * WARINESS)).abs() < 1.0e-6);
        assert_eq!(
            crate::social::opinion::attitude(&crate::social::opinion::Tuning::DEFAULT, wary),
            crate::social::opinion::Attitude::Wary
        );
        s.settle(4);
        assert_eq!(s.most(&T, 0.0), 0.0);
    }

    #[test]
    fn it_keeps_only_its_latest_sightings_and_lets_nonsense_go() {
        let mut s = Suspicions::new();
        for n in 0..20 {
            s.saw_player(&T, at(n as f32 * 10.0), n as f32 * 5.0);
        }
        assert_eq!(s.sightings().len(), KEPT);
        assert_eq!(s.sightings().last().unwrap().when, 95.0);
        // Seen again a moment later on the same spot: the same sighting.
        s.saw_player(&T, at(190.5), 95.5);
        assert_eq!(s.sightings().len(), KEPT);
        s.saw_player(&T, Vector3::new(f32::NAN, 0.0, 0.0), 96.0);
        assert_eq!(s.sightings().len(), KEPT);
        s.forget(&T, 95.0 + KEPT_FOR + 1.0);
        assert!(s.sightings().is_empty());
        assert_eq!(s.trouble(&T, 1, at(f32::INFINITY), 0.0, 1.0, 0.0), 0.0);
        assert!(s.alibi(&T, at(0.0), f32::NAN, 1.0, 0.0).is_empty());
    }
}
