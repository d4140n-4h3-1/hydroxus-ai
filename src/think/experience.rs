//! What an NPC has been through: one place for a game to say how what it did turned out, and for
//! that to go wherever in [`crate::think`] it should.
//!
//! An [`Outcome`] says what the NPC was trying ([`Venture`]: looking into something it noticed,
//! after someone, keeping itself safe, working with another, searching), how it went - from -1 (as
//! badly as it could) to 1 (as well), not just one or the other: a search that found the player, 1;
//! one that came to nothing, a half the other way ([`UNPRODUCTIVE`]); a chase that kept them in
//! sight till others came, somewhere between - and, if the game knows them, where the player was
//! and what the game makes of what they did.
//!
//! [`Experience::report`] takes it to each part it is for:
//!
//! - its [`Lessons`], through its [`Trail`]: only what it did lately that the venture is down to
//!   ([`Venture::covers`]) learns from it, each by how lately - a shelter it took from a shootout
//!   learns nothing from a search that came to nothing;
//! - its [`Haunts`], given `at` and a navmesh: where a search or a look found the player hiding,
//!   or where they were seen, or got away;
//! - its [`Habits`], given what the player did.
//!
//! What it learned then weighs what it chooses ([`Experience::choose`]): its role's reasoner, each
//! conduct times what it makes of it, but never one its situation shuts. With the lesson tuning's
//! `sway` at nothing, it chooses just as the reasoner alone would: learning switched off.
//!
//! Each NPC keeps its own: two sentries through the same streets come to their own ways for what
//! happened to each. One tells another only when the game says so ([`Experience::tell`]), weaker
//! for being told. Nothing in it is random, and everything it keeps is bounded - its lessons by how
//! many conducts there are, its trail by [`super::lesson::TRAIL`], its haunts by the haunt
//! tuning's `most`, its habits by the game's own kinds - and fades with time.

use crate::navigation::navmesh::NavMesh;
use crate::tactics::conduct::{Conduct, Reading, Reasoners, Role};
use crate::think::{
    habit::Habits,
    haunt::Haunts,
    lesson::{Lessons, Trail, Verdict},
};
use nalgebra::Vector3;

/// How a venture that came to nothing went: neither a success nor a failure, but no use - a
/// search that turned up no one, a look that found nothing there.
pub const UNPRODUCTIVE: f32 = -0.5;

/// What an NPC was trying to do, that turned out one way or another.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Venture {
    /// Looking into what it noticed.
    Investigation,
    /// After someone: the player, or one of the other side.
    Pursuit,
    /// Keeping itself safe: sheltering, backing off, shooting back.
    Defence,
    /// Working with another: telling a sentry, keeping by its partner.
    Cooperation,
    /// Searching for the player, lost.
    Search,
}

impl Venture {
    /// Whether how this venture turned out is down to doing `conduct`.
    pub fn covers(self, conduct: Conduct) -> bool {
        use Conduct::*;
        matches!(
            (self, conduct),
            (Venture::Investigation, Investigate)
                | (Venture::Pursuit, Chase | Fight)
                | (Venture::Defence, Shelter | BackAway | ShootBack)
                | (Venture::Cooperation, Report | Follow)
                | (Venture::Search, Search)
        )
    }
}

/// How something an NPC tried turned out, as the game tells it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Outcome<T = ()> {
    /// What it was trying to do.
    pub venture: Venture,
    /// How it went, from -1 (as badly as it could) to 1 (as well).
    pub went: f32,
    /// Where the player was: found, seen, or last seen before they got away.
    pub at: Option<Vector3<f32>>,
    /// What the player did, by the game's own kinds: the way they got away, say.
    pub player_did: Option<T>,
}

impl<T> Outcome<T> {
    /// `venture` went `went`, from -1 to 1.
    pub fn new(venture: Venture, went: f32) -> Self {
        Self { venture, went, at: None, player_did: None }
    }

    /// `venture` went as well as it could: a search or a look that found the player, a chase
    /// that caught them, a shelter that kept it whole, a report that brought help.
    pub fn succeeded(venture: Venture) -> Self {
        Self::new(venture, 1.0)
    }

    /// `venture` went as badly as it could: a chase the player got away from, a shelter it was
    /// shot down in, a report no one answered.
    pub fn failed(venture: Venture) -> Self {
        Self::new(venture, -1.0)
    }

    /// `venture` came to nothing: an unproductive search, a look that found nothing there.
    pub fn came_to_nothing(venture: Venture) -> Self {
        Self::new(venture, UNPRODUCTIVE)
    }

    /// `venture` got `share` of the way, from 0 (nowhere: as badly as it could) to 1 (all the way).
    pub fn partly(venture: Venture, share: f32) -> Self {
        Self::new(venture, share.clamp(0.0, 1.0) * 2.0 - 1.0)
    }

    /// The player was at `at`.
    pub fn at(self, at: Vector3<f32>) -> Self {
        Self { at: Some(at), ..self }
    }

    /// The player did `what`.
    pub fn player_did(self, what: T) -> Self {
        Self { player_did: Some(what), ..self }
    }
}

/// What one NPC has been through, and learned from it: what has worked for it, what it did
/// lately, the player's habits and where they turn up, as it has seen them.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Experience<T = ()> {
    /// What has worked for it.
    pub lessons: Lessons<Conduct>,
    /// What it did lately, for an outcome to be put down to.
    pub trail: Trail<Conduct>,
    /// The player's habits, as it has seen them.
    pub habits: Habits<T>,
    /// Where the player turns up, as it has seen it.
    pub haunts: Haunts,
}

impl<T> Default for Experience<T> {
    fn default() -> Self {
        Self { lessons: Lessons::default(), trail: Trail::default(), habits: Habits::default(), haunts: Haunts::default() }
    }
}

impl<T: Copy + Eq> Experience<T> {
    /// Nothing been through yet.
    pub fn new() -> Self {
        Self::default()
    }

    /// What it does, of `role`, in `reading`, doing `current` already: its role's reasoner
    /// weighed by what it learned, going about its business with nothing else open to it - and
    /// it notes it is doing it, for what comes of it.
    pub fn choose(&mut self, tuning: &crate::Tuning, reasoners: &mut Reasoners, role: Role, reading: &Reading, current: Option<Conduct>) -> Conduct {
        self.choose_with(tuning, reasoners, role, reading, current, |_| 1.0)
    }

    /// As [`Experience::choose`], each conduct weighed by `weigh` of it as well as by what it
    /// learned: by its priorities, say ([`crate::tactics::priority::weigh`]).
    pub fn choose_with(
        &mut self,
        tuning: &crate::Tuning,
        reasoners: &mut Reasoners,
        role: Role,
        reading: &Reading,
        current: Option<Conduct>,
        weigh: impl Fn(Conduct) -> f32,
    ) -> Conduct {
        let conduct = self.lessons.choose_with(&tuning.lesson, reasoners.of(role), reading, current, weigh).unwrap_or(Conduct::GoAbout);
        self.trail.doing(conduct);
        conduct
    }

    /// It is held to what it does, choosing nothing: down, carried, talking, in a foot chase.
    pub fn held(&mut self) {
        self.trail.stopped();
    }

    /// Something it tried turned out `outcome`: its lessons learn it, for what it did lately that
    /// is down to; where the player was goes into its haunts, given `mesh`; what the player did,
    /// into its habits - giving back what it changed its mind about for it. An outcome with no
    /// sense in it - how it went not a number, the player nowhere - is let go, and leaves what it
    /// learned as it was.
    pub fn report(&mut self, tuning: &crate::Tuning, mesh: Option<&NavMesh>, outcome: Outcome<T>) -> Vec<Verdict<Conduct>> {
        if !outcome.went.is_finite() {
            return Vec::new();
        }
        let verdicts = self.trail.turned_out(&tuning.lesson, &mut self.lessons, outcome.went, |conduct| outcome.venture.covers(conduct));
        if let (Some(at), Some(mesh)) = (outcome.at.filter(|at| at.iter().all(|v| v.is_finite())), mesh) {
            match outcome.venture {
                Venture::Search | Venture::Investigation if outcome.went > 0.0 => self.haunts.found_at(&tuning.haunt, mesh, at),
                Venture::Search | Venture::Investigation | Venture::Pursuit => self.haunts.seen_at(&tuning.haunt, mesh, at),
                Venture::Defence | Venture::Cooperation => (),
            }
        }
        if let Some(what) = outcome.player_did {
            self.habits.saw(what);
        }
        verdicts
    }

    /// Moves it on `dt` seconds: everything it learned fading, what it did lately further back -
    /// giving back the views it gave up.
    pub fn step(&mut self, tuning: &crate::Tuning, dt: f32) -> Vec<Verdict<Conduct>> {
        let dt = if dt.is_nan() { 0.0 } else { dt.max(0.0) };
        self.trail.step(&tuning.lesson, dt);
        self.habits.step(&tuning.habit, dt);
        self.haunts.step(&tuning.haunt, dt);
        self.lessons.step(&tuning.lesson, dt)
    }

    /// It tells `to` what it has been through: what has worked for it, the player's habits and
    /// where they turn up, each weaker for being told - giving back what `to` changed its mind
    /// about for it. What it did lately is its own.
    pub fn tell(&self, tuning: &crate::Tuning, to: &mut Experience<T>) -> Vec<Verdict<Conduct>> {
        self.habits.tell(&tuning.habit, &mut to.habits);
        self.haunts.tell(&tuning.haunt, &mut to.haunts);
        self.lessons.tell(&tuning.lesson, &mut to.lessons)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::navigation::grid::WalkGrid;
    use crate::think::lesson::{View, TRAIL};

    const T: crate::Tuning = crate::Tuning::DEFAULT;

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    enum Way {
        Roofs,
        Sewers,
    }

    /// Chasing and searching both open to it, as when it glimpses the player while searching.
    const CHASE_OR_SEARCH: Reading = Reading { hunting: true, searching: true, ..READING };
    /// Investigating and searching both open to it.
    const LOOK_OR_SEARCH: Reading = Reading { investigating: true, searching: true, ..READING };
    const READING: Reading = Reading {
        hunting: false,
        foe: false,
        shot_at: false,
        investigating: false,
        searching: false,
        reporting: false,
        sheltering: false,
        mourning: false,
        afraid: false,
        watching: false,
        avoiding: false,
        regarding: false,
        partnered: false,
        nerve: 0.5,
        wariness: 0.0,
    };

    fn sentry(experience: &mut Experience<Way>, reading: &Reading) -> Conduct {
        experience.choose(&T, &mut Reasoners::default(), Role::Sentry, reading, None)
    }

    /// It does `conduct` for a few seconds, and it turns out `outcome`.
    fn went(experience: &mut Experience<Way>, conduct: Conduct, outcome: Outcome<Way>) -> Vec<Verdict<Conduct>> {
        experience.trail.doing(conduct);
        experience.step(&T, 3.0);
        experience.report(&T, None, outcome)
    }

    fn corridor() -> NavMesh {
        let mut grid = WalkGrid::new(21, 1, 1.0);
        for x in 0..21 {
            grid.set(x, 0, true);
        }
        NavMesh::build(&grid, Vector3::zeros(), |x, _| (x / 2) as u8)
    }

    #[test]
    fn what_worked_it_comes_to_do_first() {
        let mut experience = Experience::new();
        assert_eq!(sentry(&mut experience, &LOOK_OR_SEARCH), Conduct::Investigate, "as its role has it");
        let mut verdicts = Vec::new();
        for _ in 0..3 {
            verdicts.extend(went(&mut experience, Conduct::Search, Outcome::succeeded(Venture::Search)));
        }
        assert_eq!(sentry(&mut experience, &LOOK_OR_SEARCH), Conduct::Search);
        assert_eq!(experience.lessons.view(Conduct::Search), Some(View::Works));
        assert_eq!(verdicts.iter().map(|v| (v.action, v.view)).collect::<Vec<_>>(), vec![(Conduct::Search, Some(View::Works))], "says so, once");
    }

    #[test]
    fn what_failed_it_comes_to_do_less_little_by_little() {
        let mut experience = Experience::new();
        let mut worths = vec![0.0];
        assert_eq!(sentry(&mut experience, &CHASE_OR_SEARCH), Conduct::Chase);
        went(&mut experience, Conduct::Chase, Outcome::failed(Venture::Pursuit));
        worths.push(experience.lessons.worth(Conduct::Chase));
        assert_eq!(sentry(&mut experience, &CHASE_OR_SEARCH), Conduct::Chase, "one lost chase is not enough");
        went(&mut experience, Conduct::Chase, Outcome::failed(Venture::Pursuit));
        worths.push(experience.lessons.worth(Conduct::Chase));
        assert_eq!(sentry(&mut experience, &CHASE_OR_SEARCH), Conduct::Search, "two are");
        for _ in 0..10 {
            went(&mut experience, Conduct::Chase, Outcome::failed(Venture::Pursuit));
            worths.push(experience.lessons.worth(Conduct::Chase));
        }
        // Each outcome moves it, each less than the last, never past as badly as it could.
        for pair in worths.windows(2) {
            assert!(pair[1] < pair[0] && pair[1] >= -1.0, "{worths:?}");
        }
        for steps in worths.windows(3) {
            assert!(steps[1] - steps[2] < steps[0] - steps[1], "{worths:?}");
        }
        // Chasing alone open to it, it chases still: weighed down, never barred.
        assert_eq!(sentry(&mut experience, &Reading { hunting: true, ..READING }), Conduct::Chase);
        // Nor does anything learned open what is shut.
        assert_eq!(sentry(&mut experience, &READING), Conduct::GoAbout);
    }

    #[test]
    fn a_venture_turns_out_by_degrees() {
        assert_eq!(Outcome::<()>::partly(Venture::Search, 0.75).went, 0.5);
        assert_eq!(Outcome::<()>::partly(Venture::Search, 7.0).went, 1.0);
        let mut found = Experience::new();
        let mut nothing = Experience::new();
        let mut lost = Experience::new();
        went(&mut found, Conduct::Search, Outcome::succeeded(Venture::Search));
        went(&mut nothing, Conduct::Search, Outcome::came_to_nothing(Venture::Search));
        went(&mut lost, Conduct::Search, Outcome::failed(Venture::Search));
        let (found, nothing, lost) = (found.lessons.worth(Conduct::Search), nothing.lessons.worth(Conduct::Search), lost.lessons.worth(Conduct::Search));
        assert!(found > 0.0 && nothing < 0.0 && lost < nothing, "{found} {nothing} {lost}");
    }

    #[test]
    fn what_it_did_before_counts_for_less_the_longer_ago() {
        let mut experience = Experience::<Way>::new();
        experience.trail.doing(Conduct::Chase);
        experience.trail.doing(Conduct::Investigate);
        experience.step(&T, T.lesson.credit);
        experience.trail.doing(Conduct::Fight);
        experience.report(&T, None, Outcome::succeeded(Venture::Pursuit));
        let (chase, fight) = (experience.lessons.worth(Conduct::Chase), experience.lessons.worth(Conduct::Fight));
        assert!((fight - T.lesson.rate).abs() < 1.0e-6, "what it is doing, all of it: {fight}");
        assert!((chase - T.lesson.rate / 2.0).abs() < 1.0e-6, "what it gave up a credit ago, half: {chase}");
        // Once put down to, a chase it gave up takes no more; the fight it is in does.
        experience.report(&T, None, Outcome::succeeded(Venture::Pursuit));
        assert_eq!(experience.lessons.worth(Conduct::Chase), chase);
        assert!(experience.lessons.worth(Conduct::Fight) > fight);
        // And held, choosing nothing, what it was doing is behind it, fading.
        experience.held();
        assert!(experience.trail.current().is_none());
        experience.step(&T, T.lesson.credit * 4.0);
        assert!(experience.trail.share(&T.lesson, Conduct::Fight) == 0.0);
    }

    #[test]
    fn two_through_the_same_come_to_their_own_ways() {
        let (mut one, mut other) = (Experience::new(), Experience::new());
        for _ in 0..4 {
            // The same streets, the same choice: one's searches find the player, the other's
            // investigations do, and the other's searches come to nothing.
            went(&mut one, Conduct::Search, Outcome::succeeded(Venture::Search));
            went(&mut other, Conduct::Investigate, Outcome::succeeded(Venture::Investigation));
            went(&mut other, Conduct::Search, Outcome::came_to_nothing(Venture::Search));
        }
        assert_eq!(sentry(&mut one, &LOOK_OR_SEARCH), Conduct::Search);
        assert_eq!(sentry(&mut other, &LOOK_OR_SEARCH), Conduct::Investigate);
        // Told, one comes some way to the other's mind - only when told, and weaker for it.
        let had = other.lessons.worth(Conduct::Search);
        one.tell(&T, &mut other);
        let told = other.lessons.worth(Conduct::Search);
        assert!(told > had && told < one.lessons.worth(Conduct::Search), "{had} {told}");
    }

    #[test]
    fn an_outcome_leaves_what_it_has_nothing_to_do_with_alone() {
        let mut experience = Experience::<Way>::new();
        experience.trail.doing(Conduct::Search);
        experience.trail.doing(Conduct::Shelter);
        experience.trail.doing(Conduct::Chase);
        experience.report(&T, None, Outcome::failed(Venture::Defence));
        assert!(experience.lessons.worth(Conduct::Shelter) < 0.0);
        assert_eq!(experience.lessons.worth(Conduct::Search), 0.0);
        assert_eq!(experience.lessons.worth(Conduct::Chase), 0.0);
        // The search is still there for its own outcome.
        experience.report(&T, None, Outcome::succeeded(Venture::Search));
        assert!(experience.lessons.worth(Conduct::Search) > 0.0);
        assert_eq!(experience.lessons.worth(Conduct::Chase), 0.0);
        // A report of what it never did teaches it nothing.
        let before = experience.lessons.clone();
        experience.report(&T, None, Outcome::succeeded(Venture::Cooperation));
        assert_eq!(experience.lessons, before);
        assert!(experience.habits.likeliest().is_none() && experience.haunts.likeliest(&corridor()).is_none());
    }

    #[test]
    fn where_the_player_was_and_what_they_did_go_to_its_haunts_and_habits() {
        let mesh = corridor();
        let at = |x: f32| Vector3::new(x + 0.5, 0.0, 0.5);
        let mut experience = Experience::new();
        experience.trail.doing(Conduct::Chase);
        experience.report(&T, Some(&mesh), Outcome::failed(Venture::Pursuit).at(at(15.0)).player_did(Way::Roofs));
        experience.trail.doing(Conduct::Search);
        experience.report(&T, Some(&mesh), Outcome::succeeded(Venture::Search).at(at(3.0)).player_did(Way::Roofs));
        // Seen where they got away, found where they hid: three times as much.
        let haunts: Vec<(f32, f32)> = experience.haunts.haunts(&mesh).map(|(p, w)| (p.x, w)).collect();
        assert_eq!(haunts, vec![(15.5, 1.0), (3.5, T.haunt.found)]);
        assert_eq!(experience.habits.likeliest(), Some((Way::Roofs, 1.0)));
        assert_eq!(experience.habits.times(Way::Sewers), 0.0);
        // Shelter is no place of the player's.
        experience.report(&T, Some(&mesh), Outcome::succeeded(Venture::Defence).at(at(9.0)));
        assert_eq!(experience.haunts.haunts(&mesh).count(), 2);
    }

    #[test]
    fn feedback_with_no_sense_in_it_changes_nothing() {
        let mesh = corridor();
        let mut experience = Experience::new();
        went(&mut experience, Conduct::Search, Outcome::succeeded(Venture::Search));
        let before = experience.clone();
        for went in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert!(experience.report(&T, Some(&mesh), Outcome::new(Venture::Search, went)).is_empty());
        }
        assert_eq!(experience, before);
        // The player nowhere: the lesson is learned, but no haunt for it.
        experience.report(&T, Some(&mesh), Outcome::succeeded(Venture::Search).at(Vector3::new(f32::NAN, 0.0, f32::INFINITY)));
        experience.report(&T, Some(&mesh), Outcome::succeeded(Venture::Search).at(Vector3::new(500.0, 0.0, 0.5)));
        assert_eq!(experience.haunts, before.haunts);
        // Beyond as well as it could is as well as it could; time that runs back, or is not a
        // number, is no time.
        experience.report(&T, None, Outcome::new(Venture::Search, 1.0e9));
        let worth = experience.lessons.worth(Conduct::Search);
        assert!(worth > 0.0 && worth <= 1.0);
        let before = experience.clone();
        experience.step(&T, f32::NAN);
        experience.step(&T, -100.0);
        assert_eq!(experience, before);
    }

    #[test]
    fn a_long_life_stays_within_bounds_and_comes_out_the_same_every_time() {
        let mesh = corridor();
        let life = || {
            let mut experience = Experience::new();
            let mut reasoners = Reasoners::default();
            let mut seed = 1u32;
            let mut next = move || {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                seed >> 8
            };
            let ventures = [Venture::Investigation, Venture::Pursuit, Venture::Defence, Venture::Cooperation, Venture::Search];
            let mut current = None;
            for _ in 0..100_000 {
                let bits = next();
                let reading = Reading {
                    hunting: bits & 1 != 0,
                    investigating: bits & 2 != 0,
                    searching: bits & 4 != 0,
                    sheltering: bits & 8 != 0,
                    reporting: bits & 16 != 0,
                    partnered: bits & 32 != 0,
                    ..READING
                };
                current = Some(experience.choose(&T, &mut reasoners, Role::Sentry, &reading, current));
                if bits & 0x700 == 0 {
                    let outcome = Outcome::new(ventures[(bits >> 11) as usize % 5], ((bits >> 14) % 201) as f32 / 100.0 - 1.0);
                    experience.report(&T, Some(&mesh), outcome.at(Vector3::new((bits >> 4) as f32 % 21.0, 0.0, 0.5)).player_did(if bits & 0x800 != 0 { Way::Roofs } else { Way::Sewers }));
                }
                experience.step(&T, 0.25);
                assert!(experience.lessons.learned().all(|(_, worth)| worth.is_finite() && (-1.0..=1.0).contains(&worth)));
                assert!(experience.lessons.learned().count() <= 14 && experience.trail.len() <= TRAIL);
                assert!(experience.haunts.haunts(&mesh).count() <= T.haunt.most);
            }
            experience
        };
        let once = life();
        assert!(once.lessons.learned().count() > 0, "it learned something");
        assert_eq!(once, life(), "deterministic");
    }

    #[test]
    fn with_learning_switched_off_it_chooses_as_the_reasoner_alone_would() {
        let off = crate::Tuning { lesson: crate::think::lesson::Tuning { sway: 0.0, ..T.lesson }, ..T };
        let mut experience = Experience::<Way>::new();
        for _ in 0..20 {
            for conduct in [Conduct::Chase, Conduct::Investigate, Conduct::Shelter] {
                went(&mut experience, conduct, Outcome::failed(Venture::Pursuit));
                went(&mut experience, conduct, Outcome::failed(Venture::Investigation));
                went(&mut experience, conduct, Outcome::failed(Venture::Defence));
            }
            went(&mut experience, Conduct::Search, Outcome::succeeded(Venture::Search));
        }
        let mut plain = Reasoners::default();
        let mut reasoners = Reasoners::default();
        for bits in 0..256u32 {
            let reading = Reading {
                hunting: bits & 1 != 0,
                investigating: bits & 2 != 0,
                searching: bits & 4 != 0,
                sheltering: bits & 8 != 0,
                reporting: bits & 16 != 0,
                afraid: bits & 32 != 0,
                watching: bits & 64 != 0,
                partnered: bits & 128 != 0,
                ..READING
            };
            for role in [Role::Sentry, Role::Citizen, Role::Gang, Role::Fighter] {
                assert_eq!(experience.choose(&off, &mut reasoners, role, &reading, None), plain.choose(role, &reading, None), "{role:?} {reading:?}");
            }
        }
        // And with no rate, nothing is learned at all.
        let still = crate::Tuning { lesson: crate::think::lesson::Tuning { rate: 0.0, ..T.lesson }, ..T };
        let mut fresh = Experience::<Way>::new();
        fresh.trail.doing(Conduct::Chase);
        fresh.report(&still, None, Outcome::failed(Venture::Pursuit));
        assert_eq!(fresh.lessons.learned().count(), 0);
    }
}
