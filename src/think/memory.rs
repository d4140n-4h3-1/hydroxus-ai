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
    /// How strong it was when last noted: `strength` is worked out afresh from it and `age`
    /// each step, rather than faded a little at a time, so that rounding does not build up over
    /// hours of short frames.
    noted: f32,
}

impl<T> Record<T> {
    /// A record of `kind`, as strong as can be, seen for itself, of nothing in particular yet.
    pub fn new(kind: Kind) -> Self {
        Self { kind, at: None, who: None, what: None, outcome: None, strength: 1.0, firsthand: true, times: 1, age: 0.0, noted: 1.0 }
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
            had.times = had.times.saturating_add(1);
            had.age = 0.0;
            had.noted = had.strength;
            return;
        }
        let most = tuning.most;
        if most == 0 {
            return;
        }
        let of_kind = self.records.iter().filter(|r| r.kind == record.kind).count();
        if of_kind >= most {
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
        self.records.push(Record { strength, outcome, age: 0.0, times: 1, noted: strength, ..record });
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
            record.strength = super::fade(record.noted, tuning.half_life(record.kind), record.age);
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
