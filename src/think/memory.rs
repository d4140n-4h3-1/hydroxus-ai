//! Memory: what one NPC remembers having happened - where it lost the player, who it met, what it
//! saw done, where it was shot at, which side someone is on, where it looked and what it found -
//! each a [`Record`], fading with time and forgotten once faint, by its kind's own half-life.
//!
//! The rest of [`crate::think`] keeps what was learned in aggregate: [`super::lesson`] how well
//! each thing it does tends to work, [`super::habit`] what the player tends to do,
//! [`super::haunt`] where the player tends to be. Memory keeps the occasions themselves, one record
//! each, so that an NPC can act on a particular one: not look again where it looked a moment ago
//! and found nothing, keep its head down where it was shot at, remember the face of someone who
//! drew on it.
//!
//! - Noting the same thing again ([`Memory::note`]) - the same kind, about the same one, within
//!   the tuning's `same_within` meters - strengthens the record it has rather than adding another,
//!   and brings it back to fresh: a place it keeps finding the player is remembered the better.
//! - Each [`Kind`] fades by its own half-life and keeps at most its own number of records, the
//!   faintest forgotten first; anything under the tuning's `forgotten` is gone.
//! - What one tells another ([`Memory::tell`]) comes to them as secondhand - not
//!   [`Record::firsthand`] - and fainter, by the tuning's `told` share.
//! - What is recalled ([`Memory::recall`], [`Memory::near`], [`Memory::about`]) comes strongest
//!   first. Nothing is random, and with the `serde` feature it is kept between sessions.
//!
//! Retrieval in context, rather than going through everything it remembers:
//!
//! - **Importance.** A record can matter more than another ([`Record::importance`], 0 to 1): being
//!   shot at more than passing someone. The more it matters, the slower it fades - up to the
//!   tuning's `importance_hold` again as long - and the more it counts when recalled.
//! - **Relevance.** [`Memory::recall_for`] takes a [`Cue`] - which kinds, near where and how far,
//!   about whom, of what - and gives back the few records that fit it best, each with how well
//!   ([`Memory::relevance`]): how strongly it is remembered, times how near the place (1 at it, a
//!   half at the cue's edge, none past it), whether it is about the one asked after and is the
//!   thing asked about, and how much it matters.
//! - **Consolidation.** Many faint records of the same kind of thing, close together - a guard that
//!   keeps glimpsing someone about one courtyard - become one ([`Memory::consolidate`]): where
//!   they were on average, by how strong each was; as strong as all of them together; noted as
//!   many times. Only records under the tuning's `consolidate_below` are merged, within its
//!   `consolidate_within` meters. A kind that is full consolidates before it forgets anything -
//!   then whatever their strength, summing up being better than forgetting.

use nalgebra::Vector3;

/// What a record is of.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Kind {
    /// A place that matters: where the player was lost, where they hid.
    Place,
    /// Someone it knows of: who drew on it, who helped it.
    Character,
    /// A meeting with someone: where and when it saw the player.
    Encounter,
    /// Something it saw done, or was told of.
    Event,
    /// Somewhere it came to harm, or nearly: shot at, a trap.
    Danger,
    /// Which side someone is on, or a group's ties.
    Group,
    /// Somewhere it looked into something, and what came of it.
    Investigation,
}

impl Kind {
    /// Every kind, in order.
    pub const ALL: [Kind; 7] = [Kind::Place, Kind::Character, Kind::Encounter, Kind::Event, Kind::Danger, Kind::Group, Kind::Investigation];
}

/// How long, by default, till a record of each kind is half as strong, in seconds.
pub const PLACE_HALF_LIFE: f32 = 1800.0;
pub const CHARACTER_HALF_LIFE: f32 = 3600.0;
pub const ENCOUNTER_HALF_LIFE: f32 = 1200.0;
pub const EVENT_HALF_LIFE: f32 = 900.0;
pub const DANGER_HALF_LIFE: f32 = 1200.0;
pub const GROUP_HALF_LIFE: f32 = 7200.0;
pub const INVESTIGATION_HALF_LIFE: f32 = 300.0;
/// How many records of each kind it keeps, at most.
pub const MOST: usize = 32;
/// How faint a record is when it is forgotten.
pub const FORGOTTEN: f32 = 0.05;
/// How near, in meters, two records of a place have to be to be of the same one.
pub const SAME_WITHIN: f32 = 3.0;
/// How much of what it is told it takes in, against what it saw for itself.
pub const TOLD: f32 = 0.5;
/// How far a record's outcome moves to a new one's, each time it is noted again.
pub const OUTCOME_RATE: f32 = 0.5;
/// How much longer the most important record lasts, on top of its kind's half-life: three times
/// again - four times as long.
pub const IMPORTANCE_HOLD: f32 = 3.0;
/// How faint a record has to be to be merged with others like it, and how near them, in meters.
pub const CONSOLIDATE_BELOW: f32 = 0.5;
pub const CONSOLIDATE_WITHIN: f32 = 8.0;

tuning!(
    /// How long an NPC remembers each kind of thing, and how much (see the module's constants).
    {
        /// How long till a record of each kind is half as strong, in seconds.
        place_half_life: f32 = PLACE_HALF_LIFE,
        character_half_life: f32 = CHARACTER_HALF_LIFE,
        encounter_half_life: f32 = ENCOUNTER_HALF_LIFE,
        event_half_life: f32 = EVENT_HALF_LIFE,
        danger_half_life: f32 = DANGER_HALF_LIFE,
        group_half_life: f32 = GROUP_HALF_LIFE,
        investigation_half_life: f32 = INVESTIGATION_HALF_LIFE,
        /// How many records of each kind it keeps, at most.
        most: usize = MOST,
        /// How faint a record is when it is forgotten.
        forgotten: f32 = FORGOTTEN,
        /// How near two records of a place have to be to be of the same one, in meters.
        same_within: f32 = SAME_WITHIN,
        /// How much of what it is told it takes in.
        told: f32 = TOLD,
        /// How far a record's outcome moves to a new one's, each time it is noted again.
        outcome_rate: f32 = OUTCOME_RATE,
        /// How much longer the most important record lasts, on top of its kind's half-life.
        importance_hold: f32 = IMPORTANCE_HOLD,
        /// How faint a record has to be to be merged with others like it, and how near them.
        consolidate_below: f32 = CONSOLIDATE_BELOW,
        consolidate_within: f32 = CONSOLIDATE_WITHIN,
    }
);

impl Tuning {
    /// How long till a record of `kind` is half as strong, in seconds.
    pub fn half_life(&self, kind: Kind) -> f32 {
        match kind {
            Kind::Place => self.place_half_life,
            Kind::Character => self.character_half_life,
            Kind::Encounter => self.encounter_half_life,
            Kind::Event => self.event_half_life,
            Kind::Danger => self.danger_half_life,
            Kind::Group => self.group_half_life,
            Kind::Investigation => self.investigation_half_life,
        }
    }
}

/// One thing remembered.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Record<T = ()> {
    pub kind: Kind,
    /// Where it was, if anywhere in particular.
    pub at: Option<Vector3<f32>>,
    /// Who it was about, by the game's own index or id, if anyone.
    pub who: Option<u32>,
    /// What it was, by the game's own kinds, if the game tells them apart.
    pub what: Option<T>,
    /// How it turned out, from -1 (as badly as it could) to 1 (as well), if that is the point of
    /// it: an investigation that found the player, 1; that came to nothing, less than nothing.
    pub outcome: Option<f32>,
    /// How strongly it is remembered, from 0 to 1.
    pub strength: f32,
    /// Whether it saw it for itself, or was told.
    pub firsthand: bool,
    /// How many times it has been noted.
    pub times: u32,
    /// How long since it was last noted, in seconds.
    pub age: f32,
    /// How much it matters, from 0 to 1: the more, the slower it fades and the more it counts when
    /// recalled.
    #[cfg_attr(feature = "serde", serde(default))]
    pub importance: f32,
    /// How strong it was when last noted: `strength` is worked out afresh from it and `age`
    /// each step, rather than faded a little at a time, so that rounding does not build up over
    /// hours of short frames.
    noted: f32,
}

impl<T> Record<T> {
    /// A record of `kind`, as strong as can be, seen for itself, of nothing in particular yet.
    pub fn new(kind: Kind) -> Self {
        Self { kind, at: None, who: None, what: None, outcome: None, strength: 1.0, firsthand: true, times: 1, age: 0.0, importance: 0.0, noted: 1.0 }
    }

    /// Where it was.
    pub fn at(self, at: Vector3<f32>) -> Self {
        Self { at: Some(at), ..self }
    }

    /// Who it was about.
    pub fn who(self, who: u32) -> Self {
        Self { who: Some(who), ..self }
    }

    /// What it was.
    pub fn what(self, what: T) -> Self {
        Self { what: Some(what), ..self }
    }

    /// How it turned out, from -1 to 1.
    pub fn outcome(self, outcome: f32) -> Self {
        Self { outcome: Some(outcome), ..self }
    }

    /// As strong as `strength`, from 0 to 1, rather than as strong as can be.
    pub fn strength(self, strength: f32) -> Self {
        Self { strength, ..self }
    }

    /// Told of it, rather than seeing it for itself.
    pub fn secondhand(self) -> Self {
        Self { firsthand: false, ..self }
    }

    /// Mattering `importance`, from 0 to 1.
    pub fn importance(self, importance: f32) -> Self {
        Self { importance, ..self }
    }
}

/// What an NPC is trying to remember: of which kinds - any, if none are given - near where and
/// how near, about whom, of what. Anything left out asks nothing of a record.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cue<'a, T = ()> {
    pub kinds: &'a [Kind],
    pub at: Option<Vector3<f32>>,
    /// How far from `at` a record can be and still be of it, in meters.
    pub within: f32,
    pub who: Option<u32>,
    pub what: Option<T>,
}

impl<T> Default for Cue<'_, T> {
    fn default() -> Self {
        Self { kinds: &[], at: None, within: SAME_WITHIN, who: None, what: None }
    }
}

impl<'a, T> Cue<'a, T> {
    /// Of `kinds`, and nothing else asked.
    pub fn of(kinds: &'a [Kind]) -> Self {
        Self { kinds, ..Self::default() }
    }

    /// Within `within` meters of `at`.
    pub fn near(self, at: Vector3<f32>, within: f32) -> Self {
        Self { at: Some(at), within, ..self }
    }

    /// About `who`.
    pub fn about(self, who: u32) -> Self {
        Self { who: Some(who), ..self }
    }

    /// Of `what`.
    pub fn what(self, what: T) -> Self {
        Self { what: Some(what), ..self }
    }
}

/// What one NPC remembers: a record of each thing, by its kind, fading.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Memory<T = ()> {
    records: Vec<Record<T>>,
}

impl<T> Default for Memory<T> {
    fn default() -> Self {
        Self { records: Vec::new() }
    }
}

impl<T: Copy + PartialEq> Memory<T> {
    /// Nothing remembered.
    pub fn new() -> Self {
        Self::default()
    }

    /// It notes `record`: the record it has of the same thing - the same kind, about the same
    /// one, what it was the same, within the tuning's `same_within` of the same place - the
    /// stronger for it, brought back to fresh, its outcome moved toward this one's; or else a new
    /// one, the faintest of its kind forgotten if it has as many as it keeps. A record with no
    /// sense in it - a place that is not one, a strength or outcome not a number - is let go.
    pub fn note(&mut self, tuning: &Tuning, record: Record<T>) {
        if record.at.is_some_and(|at| !at.iter().all(|v| v.is_finite())) || !record.strength.is_finite() || record.outcome.is_some_and(|o| !o.is_finite()) {
            return;
        }
        let strength = record.strength.clamp(0.0, 1.0);
        let outcome = record.outcome.map(|o| o.clamp(-1.0, 1.0));
        let importance = if record.importance.is_finite() { record.importance.clamp(0.0, 1.0) } else { 0.0 };
        if strength < tuning.forgotten {
            return;
        }
        let same = |had: &Record<T>| {
            had.kind == record.kind
                && had.who == record.who
                && had.what == record.what
                && match (had.at, record.at) {
                    (Some(a), Some(b)) => (a - b).norm() <= tuning.same_within,
                    (None, None) => true,
                    _ => false,
                }
        };
        if let Some(had) = self.records.iter_mut().find(|had| same(had)) {
            had.strength = 1.0 - (1.0 - had.strength) * (1.0 - strength);
            had.at = record.at.or(had.at);
            had.outcome = match (had.outcome, outcome) {
                (Some(was), Some(now)) => Some(was + (now - was) * tuning.outcome_rate.clamp(0.0, 1.0)),
                (was, now) => now.or(was),
            };
            had.firsthand |= record.firsthand;
            had.importance = had.importance.max(importance);
            had.times = had.times.saturating_add(1);
            had.age = 0.0;
            had.noted = had.strength;
            return;
        }
        let most = tuning.most;
        if most == 0 {
            return;
        }
        let full = |records: &[Record<T>]| records.iter().filter(|r| r.kind == record.kind).count() >= most;
        // Full: what it can sum up, it sums up - however strong, rather than forget anything.
        if full(&self.records) {
            self.consolidate_kind(tuning, record.kind, f32::INFINITY);
        }
        if full(&self.records) {
            let faintest = self
                .records
                .iter()
                .enumerate()
                .filter(|(_, r)| r.kind == record.kind)
                .min_by(|a, b| a.1.strength.total_cmp(&b.1.strength))
                .map(|(n, r)| (n, r.strength));
            match faintest {
                Some((n, faint)) if faint <= strength => {
                    self.records.remove(n);
                }
                // Fainter than everything it keeps of the kind: not worth a place.
                _ => return,
            }
        }
        self.records.push(Record { strength, outcome, importance, age: 0.0, times: 1, noted: strength, ..record });
    }

    /// Moves it on `dt` seconds: each record fading by its kind's half-life, forgotten once under
    /// the tuning's `forgotten`.
    pub fn step(&mut self, tuning: &Tuning, dt: f32) {
        let dt = if dt.is_finite() { dt.max(0.0) } else { 0.0 };
        if dt == 0.0 {
            return;
        }
        for record in &mut self.records {
            record.age += dt;
            let half_life = tuning.half_life(record.kind) * (1.0 + tuning.importance_hold.max(0.0) * record.importance);
            record.strength = super::fade(record.noted, half_life, record.age);
        }
        self.records.retain(|r| r.strength >= tuning.forgotten);
    }

    /// Everything it remembers of `kind`, strongest first.
    pub fn recall(&self, kind: Kind) -> Vec<&Record<T>> {
        self.sorted(|r| r.kind == kind)
    }

    /// What it remembers of `kind` within `within` meters of `at`, strongest first.
    pub fn near(&self, kind: Kind, at: Vector3<f32>, within: f32) -> Vec<&Record<T>> {
        self.sorted(|r| r.kind == kind && r.at.is_some_and(|there| (there - at).norm() <= within))
    }

    /// Everything it remembers about `who`, strongest first.
    pub fn about(&self, who: u32) -> Vec<&Record<T>> {
        self.sorted(|r| r.who == Some(who))
    }

    /// How strongly it remembers anything of `kind` within `within` meters of `at`: the strongest
    /// such record's strength, 0 if none.
    pub fn strength_near(&self, kind: Kind, at: Vector3<f32>, within: f32) -> f32 {
        self.records
            .iter()
            .filter(|r| r.kind == kind && r.at.is_some_and(|there| (there - at).norm() <= within))
            .map(|r| r.strength)
            .fold(0.0, f32::max)
    }

    /// It tells `to` what it remembers of the kinds `kinds`: each record noted by them as
    /// secondhand, as strong as the tuning's `told` share of how strongly it remembers it.
    pub fn tell(&self, tuning: &Tuning, to: &mut Memory<T>, kinds: &[Kind]) {
        for record in self.records.iter().filter(|r| kinds.contains(&r.kind)) {
            let strength = record.strength * tuning.told.clamp(0.0, 1.0);
            to.note(tuning, Record { firsthand: false, strength, noted: strength, ..*record });
        }
    }

    /// How well `record` fits `cue`, from 0 (not at all) up: how strongly it is remembered, times
    /// how near the place asked about (1 at it, a half at the cue's `within`, nothing past it; a
    /// half for a record of nowhere in particular), whether it is about the one asked after (a half
    /// if about no one in particular, nothing if about another), whether it is what was asked
    /// about (the same), and how much it matters (from a half, mattering nothing, to 1).
    pub fn relevance(&self, cue: &Cue<'_, T>, record: &Record<T>) -> f32 {
        if !cue.kinds.is_empty() && !cue.kinds.contains(&record.kind) {
            return 0.0;
        }
        let place = match (cue.at, record.at) {
            (Some(a), Some(b)) => {
                let d = (a - b).norm();
                let within = if cue.within.is_finite() { cue.within.max(1.0e-3) } else { f32::MAX };
                if d > within {
                    return 0.0;
                }
                1.0 - 0.5 * d / within
            }
            (Some(_), None) => 0.5,
            _ => 1.0,
        };
        let who = match (cue.who, record.who) {
            (Some(a), Some(b)) if a != b => return 0.0,
            (Some(_), None) => 0.5,
            _ => 1.0,
        };
        let what = match (cue.what, record.what) {
            (Some(a), Some(b)) if a != b => return 0.0,
            (Some(_), None) => 0.5,
            _ => 1.0,
        };
        record.strength * place * who * what * (0.5 + 0.5 * record.importance)
    }

    /// The `most` records that fit `cue` best, best first, each with how well (see
    /// [`Memory::relevance`]) - none that does not fit at all.
    pub fn recall_for(&self, cue: &Cue<'_, T>, most: usize) -> Vec<(f32, &Record<T>)> {
        let mut found: Vec<(f32, &Record<T>)> = self.records.iter().map(|r| (self.relevance(cue, r), r)).filter(|&(score, _)| score > 0.0).collect();
        if found.len() > most {
            found.select_nth_unstable_by(most, |a, b| b.0.total_cmp(&a.0));
            found.truncate(most);
        }
        found.sort_by(|a, b| b.0.total_cmp(&a.0));
        found
    }

    /// Sums up faint records of the same thing close together, of every kind (see the module):
    /// how many records fewer it has for it.
    pub fn consolidate(&mut self, tuning: &Tuning) -> usize {
        let before = self.records.len();
        for kind in Kind::ALL {
            self.consolidate_kind(tuning, kind, tuning.consolidate_below);
        }
        before - self.records.len()
    }

    /// Sums up the records of `kind` fainter than `below` (see [`Memory::consolidate`]).
    fn consolidate_kind(&mut self, tuning: &Tuning, kind: Kind, below: f32) {
        let faint = |r: &Record<T>| r.kind == kind && r.at.is_some() && r.strength < below;
        // Strongest first: each gathers in the faint ones round it not gathered already.
        let mut order: Vec<usize> = (0..self.records.len()).filter(|&n| faint(&self.records[n])).collect();
        order.sort_by(|&a, &b| self.records[b].strength.total_cmp(&self.records[a].strength));
        let mut gone = vec![false; self.records.len()];
        for (i, &n) in order.iter().enumerate() {
            if gone[n] {
                continue;
            }
            let centre = self.records[n].at.expect("faint records have a place");
            let group: Vec<usize> = std::iter::once(n)
                .chain(order[i + 1..].iter().copied().filter(|&m| {
                    let (a, b) = (&self.records[n], &self.records[m]);
                    !gone[m] && a.who == b.who && a.what == b.what && b.at.is_some_and(|at| (at - centre).norm() <= tuning.consolidate_within)
                }))
                .collect();
            if group.len() < 2 {
                continue;
            }
            let weight: f32 = group.iter().map(|&m| self.records[m].strength).sum::<f32>().max(1.0e-6);
            let at = group.iter().map(|&m| self.records[m].at.expect("placed") * self.records[m].strength).sum::<Vector3<f32>>() / weight;
            let strength = 1.0 - group.iter().map(|&m| 1.0 - self.records[m].strength).product::<f32>();
            let outcomes: Vec<(f32, f32)> = group.iter().filter_map(|&m| self.records[m].outcome.map(|o| (o, self.records[m].strength))).collect();
            let outcome = (!outcomes.is_empty()).then(|| {
                outcomes.iter().map(|(o, w)| o * w).sum::<f32>() / outcomes.iter().map(|(_, w)| w).sum::<f32>().max(1.0e-6)
            });
            let merged = Record {
                at: Some(at),
                strength,
                noted: strength,
                outcome,
                firsthand: group.iter().any(|&m| self.records[m].firsthand),
                importance: group.iter().map(|&m| self.records[m].importance).fold(0.0, f32::max),
                times: group.iter().map(|&m| self.records[m].times).fold(0u32, |a, t| a.saturating_add(t)),
                age: 0.0,
                ..self.records[n]
            };
            for &m in &group[1..] {
                gone[m] = true;
            }
            self.records[n] = merged;
        }
        let mut n = 0;
        self.records.retain(|_| {
            n += 1;
            !gone[n - 1]
        });
    }

    /// How many records it has.
    pub fn len(&self) -> usize {
        self.records.len()
    }

    /// Whether it remembers nothing.
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// Every record, in the order noted.
    pub fn records(&self) -> &[Record<T>] {
        &self.records
    }

    fn sorted(&self, keep: impl Fn(&Record<T>) -> bool) -> Vec<&Record<T>> {
        let mut found: Vec<&Record<T>> = self.records.iter().filter(|r| keep(r)).collect();
        found.sort_by(|a, b| b.strength.total_cmp(&a.strength));
        found
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T: Tuning = Tuning::DEFAULT;

    fn at(x: f32) -> Vector3<f32> {
        Vector3::new(x, 0.0, 0.0)
    }

    #[derive(Debug, Clone, Copy, PartialEq)]
    enum Saw {
        Shooting,
        Robbery,
    }

    #[test]
    fn every_kind_the_roadmap_names_can_be_kept() {
        let mut memory = Memory::<Saw>::new();
        memory.note(&T, Record::new(Kind::Place).at(at(1.0)));
        memory.note(&T, Record::new(Kind::Character).who(7));
        memory.note(&T, Record::new(Kind::Encounter).who(0).at(at(5.0)));
        memory.note(&T, Record::new(Kind::Event).what(Saw::Shooting).at(at(9.0)));
        memory.note(&T, Record::new(Kind::Danger).at(at(9.0)));
        memory.note(&T, Record::new(Kind::Group).who(7).what(Saw::Robbery));
        memory.note(&T, Record::new(Kind::Investigation).at(at(20.0)).outcome(1.0));
        memory.note(&T, Record::new(Kind::Investigation).at(at(40.0)).outcome(-0.5));
        assert_eq!(memory.len(), 8);
        for kind in Kind::ALL {
            assert!(!memory.recall(kind).is_empty(), "{kind:?}");
        }
        let looked: Vec<f32> = memory.recall(Kind::Investigation).iter().filter_map(|r| r.outcome).collect();
        assert!(looked.contains(&1.0) && looked.contains(&-0.5), "successful and failed investigations both");
    }

    #[test]
    fn noting_the_same_thing_again_strengthens_it_rather_than_adding_another() {
        let mut memory = Memory::<()>::new();
        memory.note(&T, Record::new(Kind::Place).at(at(10.0)).strength(0.5));
        memory.step(&T, 600.0);
        let faded = memory.recall(Kind::Place)[0].strength;
        memory.note(&T, Record::new(Kind::Place).at(at(11.0)).strength(0.5));
        assert_eq!(memory.len(), 1, "within same_within: the same place");
        let place = memory.recall(Kind::Place)[0];
        assert!(place.strength > faded && place.strength > 0.5, "{faded} -> {}", place.strength);
        assert_eq!((place.times, place.age, place.at), (2, 0.0, Some(at(11.0))));
        // Elsewhere, or of something else, is another record.
        memory.note(&T, Record::new(Kind::Place).at(at(20.0)));
        memory.note(&T, Record::new(Kind::Danger).at(at(11.0)));
        assert_eq!(memory.len(), 3);
    }

    #[test]
    fn an_outcome_noted_again_moves_toward_the_new_one() {
        let mut memory = Memory::<()>::new();
        memory.note(&T, Record::new(Kind::Investigation).at(at(3.0)).outcome(-1.0));
        memory.note(&T, Record::new(Kind::Investigation).at(at(3.0)).outcome(1.0));
        assert_eq!(memory.recall(Kind::Investigation)[0].outcome, Some(0.0));
    }

    #[test]
    fn each_kind_fades_by_its_own_half_life_and_is_forgotten_once_faint() {
        let mut memory = Memory::<()>::new();
        memory.note(&T, Record::new(Kind::Investigation).at(at(1.0)));
        memory.note(&T, Record::new(Kind::Group).who(3));
        memory.step(&T, INVESTIGATION_HALF_LIFE);
        assert!((memory.recall(Kind::Investigation)[0].strength - 0.5).abs() < 1.0e-5);
        assert!(memory.recall(Kind::Group)[0].strength > 0.95, "a group is remembered far longer");
        memory.step(&T, INVESTIGATION_HALF_LIFE * 5.0);
        assert!(memory.recall(Kind::Investigation).is_empty(), "forgotten");
        assert_eq!(memory.len(), 1);
    }

    #[test]
    fn it_fades_the_same_in_one_long_step_as_in_many_short_ones() {
        let mut once = Memory::<()>::new();
        for kind in Kind::ALL {
            once.note(&T, Record::new(kind).at(at(kind as u8 as f32 * 10.0)));
        }
        let mut often = once.clone();
        once.step(&T, 900.0);
        for _ in 0..9000 {
            often.step(&T, 0.1);
        }
        assert_eq!(once.len(), often.len());
        for (a, b) in once.records().iter().zip(often.records()) {
            assert!((a.strength - b.strength).abs() < 1.0e-4 && (a.age - b.age).abs() < 0.2, "{a:?} {b:?}");
        }
    }

    #[test]
    fn it_keeps_only_so_many_of_a_kind_the_faintest_going_first() {
        let tuning = Tuning { most: 4, ..T };
        let mut memory = Memory::<()>::new();
        for n in 0..4 {
            memory.note(&tuning, Record::new(Kind::Encounter).at(at(n as f32 * 10.0)).strength(0.2 + n as f32 * 0.1));
        }
        memory.note(&tuning, Record::new(Kind::Danger).at(at(0.0)).strength(0.1));
        // A stronger one takes the faintest's place; a fainter one is not kept.
        memory.note(&tuning, Record::new(Kind::Encounter).at(at(100.0)).strength(0.9));
        memory.note(&tuning, Record::new(Kind::Encounter).at(at(200.0)).strength(0.1));
        let kept: Vec<f32> = memory.recall(Kind::Encounter).iter().map(|r| r.strength).collect();
        assert_eq!(kept, vec![0.9, 0.5, 0.4, 0.3]);
        assert_eq!(memory.recall(Kind::Danger).len(), 1, "other kinds keep their own places");
    }

    #[test]
    fn what_is_recalled_comes_strongest_first_by_place_and_by_who() {
        let mut memory = Memory::<()>::new();
        memory.note(&T, Record::new(Kind::Danger).at(at(0.0)).strength(0.3));
        memory.note(&T, Record::new(Kind::Danger).at(at(5.0)).strength(0.8));
        memory.note(&T, Record::new(Kind::Danger).at(at(50.0)).strength(1.0));
        memory.note(&T, Record::new(Kind::Character).who(4).strength(0.6));
        memory.note(&T, Record::new(Kind::Encounter).who(4).at(at(5.0)).strength(0.9));
        let near: Vec<f32> = memory.near(Kind::Danger, at(2.0), 10.0).iter().map(|r| r.strength).collect();
        assert_eq!(near, vec![0.8, 0.3]);
        assert_eq!(memory.strength_near(Kind::Danger, at(2.0), 10.0), 0.8);
        assert_eq!(memory.strength_near(Kind::Danger, at(30.0), 10.0), 0.0);
        let about: Vec<Kind> = memory.about(4).iter().map(|r| r.kind).collect();
        assert_eq!(about, vec![Kind::Encounter, Kind::Character]);
    }

    #[test]
    fn what_it_is_told_is_secondhand_and_fainter() {
        let mut veteran = Memory::<()>::new();
        veteran.note(&T, Record::new(Kind::Danger).at(at(7.0)));
        veteran.note(&T, Record::new(Kind::Encounter).at(at(9.0)));
        let mut rookie = Memory::new();
        veteran.tell(&T, &mut rookie, &[Kind::Danger]);
        let told = rookie.recall(Kind::Danger);
        assert_eq!(told.len(), 1);
        assert!(!told[0].firsthand && (told[0].strength - TOLD).abs() < 1.0e-6);
        assert!(rookie.recall(Kind::Encounter).is_empty(), "only what it was told of");
        // Seeing it for itself afterwards makes it its own.
        rookie.note(&T, Record::new(Kind::Danger).at(at(7.5)));
        assert!(rookie.recall(Kind::Danger)[0].firsthand);
    }

    #[test]
    fn records_with_no_sense_in_them_change_nothing() {
        let mut memory = Memory::<()>::new();
        memory.note(&T, Record::new(Kind::Place).at(at(1.0)));
        let before = memory.clone();
        memory.note(&T, Record::new(Kind::Place).at(Vector3::new(f32::NAN, 0.0, 0.0)));
        memory.note(&T, Record::new(Kind::Danger).strength(f32::NAN));
        memory.note(&T, Record::new(Kind::Investigation).outcome(f32::INFINITY));
        memory.note(&T, Record::new(Kind::Event).strength(0.0));
        memory.step(&T, f32::NAN);
        memory.step(&T, -5.0);
        assert_eq!(memory, before);
        memory.note(&Tuning { most: 0, ..T }, Record::new(Kind::Group).who(1));
        assert_eq!(memory, before, "keeping none, it keeps none");
    }

    #[test]
    fn what_matters_more_is_remembered_longer() {
        let mut memory = Memory::<()>::new();
        memory.note(&T, Record::new(Kind::Danger).at(at(0.0)));
        memory.note(&T, Record::new(Kind::Danger).at(at(50.0)).importance(1.0));
        memory.step(&T, DANGER_HALF_LIFE);
        let near = |x: f32| memory.near(Kind::Danger, at(x), 1.0)[0].strength;
        assert!((near(0.0) - 0.5).abs() < 1.0e-4);
        assert!((near(50.0) - 0.5f32.powf(1.0 / (1.0 + IMPORTANCE_HOLD))).abs() < 1.0e-4, "four times as long: {}", near(50.0));
    }

    #[test]
    fn what_fits_the_cue_comes_first_not_merely_what_is_strongest() {
        let mut memory = Memory::<Saw>::new();
        memory.note(&T, Record::new(Kind::Danger).at(at(100.0)).strength(1.0));
        memory.note(&T, Record::new(Kind::Danger).at(at(2.0)).strength(0.6));
        memory.note(&T, Record::new(Kind::Danger).at(at(9.0)).strength(0.9));
        memory.note(&T, Record::new(Kind::Event).at(at(1.0)).what(Saw::Shooting));
        memory.note(&T, Record::new(Kind::Event).at(at(1.5)).what(Saw::Robbery).who(3));
        // Dangers about here: the near one before the stronger one further off; the far one not at all.
        let dangers = Cue::of(&[Kind::Danger]).near(at(0.0), 10.0);
        let found: Vec<f32> = memory.recall_for(&dangers, 5).iter().map(|(_, r)| r.at.unwrap().x).collect();
        assert_eq!(found, vec![2.0, 9.0]);
        // A shooting seen about here: the shooting, not the robbery.
        let shooting = Cue::of(&[Kind::Event]).near(at(0.0), 10.0).what(Saw::Shooting);
        let found = memory.recall_for(&shooting, 5);
        assert_eq!(found.iter().map(|(_, r)| r.what).collect::<Vec<_>>(), vec![Some(Saw::Shooting)]);
        // Anything about a shooting here: a danger nearby, of nothing in particular, counts half.
        let any = Cue::default().near(at(0.0), 10.0).what(Saw::Shooting);
        let found = memory.recall_for(&any, 5);
        assert_eq!(found[0].1.what, Some(Saw::Shooting));
        assert!(found[1..].iter().all(|(score, r)| r.kind == Kind::Danger && *score <= 0.5 * found[0].0 + 1.0e-6), "{found:?}");
        // About one in particular, only theirs.
        let theirs = Cue::of(&[Kind::Event]).about(3);
        let found = memory.recall_for(&theirs, 5);
        assert_eq!(found.len(), 2, "theirs, and one about no one in particular, at half");
        assert_eq!(found[0].1.who, Some(3));
        assert!(found[1].0 < found[0].0);
        // Asked for one, it gives the best one.
        assert_eq!(memory.recall_for(&Cue::default(), 1).len(), 1);
        assert!(memory.recall_for(&Cue::of(&[Kind::Group]), 5).is_empty());
    }

    #[test]
    fn many_faint_glimpses_about_one_place_become_one_memory_of_it() {
        // Six faint glimpses round a courtyard, each 3.5 m from the middle: too far apart to be
        // the same glimpse (`same_within`), near enough to be the same place (`consolidate_within`).
        let mut memory = Memory::<()>::new();
        for n in 0..6 {
            let angle = n as f32 * std::f32::consts::TAU / 6.0;
            memory.note(&T, Record::new(Kind::Encounter).at(Vector3::new(3.5 * angle.cos(), 0.0, 3.5 * angle.sin())).strength(0.3));
        }
        memory.note(&T, Record::new(Kind::Encounter).at(at(40.0)).strength(0.3));
        memory.note(&T, Record::new(Kind::Encounter).at(at(2.0)).who(9).strength(0.3));
        memory.note(&T, Record::new(Kind::Encounter).at(at(0.0)).strength(0.9));
        let before = memory.len();
        let fewer = memory.consolidate(&T);
        assert_eq!((before, fewer), (9, 5));
        let merged = memory.recall(Kind::Encounter).into_iter().find(|r| r.times == 6).expect("one of six");
        assert!(merged.strength > 0.85, "as strong as all of them together: {}", merged.strength);
        assert!(merged.at.unwrap().norm() < 1.0e-4, "where they were on average: {:?}", merged.at);
        // Far off, about someone else, or strong already: left as they were.
        assert_eq!(memory.near(Kind::Encounter, at(40.0), 1.0).len(), 1);
        assert_eq!(memory.about(9).len(), 1);
        assert!(memory.recall(Kind::Encounter).iter().any(|r| r.strength == 0.9 && r.times == 1));
    }

    #[test]
    fn a_full_memory_sums_up_before_it_forgets() {
        // Twelve faint glimpses, in two places, each 4 m or more from the others there: more than
        // it keeps of the kind.
        let tuning = Tuning { most: 4, ..T };
        let spots = [(0.0, 0.0), (4.0, 0.0), (0.0, 4.0), (4.0, 4.0), (-4.0, 0.0), (0.0, -4.0)];
        let mut memory = Memory::<()>::new();
        for (n, &(x, z)) in spots.iter().chain(&spots).enumerate() {
            let x = x + if n < 6 { 0.0 } else { 30.0 };
            memory.note(&tuning, Record::new(Kind::Encounter).at(Vector3::new(x, 0.0, z)).strength(0.3));
        }
        assert!(memory.len() <= 4);
        let seen: u32 = memory.recall(Kind::Encounter).iter().map(|r| r.times).sum();
        assert_eq!(seen, 12, "every glimpse counted in some record, none simply forgotten");
    }

    #[test]
    fn a_long_life_stays_bounded() {
        let mut memory = Memory::<u8>::new();
        let mut seed = 3u32;
        for n in 0..50_000u32 {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let kind = Kind::ALL[(seed >> 8) as usize % 7];
            memory.note(&T, Record::new(kind).at(at((seed >> 12) as f32 % 500.0)).what((n % 3) as u8).strength(((seed >> 16) % 100) as f32 / 100.0));
            memory.step(&T, 0.5);
        }
        assert!(memory.len() <= MOST * 7, "{}", memory.len());
        for kind in Kind::ALL {
            assert!(memory.recall(kind).len() <= MOST);
        }
    }
}
