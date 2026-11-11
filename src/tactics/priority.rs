//! Priorities: how pressing, how dangerous, how sure, how far into it, and what kind of NPC it is
//! - weighing what [`crate::tactics::conduct`]'s reasoner would choose.
//!
//! The conduct reasoner ranks what is open to an NPC in a fixed order by role - security chases
//! before it investigates, investigates before it searches, and so on - nudged by its nerve and
//! wariness. That order says what matters most in general; it cannot say that a search started a
//! moment ago is worth more than a faint noise, or that a guard halfway to where it heard something
//! should finish looking rather than turn for the next sound, or that a hothead shoots back where
//! a careful one takes cover. This module weighs each conduct by a [`Context`] - the situation
//! as the NPC sees it - and its [`Traits`], as one multiplier per conduct, given to the reasoner
//! through [`crate::tactics::utility::Reasoner::choose_by`] (and, with what it has learned, by
//! [`crate::think::experience::Experience::choose_with`]).
//!
//! Each factor is 1, weighing nothing, when there is nothing to go on: no urgency, no threat, no
//! confidence given, traits in the middle, not doing anything yet. So a game that passes a default
//! [`Context`] and default [`Traits`] chooses exactly as the reasoner alone would. Each is bounded
//! by its tuning's weight, so none can turn the order upside down on its own:
//!
//! | Factor | Weighs up | Weighs down | From |
//! |--------|-----------|-------------|------|
//! | urgency | responding: chasing, fighting, shooting back, sheltering, reporting | going about its business, company, looking at someone | [`Context::urgency`], 0 to 1 |
//! | threat | keeping safe: sheltering, backing away; the cautious more, the aggressive less | - | [`Context::threat`], 0 to 1 |
//! | confidence | acting on what it is sure of: a fresh sighting to chase or search from, a clear lead to investigate | the same, unsure | [`Context::sighting`], [`Context::lead`] |
//! | commitment | going on with what it is doing, the more the longer it has been at it | - | [`Context::doing`] |
//! | traits | the aggressive: chasing, fighting, shooting back; the cautious: sheltering, backing away, watching; the curious: investigating - following a new lead, rather than keeping to a search | the same, the other way, below the middle | [`Traits`] |
//!
//! [`factors`] gives each factor for a conduct, for a game to show why an NPC chose what it did.
//!
//! Three traits bear on more than choice, each as a multiplier that is 1 in the middle:
//!
//! | Trait | Bears on | Helper |
//! |-------|----------|--------|
//! | observant | how keenly it sees and hears: the eagerness its awareness fills with ([`crate::perception::detection::Detection::step`], `hear`) | [`keenness`] |
//! | social | how far its voice carries word to others, and how readily it believes what it is told | [`voice`], [`credulity`] |
//! | independent | how much surer word has to be to override what it saw itself, and how much it keeps to what it is doing (the commitment factor) | [`self_reliance`] |
//!
//! So two NPCs of one kind, with one AI, take the same event differently: one notices it sooner,
//! one tells it further, one believes it, one goes by its own eyes.

use crate::tactics::conduct::Conduct;

/// How much urgency weighs responding up, at most: half as much again.
pub const URGENCY: f32 = 0.5;
/// How much threat weighs keeping safe up, at most: as much again.
pub const THREAT: f32 = 1.0;
/// How much confidence weighs acting on it, either way, at most: half.
pub const CONFIDENCE: f32 = 0.5;
/// How much more going on with what it is doing counts, at most, and how long it takes to come to
/// that, in seconds.
pub const COMMITMENT: f32 = 0.3;
pub const COMMITTED_AFTER: f32 = 4.0;
/// How much a trait weighs what it bears on, either way, at its extremes: two fifths.
pub const TRAIT: f32 = 0.4;
/// How much independence weighs relying on itself, either way, at its extremes: from nothing to
/// twice as much.
pub const INDEPENDENCE: f32 = 1.0;

tuning!(
    /// How much each factor of a [`Context`] and of [`Traits`] weighs (see the module's constants).
    {
        /// How much urgency weighs responding up, at most.
        urgency: f32 = URGENCY,
        /// How much threat weighs keeping safe up, at most.
        threat: f32 = THREAT,
        /// How much confidence weighs acting on it, either way, at most.
        confidence: f32 = CONFIDENCE,
        /// How much more going on with what it is doing counts, at most, and how long it takes to
        /// come to that, in seconds.
        commitment: f32 = COMMITMENT,
        committed_after: f32 = COMMITTED_AFTER,
        /// How much a trait weighs what it bears on, either way, at its extremes.
        traits: f32 = TRAIT,
        /// How much independence weighs relying on itself, either way, at its extremes.
        independence: f32 = INDEPENDENCE,
    }
);

/// What kind of NPC it is, each from 0 to 1, a half being neither one way nor the other.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(default))]
pub struct Traits {
    /// Quick to go after someone and to fight: chases, fights and shoots back the more.
    pub aggression: f32,
    /// Careful of itself: shelters, backs away and watches the more.
    pub caution: f32,
    /// Keen to know: looks into what it notices the more - leaving a search for a new lead.
    pub curiosity: f32,
    /// Sharp-eyed and sharp-eared: notices sooner (see [`keenness`]).
    pub observant: f32,
    /// Outgoing: calls out what it saw further, and believes what it is told the more (see
    /// [`voice`], [`credulity`]).
    pub sociability: f32,
    /// Its own judge: goes by what it saw itself over what it is told, and keeps to what it is
    /// doing (see [`self_reliance`]).
    pub independence: f32,
}

impl Traits {
    /// Neither one way nor the other in anything.
    pub const NEUTRAL: Self = Self { aggression: 0.5, caution: 0.5, curiosity: 0.5, observant: 0.5, sociability: 0.5, independence: 0.5 };

    /// Traits of its own, the same each time from `seed` - an NPC's number, say - each spread
    /// about the middle by up to `spread` either way (0 to 0.5): a crowd that is not all alike,
    /// with no two runs differing.
    pub fn from_seed(seed: u32, spread: f32) -> Self {
        let spread = if spread.is_finite() { spread.clamp(0.0, 0.5) } else { 0.0 };
        let mut state = seed.wrapping_mul(0x9E37_79B9).wrapping_add(0x7F4A_7C15);
        let mut next = || {
            // xorshift32: enough to spread six numbers.
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            0.5 + ((state >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0) * spread
        };
        Self { aggression: next(), caution: next(), curiosity: next(), observant: next(), sociability: next(), independence: next() }
    }
}

impl Default for Traits {
    fn default() -> Self {
        Self::NEUTRAL
    }
}

/// The situation as an NPC sees it, for weighing what to do. Its [`Default`] weighs nothing.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Context {
    /// How pressing things are, from 0 (nothing presses) to 1 (as pressing as can be: the player
    /// in sight, a shooting).
    pub urgency: f32,
    /// How dangerous for it, from 0 (not at all) to 1 (shot at, a weapon on it).
    pub threat: f32,
    /// How sure it is of where the player is, to chase or search from, from 0 to 1: a fresh
    /// sighting of its own near 1, fading the longer ago; told by another, less. None: nothing to
    /// go on, weighing nothing.
    pub sighting: Option<f32>,
    /// How sure it is that what it noticed is worth looking into, from 0 to 1: how aware of
    /// something it became, how near a noise. None: weighing nothing.
    pub lead: Option<f32>,
    /// What it is doing already, and for how long, in seconds.
    pub doing: Option<(Conduct, f32)>,
}

/// Each factor weighing a conduct, multiplied together in [`Factors::total`] - for a game to show
/// why an NPC did what it did.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Factors {
    pub urgency: f32,
    pub threat: f32,
    pub confidence: f32,
    pub commitment: f32,
    pub traits: f32,
}

impl Factors {
    /// All of them together.
    pub fn total(&self) -> f32 {
        self.urgency * self.threat * self.confidence * self.commitment * self.traits
    }
}

/// What `conduct` does: respond to trouble, keep itself safe, go after someone, look into
/// something, or neither.
fn responds(conduct: Conduct) -> bool {
    use Conduct::*;
    matches!(conduct, Chase | Fight | ShootBack | Shelter | Report)
}

fn unhurried(conduct: Conduct) -> bool {
    use Conduct::*;
    matches!(conduct, GoAbout | Regard | Follow | Avoid | Watch)
}

/// `x` brought to 0 to 1, nothing that is not a number.
fn unit(x: f32) -> f32 {
    if x.is_nan() {
        0.0
    } else {
        x.clamp(0.0, 1.0)
    }
}

/// From `1 - weight` at 0 to `1 + weight` at 1, through 1 at a half.
fn either_way(weight: f32, x: f32) -> f32 {
    1.0 + weight.max(0.0) * (unit(x) - 0.5) * 2.0
}

/// How keenly an NPC with `traits` sees and hears: a multiplier on how fast its awareness fills
/// (the eagerness given [`crate::perception::detection::Detection`]), from `1 - traits` weight
/// for the least observant to `1 + ` it for the most.
pub fn keenness(tuning: &Tuning, traits: &Traits) -> f32 {
    either_way(tuning.traits, traits.observant)
}

/// How far an NPC with `traits` calls out word: a multiplier on how far its voice carries (see
/// [`crate::social::word::Tuning::voice_range`]).
pub fn voice(tuning: &Tuning, traits: &Traits) -> f32 {
    either_way(tuning.traits, traits.sociability)
}

/// How readily an NPC with `traits` believes what it is told: a multiplier on its belief in word
/// from others (see [`crate::social::word::Word::belief`]), never on what it saw itself.
pub fn credulity(tuning: &Tuning, traits: &Traits) -> f32 {
    either_way(tuning.traits, traits.sociability)
}

/// How much an NPC with `traits` relies on itself: a multiplier, from nothing for one that goes
/// by whatever it is told to twice as much (with the default weight) for its own judge, on how
/// much surer word has to be to override what it saw, and on how much it keeps to what it is
/// doing.
pub fn self_reliance(tuning: &Tuning, traits: &Traits) -> f32 {
    either_way(tuning.independence, traits.independence).max(0.0)
}

/// Each factor weighing `conduct`, for an NPC with `traits` in `context`.
pub fn factors(tuning: &Tuning, traits: &Traits, context: &Context, conduct: Conduct) -> Factors {
    use Conduct::*;
    let urgency = unit(context.urgency);
    let urgency = if responds(conduct) {
        1.0 + tuning.urgency.max(0.0) * urgency
    } else if unhurried(conduct) {
        1.0 / (1.0 + tuning.urgency.max(0.0) * urgency)
    } else {
        1.0
    };
    // Keeping safe, under threat: the more so the more cautious, the less so the more aggressive.
    let threat = match conduct {
        Shelter | BackAway => {
            let temper = (1.0 + unit(traits.caution) - unit(traits.aggression)).clamp(0.0, 2.0);
            1.0 + tuning.threat.max(0.0) * unit(context.threat) * temper
        }
        _ => 1.0,
    };
    let confidence = match (conduct, context.sighting, context.lead) {
        (Chase | Search, Some(sure), _) => either_way(tuning.confidence, sure),
        (Investigate, _, Some(sure)) => either_way(tuning.confidence, sure),
        _ => 1.0,
    };
    let commitment = match context.doing {
        Some((doing, for_)) if doing == conduct && for_.is_finite() => {
            1.0 + tuning.commitment.max(0.0) * unit(for_ / tuning.committed_after.max(1.0e-3)) * self_reliance(tuning, traits)
        }
        _ => 1.0,
    };
    let traits = match conduct {
        Chase | Fight | ShootBack => either_way(tuning.traits, traits.aggression),
        Shelter | BackAway | Watch => either_way(tuning.traits, traits.caution),
        Investigate => either_way(tuning.traits, traits.curiosity),
        _ => 1.0,
    };
    Factors { urgency, threat, confidence, commitment, traits }
}

/// How much `conduct` is weighed, all factors together (see [`factors`]): the bias to give a
/// reasoner.
pub fn weigh(tuning: &Tuning, traits: &Traits, context: &Context, conduct: Conduct) -> f32 {
    factors(tuning, traits, context, conduct).total()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tactics::conduct::{Reading, Reasoners, Role};

    const T: Tuning = Tuning::DEFAULT;

    fn choose(traits: &Traits, context: &Context, reading: &Reading, current: Option<Conduct>) -> Conduct {
        let mut reasoners = Reasoners::default();
        reasoners.of(Role::Sentry).choose_by(reading, current, |c| weigh(&T, traits, context, c)).unwrap_or(Conduct::GoAbout)
    }

    const ALL: [Conduct; 14] = [
        Conduct::Chase,
        Conduct::Fight,
        Conduct::ShootBack,
        Conduct::Investigate,
        Conduct::Search,
        Conduct::Report,
        Conduct::Shelter,
        Conduct::Mourn,
        Conduct::BackAway,
        Conduct::Watch,
        Conduct::Avoid,
        Conduct::Regard,
        Conduct::Follow,
        Conduct::GoAbout,
    ];

    #[test]
    fn observant_social_and_independent_bear_on_perception_talk_and_self_reliance() {
        let n = Traits::NEUTRAL;
        assert_eq!((keenness(&T, &n), voice(&T, &n), credulity(&T, &n), self_reliance(&T, &n)), (1.0, 1.0, 1.0, 1.0));
        let sharp = Traits { observant: 1.0, ..n };
        let dull = Traits { observant: 0.0, ..n };
        assert!((keenness(&T, &sharp) - 1.4).abs() < 1.0e-6 && (keenness(&T, &dull) - 0.6).abs() < 1.0e-6);
        let outgoing = Traits { sociability: 0.9, ..n };
        assert!(voice(&T, &outgoing) > 1.0 && credulity(&T, &outgoing) > 1.0);
        let own_judge = Traits { independence: 1.0, ..n };
        let follower = Traits { independence: 0.0, ..n };
        assert_eq!((self_reliance(&T, &own_judge), self_reliance(&T, &follower)), (2.0, 0.0));
        // Independence weighs keeping on with what it is doing; with nothing done, nothing.
        let doing = Context { doing: Some((Conduct::Search, 10.0)), ..Context::default() };
        assert!(weigh(&T, &own_judge, &doing, Conduct::Search) > weigh(&T, &n, &doing, Conduct::Search));
        assert_eq!(weigh(&T, &follower, &doing, Conduct::Search), 1.0);
        assert_eq!(weigh(&T, &own_judge, &Context::default(), Conduct::Search), 1.0);
    }

    #[test]
    fn traits_from_a_seed_are_its_own_and_the_same_each_time() {
        let a = Traits::from_seed(7, 0.3);
        assert_eq!(a, Traits::from_seed(7, 0.3));
        assert_ne!(a, Traits::from_seed(8, 0.3));
        assert_eq!(Traits::from_seed(7, 0.0), Traits::NEUTRAL);
        let all: Vec<Traits> = (0..1000).map(|s| Traits::from_seed(s, 0.5)).collect();
        for t in &all {
            for x in [t.aggression, t.caution, t.curiosity, t.observant, t.sociability, t.independence] {
                assert!((0.0..=1.0).contains(&x), "{t:?}");
            }
        }
        let mean = all.iter().map(|t| t.observant).sum::<f32>() / all.len() as f32;
        assert!((mean - 0.5).abs() < 0.05, "{mean}");
        assert!(all.iter().any(|t| t.observant < 0.1) && all.iter().any(|t| t.observant > 0.9));
    }

    #[test]
    fn with_nothing_to_go_on_it_weighs_nothing() {
        for conduct in ALL {
            assert_eq!(weigh(&T, &Traits::default(), &Context::default(), conduct), 1.0, "{conduct:?}");
        }
    }

    #[test]
    fn a_fresh_trail_comes_before_a_faint_noise_and_a_clear_lead_before_a_cold_trail() {
        // Searching, and something noticed: as the reasoner alone has it, it goes to look.
        let both = Reading { searching: true, investigating: true, ..Reading::default() };
        let neutral = Traits::default();
        assert_eq!(choose(&neutral, &Context::default(), &both, None), Conduct::Investigate);
        // Lost a moment ago, a faint noise: it keeps to the search.
        let fresh = Context { sighting: Some(0.95), lead: Some(0.3), ..Context::default() };
        assert_eq!(choose(&neutral, &fresh, &both, None), Conduct::Search);
        // Lost long ago, a clear noise close by: it goes to look.
        let cold = Context { sighting: Some(0.15), lead: Some(0.9), ..Context::default() };
        assert_eq!(choose(&neutral, &cold, &both, None), Conduct::Investigate);
    }

    #[test]
    fn under_fire_the_cautious_take_cover_and_the_aggressive_shoot_back() {
        let shot_at = Reading { shot_at: true, sheltering: true, ..Reading::default() };
        let under_fire = Context { urgency: 1.0, threat: 1.0, ..Context::default() };
        let careful = Traits { caution: 0.9, aggression: 0.2, ..Traits::default() };
        let hothead = Traits { caution: 0.1, aggression: 0.95, ..Traits::default() };
        assert_eq!(choose(&careful, &under_fire, &shot_at, None), Conduct::Shelter);
        assert_eq!(choose(&hothead, &under_fire, &shot_at, None), Conduct::ShootBack);
        // Neither one nor the other, with nothing pressing, it shoots back, as the order has it;
        // the careful one still keeps its head down - cover is only open in a shooting.
        assert_eq!(choose(&Traits::default(), &Context::default(), &shot_at, None), Conduct::ShootBack);
        assert_eq!(choose(&careful, &Context::default(), &shot_at, None), Conduct::Shelter);
    }

    #[test]
    fn urgency_puts_company_off_but_never_opens_what_is_shut() {
        let passing = Reading { regarding: true, investigating: true, ..Reading::default() };
        let calm_traits = Traits { curiosity: 0.1, ..Traits::default() };
        // Unhurried and incurious, it stops to look at someone rather than look into a thing...
        let mut reasoners = Reasoners::default();
        let citizen = |context: &Context, reasoners: &mut Reasoners| {
            reasoners.of(Role::Citizen).choose_by(&passing, None, |c| weigh(&T, &calm_traits, context, c)).unwrap()
        };
        let quiet = Context::default();
        let first = citizen(&quiet, &mut reasoners);
        // ...but it never chooses what is shut, however urgent.
        let urgent = Context { urgency: 1.0, threat: 1.0, ..Context::default() };
        let chosen = citizen(&urgent, &mut reasoners);
        assert!(matches!(chosen, Conduct::Investigate | Conduct::Regard), "{first:?} {chosen:?}");
        assert_eq!(choose(&Traits::default(), &urgent, &Reading::default(), None), Conduct::GoAbout);
        let f = factors(&T, &Traits::default(), &urgent, Conduct::Regard);
        assert!(f.urgency < 1.0 && factors(&T, &Traits::default(), &urgent, Conduct::Chase).urgency > 1.0);
    }

    #[test]
    fn the_longer_it_has_been_at_something_the_more_it_takes_to_turn_it_from_it() {
        // Searching; something noticed, a lead a little better than the trail.
        let both = Reading { searching: true, investigating: true, ..Reading::default() };
        let context = |doing| Context { sighting: Some(0.6), lead: Some(0.55), doing, ..Context::default() };
        let neutral = Traits::default();
        // Not committed to the search yet: it goes to look.
        assert_eq!(choose(&neutral, &context(Some((Conduct::Search, 0.0))), &both, Some(Conduct::Search)), Conduct::Search, "inertia alone holds it a moment");
        assert_eq!(choose(&neutral, &context(None), &both, None), Conduct::Investigate);
        // Well into the search, it keeps to it.
        let committed = factors(&T, &neutral, &context(Some((Conduct::Search, COMMITTED_AFTER))), Conduct::Search).commitment;
        assert!((committed - (1.0 + COMMITMENT)).abs() < 1.0e-6);
        assert!(factors(&T, &neutral, &context(Some((Conduct::Search, 100.0))), Conduct::Search).commitment <= 1.0 + COMMITMENT);
    }

    #[test]
    fn readings_that_waver_about_a_tie_do_not_make_it_flicker() {
        // The lead's confidence wavers about where investigating and searching tie, frame by frame.
        let both = Reading { searching: true, investigating: true, ..Reading::default() };
        let neutral = Traits::default();
        let mut reasoners = Reasoners::default();
        let mut current: Option<Conduct> = None;
        let mut since = 0.0;
        let mut changes = 0;
        for frame in 0..600 {
            let lead = 0.4 + 0.1 * ((frame as f32) * 0.7).sin();
            let context = Context { sighting: Some(0.5), lead: Some(lead), doing: current.map(|c| (c, since)), ..Context::default() };
            let chosen = reasoners.of(Role::Sentry).choose_by(&both, current, |c| weigh(&T, &neutral, &context, c)).unwrap();
            if current.is_some_and(|c| c != chosen) {
                changes += 1;
                since = 0.0;
            } else {
                since += 1.0 / 60.0;
            }
            current = Some(chosen);
        }
        assert!(changes <= 1, "{changes} changes in ten seconds");
    }

    #[test]
    fn nonsense_weighs_nothing_and_every_factor_stays_bounded() {
        let wild = Context { urgency: f32::NAN, threat: f32::INFINITY, sighting: Some(f32::NAN), lead: Some(-5.0), doing: Some((Conduct::Chase, f32::NAN)) };
        let traits = Traits { aggression: f32::NAN, caution: 9.0, curiosity: -3.0, observant: f32::INFINITY, sociability: f32::NAN, independence: -7.0 };
        for x in [keenness(&T, &traits), voice(&T, &traits), credulity(&T, &traits), self_reliance(&T, &traits)] {
            assert!(x.is_finite() && (0.0..=2.0).contains(&x), "{x}");
        }
        for conduct in ALL {
            let f = factors(&T, &traits, &wild, conduct);
            for x in [f.urgency, f.threat, f.confidence, f.commitment, f.traits] {
                assert!(x.is_finite() && (1.0 - T.confidence - T.traits..=2.0 + T.threat * 2.0).contains(&x), "{conduct:?} {f:?}");
            }
        }
    }
}
