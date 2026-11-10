//! Orchestration, optional: the crate's modules run together as one guard's mind, and a squad's.
//!
//! Every other module stands on its own, and a game can wire them up however it likes (as
//! `tests/scenarios.rs` and Maze's `inhabitants.rs` do). What every game ends up writing is
//! the same glue: awareness filling into a noticed-something, then an investigation; spotted, the
//! alert phase with its seconds left and how wariness stretches them; lost, a search map to
//! spread and look over on a clock, and searchers sent to different places on it; another sentry
//! joining in a chase it sees; being told where to look; choosing what to do by utility, weighed by
//! what it has learned; and, once calm, back to its routine. This module is that glue, so that a
//! game need not write it - and nothing else in the crate depends on it.
//!
//! - A [`Sentry`] is one NPC's mind: what it is aware of, its alert phase, what it is doing about
//!   it, and what it has learned ([`Experience`]).
//! - A [`Squad`] is what sentries share: the search map, and the clock it is looked over by.
//! - Each frame the game says what each sentry takes in ([`Senses`]: where it stands, which way it
//!   faces, how visible the player is to it - the game casts the rays), and [`Squad::step`] gives
//!   back, for each, a [`Thought`]: what it means to do ([`Intent`]: its routine, to go somewhere
//!   at a [`Pace`], or to face a way), what it chose ([`Conduct`]), and what happened to it
//!   ([`Happened`]) for the game to say, play or show.
//!
//! The game still moves the body: it routes to where an [`Intent::GoTo`] says, walks or runs it
//! there, and turns it as an [`Intent::Face`] says; it tells a sentry when there is no way there
//! ([`Sentry::no_way`]), what it hears ([`Squad::noise`]) and what it is told ([`Sentry::tell`]).
//! Its routine is the game's own ([`Intent::Routine`]), unless it is given a beat to walk
//! ([`Sentry::with_beat`]).
//!
//! The phases it goes through are the crate's own ([`crate::alert`]): calm; noticing something
//! (awareness past caution), it investigates; spotted, Alert, chasing; lost, Evasion, searching,
//! where the player was lost, then wherever the shared search map says they likeliest are; the
//! search run out, Caution - back about its routine, but warier, and quicker to spot; and then calm
//! again. Its wariness from all this fades over minutes ([`crate::alert::Wariness`]).

use crate::{
    alert::{Alert, Incident, Wariness},
    flat,
    navigation::navmesh::NavMesh,
    perception::{
        detection::{Change, Detection},
        hearing::Heard,
        sight::{Seeing, Stance},
    },
    social::word::{self, Channel, Trust, Word},
    tactics::{
        conduct::{Conduct, Reading, Reasoners, Role},
        investigate::{Doing, Investigation},
        join::{joiners, Watch},
        look::LookAbout,
        priority::{self, Context, Traits},
        search::SearchMap,
    },
    think::{
        experience::{Experience, Outcome, Venture},
        memory::{Cue, Kind, Memory, Record},
    },
};
use nalgebra::Vector3;

/// How near where it is going counts as there, in meters.
pub const ARRIVED: f32 = 1.0;
/// How often the searchers look over the search map, in seconds.
pub const LOOK_EVERY: f32 = 0.25;
/// How far apart searchers keep where they are going, in meters.
pub const APART: f32 = 4.0;
/// How far, in meters of walking, a searcher goes at most to search somewhere.
pub const WITHIN: f32 = 100.0;
/// How fast the player is taken to get away, in meters per second, for where they could be.
pub const GETAWAY: f32 = 4.0;
/// How far a searcher's look clears the search map, in meters.
pub const SEARCH_SIGHT: f32 = 16.0;
/// How long, in seconds, till a sentry is half as sure of where it saw the player, and of a lead
/// it is following.
pub const SIGHTING_HALF_LIFE: f32 = 15.0;
pub const LEAD_HALF_LIFE: f32 = 10.0;
/// How sure a sentry is of where it is told the player is, against seeing them itself.
pub const TOLD: f32 = 0.6;
/// How sure of a lead it has passed over, at least, for it to keep it in mind: under this, it lets
/// it go, rather than look into a noise long after.
pub const LEAD_FORGOTTEN: f32 = 0.05;
/// Persistence: how much more what it is in the middle of counts - an investigation within
/// [`FINISH_WITHIN`] of its spot, a search spot it is looking about - on top of
/// whatever else weighs it: twice as much.
pub const HOLD: f32 = 1.0;
/// How near the spot it is investigating it finishes it before anything new but a sighting, in
/// meters.
pub const FINISH_WITHIN: f32 = 4.0;
/// How much surer than the lead it is following a new one has to be to take its place, rather than
/// wait its turn.
pub const SWITCH_LEAD_BY: f32 = 0.25;
/// After a disturbance that came to nothing - a look that found nothing, a noise too faint to look
/// into - how long it stays attentive, in seconds; how many times as keenly it looks and listens
/// meanwhile; and how long of that it stands looking that way first.
pub const ATTENTIVE_FOR: f32 = 8.0;
pub const ATTENTIVE_KEENNESS: f32 = 1.5;
pub const LINGER: f32 = 1.5;
/// Memory: how much less sure of a lead it is where it looked lately and found nothing, at most;
/// and how sure a lead has to be, at least, for a calm sentry to go and look rather than only
/// look that way.
pub const RECHECK: f32 = 0.75;
pub const WORTH_A_LOOK: f32 = 0.3;
/// How dangerous a place it remembers coming to harm feels, at most, from 0 to 1, within how many
/// meters of it; and how dangerous things have to be for it to remember the place.
pub const REMEMBERED_DANGER: f32 = 0.6;
pub const DANGER_NEAR: f32 = 5.0;
pub const DANGER_NOTED: f32 = 0.5;
/// Losing the player, it recalls where it found them before, and where it lost them before, within
/// how many meters, at most how many such memories; and leans the search map toward them, a
/// polygon within how many meters of one up to how many times again as likely.
pub const RECALL_WITHIN: f32 = 30.0;
pub const RECALL_MOST: usize = 8;
pub const MEMORY_LEAN: f32 = 1.0;
pub const LEAN_NEAR: f32 = 4.0;
/// How much a sentry has to believe word of where the player is (see [`word::Word::belief`]) to
/// go and search there.
pub const BELIEVE: f32 = 0.3;

tuning!(
    /// How the orchestration layer runs the modules together (see the module's constants).
    {
        /// How near where it is going counts as there, in meters.
        arrived: f32 = ARRIVED,
        /// How often the searchers look over the search map, in seconds.
        look_every: f32 = LOOK_EVERY,
        /// How far apart searchers keep where they are going, in meters.
        apart: f32 = APART,
        /// How far, in meters of walking, a searcher goes at most to search somewhere.
        within: f32 = WITHIN,
        /// How fast the player is taken to get away, in meters per second.
        getaway: f32 = GETAWAY,
        /// How far a searcher's look clears the search map, in meters.
        search_sight: f32 = SEARCH_SIGHT,
        /// How long till a sentry is half as sure of where it saw the player, and of a lead.
        sighting_half_life: f32 = SIGHTING_HALF_LIFE,
        lead_half_life: f32 = LEAD_HALF_LIFE,
        /// How sure it is of where it is told the player is, from 0 to 1.
        told: f32 = TOLD,
        /// How much more what it is in the middle of counts.
        hold: f32 = HOLD,
        /// How near its spot an investigation or a search spot is finished before anything new.
        finish_within: f32 = FINISH_WITHIN,
        /// How much surer a new lead has to be to take the place of the one it is following.
        switch_lead_by: f32 = SWITCH_LEAD_BY,
        /// How long it stays attentive after a disturbance that came to nothing, how much more
        /// keenly, and how long of it it stands looking that way first.
        attentive_for: f32 = ATTENTIVE_FOR,
        attentive_keenness: f32 = ATTENTIVE_KEENNESS,
        linger: f32 = LINGER,
        /// How much less sure of a lead it is where it looked lately and found nothing, at most;
        /// and how sure a lead has to be for a calm sentry to go and look.
        recheck: f32 = RECHECK,
        worth_a_look: f32 = WORTH_A_LOOK,
        /// How dangerous a place it remembers coming to harm feels, at most, within how far of
        /// it; and how dangerous things have to be for it to remember the place.
        remembered_danger: f32 = REMEMBERED_DANGER,
        danger_near: f32 = DANGER_NEAR,
        danger_noted: f32 = DANGER_NOTED,
        /// Losing the player, how far off it recalls where it found or lost them before, how many
        /// such memories at most, how much it leans the search toward them, within how far.
        recall_within: f32 = RECALL_WITHIN,
        recall_most: usize = RECALL_MOST,
        memory_lean: f32 = MEMORY_LEAN,
        lean_near: f32 = LEAN_NEAR,
        /// How much a sentry has to believe word of where the player is to search there.
        believe: f32 = BELIEVE,
    }
);

/// How fast to go: as the game has its NPCs walk, hurry and run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Pace {
    /// About its business, or going to look at something.
    Walk,
    /// Searching.
    Search,
    /// After the player.
    Run,
}

/// What a sentry means to do this frame, for the game to carry out.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Intent {
    /// Its routine, whatever the game has it do: there is nothing else on its mind.
    Routine,
    /// Go to `at`, at `pace`: route there if it is not already going there.
    GoTo { at: Vector3<f32>, pace: Pace },
    /// Stand, facing `heading`.
    Face { heading: f32 },
}

/// Something that happened to a sentry, for the game to say, play or show.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Happened {
    /// It noticed something, and goes to look.
    Noticed,
    /// It spotted the player: Alert.
    Spotted,
    /// It lost sight of the player: Evasion, searching.
    LostContact,
    /// Searching, or watchful, it saw them again: Alert.
    FoundAgain,
    /// It looked where it noticed something, and found nothing.
    InvestigationDone,
    /// Its search ran out: Caution.
    GaveUp,
    /// It is calm again.
    Calm,
    /// It was told where to look, and searches there.
    Told,
    /// It saw another sentry after the player, and joined in.
    Joined,
    /// It was told where the player was, and did not believe it enough to go: too old, passed on
    /// too often, or from one whose word has proved wrong.
    Doubted,
}

/// What a sentry takes in this frame: the game's part.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Senses {
    /// Where it stands, and which way it faces.
    pub feet: Vector3<f32>,
    pub heading: f32,
    /// How visible the player is to it, from 0 - out of sight, or anything in the way - to 1: see
    /// [`crate::perception::sight::Sight::visibility`], times nothing if the game's ray is
    /// blocked.
    pub visible: f32,
    /// Where the player is.
    pub player: Vector3<f32>,
    /// How dangerous things are for it, from 0 to 1: shot at, a weapon on it (see
    /// [`priority::Context::threat`]). 0 if the game has no shooting.
    pub threat: f32,
}

/// What came of a sentry's frame.
#[derive(Debug, Clone, PartialEq)]
pub struct Thought {
    /// What it means to do.
    pub intent: Intent,
    /// What it chose to do, by utility.
    pub conduct: Conduct,
    /// What happened to it this frame, in order.
    pub happened: Vec<Happened>,
}

/// One NPC's mind: what it is aware of, its alert phase, what it is doing about it, and what it
/// has learned.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Sentry {
    /// What kind of NPC it is, which weighs its choices.
    pub role: Role,
    /// How sure of itself it is, from 0 to 1 (see [`Reading::nerve`]).
    pub nerve: f32,
    /// How aware of the player it is.
    pub detection: Detection,
    /// How wary it is from what it has been through.
    pub wariness: Wariness,
    /// What it has been through, and learned from it.
    pub experience: Experience,
    /// What kind of NPC it is: aggressive, cautious, curious (see [`priority`]).
    pub traits: Traits,
    /// What it remembers having happened (see [`crate::think::memory`]): where it saw and lost
    /// the player, where it looked and what it found, what it was told, where it came to harm.
    pub memory: Memory,
    /// Whether it has a radio, to pass word on and be told it from afar (see [`word`]).
    pub radio: bool,
    /// How far it trusts the word of each it has been told by.
    pub trust: Trust,
    /// Whose word it is searching on, if anyone's: to trust the more if it finds them, the less
    /// if it does not.
    acting_on: Option<u32>,
    alert: Option<Alert>,
    /// Seconds left of its phase, Evasion or Caution.
    left: f32,
    investigation: Option<Investigation>,
    look: Option<LookAbout>,
    /// Whether it is looking about a spot it went to search - something to finish - rather than
    /// looking about where it is for want of anywhere likelier.
    at_spot: bool,
    /// Where it is going, searching or walking its beat.
    goal: Option<Vector3<f32>>,
    beat: Vec<Vector3<f32>>,
    post: usize,
    conduct: Option<Conduct>,
    /// How long it has been doing it, in seconds.
    doing_for: f32,
    /// How sure it was of where it saw the player, when it did, and how long ago that was; and of
    /// its lead, and how long ago.
    sighting: (f32, f32),
    lead: (f32, f32),
    threat: f32,
    /// A lead waiting its turn: where, how sure it was, and how long ago.
    pending: Option<(Vector3<f32>, f32, f32)>,
    /// Attentive after a disturbance: how long it has left, toward where, and how long it has
    /// stood looking that way.
    attentive: Option<(f32, Vector3<f32>, f32)>,
    /// Where, about where it lost the player, it recalled finding and losing them before, and how
    /// well each fits: where it searches first.
    haunts: Vec<(Vector3<f32>, f32)>,
    /// Where it last saw the player, and which way they were going.
    last_seen: Option<Vector3<f32>>,
    going: Vector3<f32>,
    /// Where it was told the player was, how many seconds ago they were there, and how sure of it.
    told: Option<(Vector3<f32>, f32, f32)>,
    feet: Vector3<f32>,
    heading: f32,
    happened: Vec<Happened>,
}

impl Sentry {
    /// A calm NPC of `role`, its routine the game's own.
    pub fn new(role: Role) -> Self {
        Self {
            role,
            nerve: 0.5,
            detection: Detection::default(),
            wariness: Wariness::default(),
            experience: Experience::new(),
            traits: Traits::NEUTRAL,
            memory: Memory::new(),
            radio: false,
            trust: Trust::new(),
            acting_on: None,
            alert: None,
            left: 0.0,
            investigation: None,
            look: None,
            at_spot: false,
            goal: None,
            beat: Vec::new(),
            post: 0,
            conduct: None,
            doing_for: 0.0,
            sighting: (0.0, 0.0),
            lead: (0.0, 0.0),
            threat: 0.0,
            pending: None,
            attentive: None,
            haunts: Vec::new(),
            last_seen: None,
            going: Vector3::zeros(),
            told: None,
            feet: Vector3::zeros(),
            heading: 0.0,
            happened: Vec::new(),
        }
    }

    /// Its routine, walking from one of `posts` to the next and round again, rather than the
    /// game's own.
    pub fn with_beat(mut self, posts: Vec<Vector3<f32>>) -> Self {
        self.beat = posts.into_iter().filter(|p| p.iter().all(|v| v.is_finite())).collect();
        self
    }

    /// Its alert phase: none, calm.
    pub fn alert(&self) -> Option<Alert> {
        self.alert
    }

    /// What it chose to do last.
    pub fn conduct(&self) -> Option<Conduct> {
        self.conduct
    }

    /// Where it last saw the player, if it has.
    pub fn last_seen(&self) -> Option<Vector3<f32>> {
        self.last_seen
    }

    /// Where it stood last frame.
    pub fn feet(&self) -> Vector3<f32> {
        self.feet
    }

    /// Where it is going, searching or on its beat, if anywhere.
    pub fn goal(&self) -> Option<Vector3<f32>> {
        self.goal
    }

    /// With a radio.
    pub fn with_radio(mut self) -> Self {
        self.radio = true;
        self
    }

    /// Its traits instead: what kind of NPC it is.
    pub fn with_traits(mut self, traits: Traits) -> Self {
        self.traits = traits;
        self
    }

    /// The situation as it sees it, as its priorities weigh it (see [`priority::Context`]).
    pub fn context(&self, tuning: &crate::Tuning) -> Context {
        let o = &tuning.orchestrate;
        let fade = |(sure, since): (f32, f32), half_life: f32| sure * 0.5f32.powf(since / half_life.max(1.0e-3));
        let pressing: f32 = match self.alert {
            Some(Alert::Alert) => 1.0,
            Some(Alert::Evasion) => 0.6,
            Some(Alert::Caution) => 0.2,
            None => 0.0,
        };
        let remembered = self.memory.strength_near(Kind::Danger, self.feet, o.danger_near) * o.remembered_danger.clamp(0.0, 1.0);
        let threat = self.threat.max(remembered);
        Context {
            urgency: pressing.max(threat),
            threat,
            sighting: self.last_seen.map(|_| fade(self.sighting, o.sighting_half_life)),
            lead: self.investigation.map(|_| fade(self.lead, o.lead_half_life)),
            doing: self.conduct.map(|c| (c, self.doing_for)),
        }
    }

    /// It is told the player is about `at`, just now: by a radio call, an alarm. Unless it is
    /// after them already, it searches there next frame.
    pub fn tell(&mut self, at: Vector3<f32>) {
        self.tell_from(at, 0.0);
    }

    /// It is told the player was about `at`, `ago` seconds ago: by a witness that had to come and
    /// find it, say. It searches for them as far as they could have got since - the search map
    /// spread that far already - and is the less sure of it the older it is.
    pub fn tell_from(&mut self, at: Vector3<f32>, ago: f32) {
        if at.iter().all(|v| v.is_finite()) {
            let ago = if ago.is_finite() { ago.max(0.0) } else { 0.0 };
            self.told = Some((at, ago, f32::NAN));
            self.acting_on = None;
        }
    }

    /// It is told `word` of where the player was, at `now` on the game's clock (see
    /// [`word::Word`]): believed enough - fresh, firsthand or from a source it trusts, passed on
    /// few times - it searches there, as sure as it believes it, and will trust the source the
    /// more if it finds them and the less if not; otherwise it takes note, and stays as it was.
    /// After the player already, it takes no notice. Whether it acted on it.
    pub fn hear_word(&mut self, tuning: &crate::Tuning, word: Word<()>, now: f32) -> bool {
        if self.alert == Some(Alert::Alert) || !word.at.iter().all(|v| v.is_finite()) {
            return false;
        }
        let belief = word.belief(&tuning.word, &self.trust, now);
        if belief < tuning.orchestrate.believe {
            self.memory.note(&tuning.memory, Record::new(Kind::Event).at(word.at).secondhand().strength(belief.max(tuning.memory.forgotten)).importance(0.1));
            self.happened.push(Happened::Doubted);
            return false;
        }
        self.told = Some((word.at, word.age(now), belief));
        self.acting_on = (!word.firsthand()).then_some(word.from);
        true
    }

    /// A noise `nearness` near (see [`Heard::nearness`]): the more aware for it, and if it notices
    /// something for it, it goes to look at `at` - calm. Searching or watchful, it is a lead: one
    /// it may follow, or keep to its search, as its priorities weigh the two (see [`priority`]).
    /// After the player, it takes no notice.
    pub fn hear(&mut self, tuning: &crate::Tuning, at: Vector3<f32>, nearness: f32) {
        if !nearness.is_finite() || !at.iter().all(|v| v.is_finite()) {
            return;
        }
        let eagerness = self.eagerness(tuning);
        self.wariness.note(&tuning.alert, Incident::Heard);
        let noticed = self.detection.hear(&tuning.detection, nearness, eagerness) == Some(Change::Noticed);
        // Where it looked lately and found nothing, it is the less sure of it.
        let nearness = unit(nearness) * (1.0 - self.looked_lately(tuning, at) * tuning.orchestrate.recheck.clamp(0.0, 1.0));
        match self.alert {
            None if (noticed || self.investigation.is_some()) && nearness >= tuning.orchestrate.worth_a_look => (),
            Some(Alert::Evasion | Alert::Caution) if nearness > 0.0 => (),
            // Too faint to look into: it is attentive that way a while.
            None if nearness > 0.0 => {
                self.attentive = Some((tuning.orchestrate.attentive_for, at, 0.0));
                return;
            }
            _ => return,
        }
        let o = &tuning.orchestrate;
        let following = self.investigation.map(|_| self.lead.0 * 0.5f32.powf(self.lead.1 / o.lead_half_life.max(1.0e-3)));
        match following {
            // Nothing on: it goes to look.
            None => self.follow(tuning, at, nearness),
            // Clearly surer than what it is following, and not about to finish that: this first.
            Some(now) if !self.finishing_within(o.finish_within) && nearness >= now + o.switch_lead_by => {
                let was = self.investigation.map(|look| (look.at, now, 0.0));
                self.follow(tuning, at, nearness);
                self.wait(was);
            }
            // Otherwise it waits its turn.
            Some(_) => self.wait(Some((at, nearness, 0.0))),
        }
    }

    /// How strongly it remembers looking near `at` lately and finding nothing, from 0 to 1.
    fn looked_lately(&self, tuning: &crate::Tuning, at: Vector3<f32>) -> f32 {
        self.memory
            .near(Kind::Investigation, at, tuning.memory.same_within)
            .iter()
            .filter(|r| r.outcome.is_some_and(|o| o < 0.0))
            .map(|r| r.strength)
            .fold(0.0, f32::max)
    }

    /// It goes to look at `at`, as sure of it as `sure`.
    fn follow(&mut self, tuning: &crate::Tuning, at: Vector3<f32>, sure: f32) {
        self.investigation = Some(Investigation::new(&tuning.investigate, at));
        self.lead = (sure, 0.0);
        self.attentive = None;
        self.happened.push(Happened::Noticed);
    }

    /// `lead` waits its turn, if it is the surer of it and the one waiting already.
    fn wait(&mut self, lead: Option<(Vector3<f32>, f32, f32)>) {
        let sure = |(_, sure, _): (Vector3<f32>, f32, f32)| sure;
        if let Some(lead) = lead.filter(|&l| self.pending.is_none_or(|p| sure(l) >= sure(p))) {
            self.pending = Some(lead);
        }
    }

    /// Whether it is in the middle of something it should finish first: an investigation it is
    /// within `finish_within` of the spot of, or a search spot it is looking about.
    fn finishing_within(&self, finish_within: f32) -> bool {
        let near = |at: Vector3<f32>| flat(at - self.feet).norm() <= finish_within;
        match self.conduct {
            Some(Conduct::Investigate) => self.investigation.is_some_and(|look| near(look.at)),
            // A search is one spot after another: what it finishes is looking about the one it is
            // at, not every walk to the next - or, on a small level, it would never be free.
            Some(Conduct::Search) => self.at_spot && self.look.is_some_and(|look| !look.done()),
            _ => false,
        }
    }

    /// Where it is going to look, or looking, into something it noticed, if anywhere.
    pub fn investigating(&self) -> Option<Vector3<f32>> {
        self.investigation.map(|look| look.at)
    }

    /// Whether it is attentive after a disturbance.
    pub fn attentive(&self) -> bool {
        self.attentive.is_some()
    }

    /// There is no way to where it was going: it gives it up - an investigation, as if done; a
    /// search spot, for another; a post of its beat, for the next.
    pub fn no_way(&mut self, tuning: &crate::Tuning) {
        match self.conduct {
            Some(Conduct::Investigate) if self.investigation.is_some() => {
                let spot = self.investigation.map(|look| look.at);
                self.investigation_done(tuning, spot);
            }
            _ if !self.beat.is_empty() && self.goal.is_some() && self.alert.is_none() => {
                self.post = (self.post + 1) % self.beat.len();
                self.goal = None;
            }
            _ => self.goal = None,
        }
    }

    fn eagerness(&self, tuning: &crate::Tuning) -> f32 {
        let searching = if self.alert == Some(Alert::Evasion) { tuning.detection.searching } else { 1.0 };
        let attentive = if self.attentive.is_some() { tuning.orchestrate.attentive_keenness.max(0.0) } else { 1.0 };
        self.wariness.keenness(&tuning.alert) * searching * attentive
    }

    /// It looked at `spot`, and found nothing: it stays attentive that way a while, and takes up
    /// the lead that was waiting, if it has not gone cold.
    fn investigation_done(&mut self, tuning: &crate::Tuning, spot: Option<Vector3<f32>>) {
        self.investigation = None;
        self.happened.push(Happened::InvestigationDone);
        self.experience.report(tuning, None, Outcome::came_to_nothing(Venture::Investigation));
        if let Some(spot) = spot {
            self.memory.note(&tuning.memory, Record::new(Kind::Investigation).at(spot).outcome(crate::think::experience::UNPRODUCTIVE).importance(0.2));
        }
        let o = &tuning.orchestrate;
        let left = self.pending.take().map(|(at, sure, since)| (at, sure * 0.5f32.powf(since / o.lead_half_life.max(1.0e-3))));
        match left.filter(|&(_, sure)| sure >= LEAD_FORGOTTEN) {
            Some((at, sure)) => self.follow(tuning, at, sure),
            None => self.attentive = spot.map(|spot| (o.attentive_for, spot, 0.0)),
        }
    }

    fn lost_at(&mut self, tuning: &crate::Tuning, search: &mut SearchMap, mesh: &NavMesh, at: Vector3<f32>, going: Option<Vector3<f32>>) {
        self.left = self.wariness.lasts(&tuning.alert, Alert::Evasion).unwrap_or_default();
        self.goal = None;
        self.look = None;
        search.lose(mesh, at, going);
        self.haunts = self.recall_haunts(tuning, at);
    }

    /// Where, about `at`, it remembers finding the player before - a search or a look that turned
    /// them up - and losing them before, as much as each fits (see [`Memory::recall_for`]).
    pub fn recall_haunts(&self, tuning: &crate::Tuning, at: Vector3<f32>) -> Vec<(Vector3<f32>, f32)> {
        let o = &tuning.orchestrate;
        let cue = Cue::of(&[Kind::Investigation, Kind::Place]).near(at, o.recall_within);
        self.memory
            .recall_for(&cue, o.recall_most)
            .into_iter()
            .filter(|(_, r)| r.kind == Kind::Place || r.outcome.is_some_and(|o| o > 0.0))
            .filter_map(|(score, r)| r.at.map(|at| (at, score)))
            .collect()
    }

    /// How much more this searcher makes of polygon `n` of `mesh`, for what it recalled when it lost
    /// the player: from 1, up to one and the tuning's `memory_lean`, for a polygon within
    /// `lean_near` of where it found or lost them before.
    fn weigh_by_memory(&self, tuning: &crate::Tuning, mesh: &NavMesh, n: crate::navigation::navmesh::Poly) -> f32 {
        let o = &tuning.orchestrate;
        // How far each remembered spot is from the polygon - nothing if it is in it - rather than
        // from its middle: a long polygon is near what is near any part of it.
        let p = mesh.polygon(n);
        let (cs, origin) = (mesh.cell_size, mesh.origin);
        let (x0, x1) = (origin.x + p.x0 as f32 * cs, origin.x + p.x1 as f32 * cs);
        let (z0, z1) = (origin.z + p.z0 as f32 * cs, origin.z + p.z1 as f32 * cs);
        let away = |at: Vector3<f32>| (x0 - at.x).max(at.x - x1).max(0.0).hypot((z0 - at.z).max(at.z - z1).max(0.0));
        let fit = self.haunts.iter().filter(|&&(at, _)| away(at) <= o.lean_near).map(|&(_, s)| s).fold(0.0, f32::max);
        1.0 + o.memory_lean.max(0.0) * fit
    }

    /// Takes in what it senses this frame: awareness, the alert phase, and anything it was told.
    /// `relayed`: it sees another sentry after the player, and so knows where they are as if it
    /// saw them itself - it stays on the chase as long as it does.
    fn perceive(&mut self, tuning: &crate::Tuning, senses: &Senses, relayed: bool, search: &mut SearchMap, mesh: &NavMesh, dt: f32) {
        let dt = if dt.is_finite() { dt.max(0.0) } else { 0.0 };
        if senses.feet.iter().all(|v| v.is_finite()) {
            self.feet = senses.feet;
        }
        if senses.heading.is_finite() {
            self.heading = senses.heading;
        }
        let player_known = senses.player.iter().all(|v| v.is_finite());
        self.threat = unit(senses.threat);
        self.memory.step(&tuning.memory, dt);
        if self.threat >= tuning.orchestrate.danger_noted && senses.feet.iter().all(|v| v.is_finite()) {
            self.memory.note(&tuning.memory, Record::new(Kind::Danger).at(senses.feet).strength(self.threat).importance(self.threat));
        }
        self.sighting.1 += dt;
        self.lead.1 += dt;
        if let Some((_, _, since)) = self.pending.as_mut() {
            *since += dt;
        }
        if let Some((left, _, _)) = self.attentive.as_mut() {
            *left -= dt;
        }
        self.attentive = self.attentive.filter(|&(left, _, _)| left > 0.0);
        let visible = if senses.visible.is_finite() && player_known { senses.visible.max(0.0) } else { 0.0 };
        self.wariness.step(&tuning.alert, dt);
        let eagerness = self.eagerness(tuning);
        let change = self.detection.step(&tuning.detection, visible, eagerness, dt);
        let sees = match self.alert {
            Some(Alert::Alert) => visible > 0.0 || relayed && player_known,
            _ => self.detection.spotted() && visible > 0.0,
        };
        match change {
            Some(Change::Noticed) if self.alert.is_none() => {
                self.happened.push(Happened::Noticed);
                self.wariness.note(&tuning.alert, Incident::Noticed);
                self.investigation.get_or_insert(Investigation::new(&tuning.investigate, senses.player));
                self.lead = (self.detection.level, 0.0);
            }
            Some(Change::Spotted) if self.alert.is_none() => {
                self.happened.push(Happened::Spotted);
                self.wariness.note(&tuning.alert, Incident::Spotted);
                self.memory.note(&tuning.memory, Record::new(Kind::Encounter).at(senses.player).importance(0.7));
                if let Some(look) = self.investigation {
                    // What it went to look into turned out to be them.
                    self.memory.note(&tuning.memory, Record::new(Kind::Investigation).at(look.at).outcome(1.0).importance(0.8));
                }
                self.investigation = None;
                self.pending = None;
                self.attentive = None;
                self.acting_on = None;
                self.alert = Some(Alert::Alert);
                self.goal = None;
                self.look = None;
            }
            _ if visible > 0.0 => {
                if let Some(look) = self.investigation.as_mut() {
                    look.saw_again(&tuning.investigate, senses.player);
                    self.lead = (self.lead.0.max(self.detection.level), 0.0);
                }
            }
            _ => (),
        }
        if sees {
            if let Some(was) = self.last_seen {
                // Which way they are going, from how they have moved lately.
                self.going = self.going * (-4.0 * dt).exp() + flat(senses.player - was);
            }
            self.last_seen = Some(senses.player);
            self.sighting = (if visible > 0.0 { 1.0 } else { 0.8 }, 0.0);
        }
        // Told where to look: it searches there.
        if let Some((there, ago, sure)) = self.told.take().filter(|_| self.alert != Some(Alert::Alert)) {
            self.wariness.note(&tuning.alert, Incident::Alarm);
            self.alert = Some(Alert::Evasion);
            self.last_seen = Some(there);
            let sure = if sure.is_finite() { sure } else { tuning.orchestrate.told };
            self.sighting = (unit(sure), ago);
            self.going = Vector3::zeros();
            self.investigation = None;
            self.lost_at(tuning, search, mesh, there, None);
            // As far as they could have got since.
            search.spread(mesh, tuning.orchestrate.getaway.max(0.0) * ago);
            self.memory.note(&tuning.memory, Record::new(Kind::Event).at(there).secondhand().importance(0.4));
            self.happened.push(Happened::Told);
        }
        // The phases.
        if let Some(phase) = self.alert {
            if phase != Alert::Alert {
                self.left -= dt;
            }
            let next = phase.next(sees, self.left);
            match (phase, next) {
                (Alert::Alert, Some(Alert::Evasion)) => {
                    self.happened.push(Happened::LostContact);
                    self.wariness.note(&tuning.alert, Incident::Lost);
                    let at = self.last_seen.unwrap_or(self.feet);
                    let going = (self.going.norm() > 1.0e-3).then_some(self.going);
                    self.lost_at(tuning, search, mesh, at, going);
                    self.memory.note(&tuning.memory, Record::new(Kind::Place).at(at).importance(0.5));
                    self.experience.report(tuning, Some(mesh), Outcome::failed(Venture::Pursuit).at(at));
                }
                (Alert::Evasion | Alert::Caution, Some(Alert::Alert)) => {
                    self.happened.push(Happened::FoundAgain);
                    // Found again: searching, a search that turned them up there.
                    let found = if phase == Alert::Evasion { Kind::Investigation } else { Kind::Encounter };
                    self.memory.note(&tuning.memory, Record::new(found).at(senses.player).outcome(1.0).importance(0.8));
                    self.goal = None;
                    self.look = None;
                    if phase == Alert::Evasion {
                        self.experience.report(tuning, Some(mesh), Outcome::succeeded(Venture::Search).at(senses.player));
                        // Searching on someone's word, and there they were: their word was good.
                        if let Some(source) = self.acting_on.take() {
                            self.trust.confirmed(&tuning.word, source);
                        }
                    }
                }
                (Alert::Evasion, Some(Alert::Caution)) => {
                    self.happened.push(Happened::GaveUp);
                    self.left = self.wariness.lasts(&tuning.alert, Alert::Caution).unwrap_or_default();
                    self.goal = None;
                    self.look = None;
                    self.experience.report(tuning, None, Outcome::came_to_nothing(Venture::Search));
                    // Searching on someone's word, and nothing: their word was no good.
                    if let Some(source) = self.acting_on.take() {
                        self.trust.refuted(&tuning.word, source);
                    }
                }
                (Alert::Caution, None) => {
                    self.happened.push(Happened::Calm);
                    self.detection.reset();
                    self.last_seen = None;
                    self.going = Vector3::zeros();
                    self.goal = None;
                }
                _ => (),
            }
            self.alert = next;
        }
        self.experience.step(tuning, dt);
    }

    /// It saw another sentry after the player, at `player`, and joins in - and if it came on
    /// someone's word, the word was good.
    fn join(&mut self, tuning: &crate::Tuning, player: Vector3<f32>) {
        if let Some(source) = self.acting_on.take() {
            self.trust.confirmed(&tuning.word, source);
        }
        self.alert = Some(Alert::Alert);
        self.detection.alert();
        self.investigation = None;
        self.last_seen = Some(player);
        self.goal = None;
        self.look = None;
        self.happened.push(Happened::Joined);
    }

    /// Chooses what to do, and what that means it does this frame.
    fn decide(&mut self, tuning: &crate::Tuning, reasoners: &mut Reasoners, search: &SearchMap, mesh: &NavMesh, taken: &[Vector3<f32>], dt: f32) -> (Intent, Conduct) {
        let t = &tuning.orchestrate;
        let reading = Reading {
            hunting: self.alert == Some(Alert::Alert),
            investigating: self.investigation.is_some() && self.alert != Some(Alert::Alert),
            searching: self.alert == Some(Alert::Evasion),
            nerve: self.nerve,
            wariness: self.wariness.level,
            ..Reading::default()
        };
        let context = self.context(tuning);
        let traits = self.traits;
        // Persistence: what it is in the middle of counts the more, so that it finishes it before
        // anything new but what shuts it (a sighting) or clearly outweighs it.
        let finishing = self.finishing_within(t.finish_within).then_some(self.conduct).flatten();
        let hold = 1.0 + t.hold.max(0.0);
        let conduct = self.experience.choose_with(tuning, reasoners, self.role, &reading, self.conduct, |c| {
            priority::weigh(&tuning.priority, &traits, &context, c) * if finishing == Some(c) { hold } else { 1.0 }
        });
        if self.conduct != Some(conduct) {
            self.goal = None;
            self.look = None;
            self.doing_for = 0.0;
        } else {
            self.doing_for += dt;
        }
        // A lead passed over till it is all but cold is let go.
        if conduct != Conduct::Investigate && context.lead.is_some_and(|lead| lead < LEAD_FORGOTTEN) {
            self.investigation = None;
        }
        self.conduct = Some(conduct);
        let there = |goal: Vector3<f32>, feet: Vector3<f32>| flat(goal - feet).norm() <= t.arrived;
        let intent = match conduct {
            Conduct::Chase => Intent::GoTo { at: self.last_seen.unwrap_or(self.feet), pace: Pace::Run },
            Conduct::Investigate => match self.investigation.take() {
                Some(mut look) => match look.step(&tuning.investigate, self.feet, self.heading, dt) {
                    Doing::Face { heading } | Doing::LookAround { heading } => {
                        self.investigation = Some(look);
                        Intent::Face { heading }
                    }
                    Doing::GoTo { at, .. } => {
                        self.investigation = Some(look);
                        Intent::GoTo { at, pace: Pace::Walk }
                    }
                    Doing::Done => {
                        self.investigation_done(tuning, Some(look.at));
                        Intent::Face { heading: self.heading }
                    }
                },
                None => Intent::Routine,
            },
            Conduct::Search => {
                if let Some(heading) = self.look.as_mut().and_then(|look| look.step(dt)) {
                    Intent::Face { heading }
                } else {
                    self.look = None;
                    if self.goal.is_some_and(|goal| there(goal, self.feet)) {
                        // Got there: it looks about.
                        self.goal = None;
                        self.look = Some(LookAbout::searching(&tuning.search, self.heading));
                        self.at_spot = true;
                        Intent::Face { heading: self.heading }
                    } else {
                        if self.goal.is_none() {
                            // Not where it stands: it has just looked about there. (A polygon's
                            // worth counts the chance next to it too, so the one it stands in,
                            // between places it cannot see into from there, would otherwise be
                            // chosen again and again, and it would never go into them.)
                            let weigh = |n| self.weigh_by_memory(tuning, mesh, n);
                            let mut taken = taken.to_vec();
                            taken.push(self.feet);
                            self.goal = (!search.is_empty()).then(|| search.best_by(mesh, self.feet, &taken, t.apart, t.within, weigh)).flatten();
                        }
                        match self.goal {
                            Some(at) => Intent::GoTo { at, pace: Pace::Search },
                            // Nowhere likelier than where it is: it looks about here.
                            None => {
                                self.look = Some(LookAbout::searching(&tuning.search, self.heading));
                                self.at_spot = false;
                                Intent::Face { heading: self.heading }
                            }
                        }
                    }
                }
            }
            // Attentive after a disturbance: first it stands looking that way a moment.
            _ if self.attentive.is_some_and(|(_, _, stood)| stood < t.linger) => {
                let (left, toward, stood) = self.attentive.expect("attentive");
                self.attentive = Some((left, toward, stood + dt));
                Intent::Face { heading: crate::heading_of(toward - self.feet).unwrap_or(self.heading) }
            }
            _ if self.beat.is_empty() => Intent::Routine,
            _ => {
                let post = self.beat[self.post % self.beat.len()];
                if there(post, self.feet) {
                    self.post = (self.post + 1) % self.beat.len();
                }
                let post = self.beat[self.post];
                self.goal = Some(post);
                Intent::GoTo { at: post, pace: Pace::Walk }
            }
        };
        (intent, conduct)
    }
}

/// What a squad of sentries shares: the search map they search by - one hunt at a time, the last
/// sentry to lose the player or be told where they are setting it afresh - the clock it is looked
/// over by, and the clock word is timed by. A sentry's place in the squad's list is its id in the
/// word it passes on: give word from others than the squad ids of their own beyond it.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Squad {
    pub search: SearchMap,
    look_in: f32,
    /// The squad's clock, in seconds since it was made: when word of a sighting was seen.
    #[cfg_attr(feature = "serde", serde(default))]
    now: f32,
    /// Where and when the player was last lost, or last told of: to search from afresh if the
    /// search map comes to nothing.
    #[cfg_attr(feature = "serde", serde(default))]
    hunt: Option<(Vector3<f32>, f32)>,
}

impl Squad {
    /// No one lost yet.
    pub fn new(tuning: &crate::Tuning) -> Self {
        Self { search: SearchMap::with(tuning.search), look_in: 0.0, now: 0.0, hunt: None }
    }

    /// The squad's clock, in seconds since it was made: the time to give word seen now (see
    /// [`Sentry::hear_word`]).
    pub fn now(&self) -> f32 {
        self.now
    }

    /// A noise at `at`, carrying `loudness` meters along the floor of `mesh`: each of `sentries`
    /// that hears it hears it (see [`Sentry::hear`]), by where it stood last frame.
    pub fn noise(&self, tuning: &crate::Tuning, mesh: &NavMesh, sentries: &mut [Sentry], at: Vector3<f32>, loudness: f32) {
        if !loudness.is_finite() || loudness <= 0.0 {
            return;
        }
        let Some(heard) = Heard::at(mesh, at, loudness) else { return };
        for sentry in sentries {
            if let Some(nearness) = heard.nearness(mesh, sentry.feet) {
                sentry.hear(tuning, at, nearness);
            }
        }
    }

    /// Moves the squad on `dt` seconds: each of `sentries` takes in what `senses` says it does -
    /// the one by the other, any left over on either side left out - those that see another after
    /// the player join in (`seeing`: what there is to see by; `clear(a, b)`: whether nothing is in
    /// the way from sentry `a` to `b`, the game's ray), the search map spreads and is looked over,
    /// and each chooses what to do. A [`Thought`] for each sentry that had senses, in order.
    #[allow(clippy::too_many_arguments)]
    pub fn step(
        &mut self,
        tuning: &crate::Tuning,
        mesh: &NavMesh,
        reasoners: &mut Reasoners,
        sentries: &mut [Sentry],
        senses: &[Senses],
        seeing: impl Into<Seeing>,
        clear: impl Fn(usize, usize) -> bool,
        dt: f32,
    ) -> Vec<Thought> {
        let seeing = seeing.into();
        let dt = if dt.is_finite() { dt.max(0.0) } else { 0.0 };
        let n = sentries.len().min(senses.len());
        let (sentries, senses) = (&mut sentries[..n], &senses[..n]);
        // Joining in a chase it sees: each sentry that sees another after the player - that can
        // see them - knows where they are through it, as if it saw them itself. One not after them
        // yet joins in; one already after them stays after them as long as it sees the chase,
        // rather than losing them the frame it joins and joining again the next.
        let watches: Vec<Watch> = sentries
            .iter()
            .zip(senses)
            .map(|(s, sense)| Watch {
                sentry: s.role == Role::Sentry,
                alert: s.alert,
                sees_player: s.alert == Some(Alert::Alert) && sense.visible.is_finite() && sense.visible > 0.0,
                busy: false,
                windup: 0.0,
                feet: if sense.feet.iter().all(|v| v.is_finite()) { sense.feet } else { s.feet },
                heading: if sense.heading.is_finite() { sense.heading } else { s.heading },
            })
            .collect();
        let mut relayed = vec![false; n];
        for joiner in joiners(&watches, seeing, &clear) {
            relayed[joiner] = true;
            if sentries[joiner].alert != Some(Alert::Alert) && senses[joiner].player.iter().all(|v| v.is_finite()) {
                sentries[joiner].join(tuning, senses[joiner].player);
            }
        }
        for ((sentry, sense), &relayed) in sentries.iter_mut().zip(senses).zip(&relayed) {
            sentry.perceive(tuning, sense, relayed, &mut self.search, mesh, dt);
        }
        self.now += dt;
        // Where the hunt starts from now: wherever one lost them, or was told of them, this frame.
        for sentry in sentries.iter() {
            let set_off = sentry.happened.iter().any(|h| matches!(h, Happened::LostContact | Happened::Told));
            if let Some(at) = sentry.last_seen.filter(|_| set_off) {
                let told_ago = if sentry.happened.contains(&Happened::Told) { sentry.sighting.1 } else { 0.0 };
                self.hunt = Some((at, self.now - told_ago));
            }
        }
        // Word of a sighting: each that saw the player for itself this frame - spotting them, or
        // finding them again - passes it on to each it reaches: by radio, both having one, or
        // by voice, near enough and nothing in the way (`clear`). Each takes it as its own
        // trust of the one telling, and how fresh and sure it is, has it.
        for i in 0..n {
            let saw = sentries[i].happened.iter().any(|h| matches!(h, Happened::Spotted | Happened::FoundAgain));
            if !saw || !senses[i].player.iter().all(|v| v.is_finite()) {
                continue;
            }
            let word = Word::seen((), senses[i].player, self.now, i as u32).passed_on(&tuning.word, i as u32);
            let (from, radio) = (sentries[i].feet, sentries[i].radio);
            for j in (0..n).filter(|&j| j != i) {
                let (to, theirs) = (sentries[j].feet, sentries[j].radio);
                let reached = word::reaches(&tuning.word, Channel::Radio, from, to, (radio, theirs), || false)
                    || word::reaches(&tuning.word, Channel::Voice, from, to, (radio, theirs), || clear(i, j));
                if reached {
                    sentries[j].hear_word(tuning, word, self.now);
                }
            }
        }
        // The search: where the player could be spreading as fast as they could go, cleared where
        // the searchers look and do not see them - on a clock of its own, as often at any frame rate.
        let o = &tuning.orchestrate;
        let hunting = sentries.iter().any(|s| s.alert == Some(Alert::Evasion));
        // Searched everywhere they could have been, and not found: they have had time to get
        // further. Search afresh from where they were lost, as far as they could have gone since -
        // rather than stand about looking at nothing.
        if hunting && self.search.is_empty() {
            if let Some((at, when)) = self.hunt {
                self.search.lose(mesh, at, None);
                self.search.spread(mesh, o.getaway.max(0.0) * (self.now - when).max(0.0));
            }
        }
        if !hunting {
            self.hunt = None;
        }
        if hunting && !self.search.is_empty() {
            self.search.spread(mesh, o.getaway * dt);
            self.look_in -= dt;
            let reach = tuning.sight.reach(Some(Alert::Evasion), Stance::Standing, seeing.dark).min(o.search_sight);
            while self.look_in <= 0.0 {
                self.look_in += o.look_every.max(1.0e-3);
                for s in sentries.iter().filter(|s| s.alert == Some(Alert::Evasion)) {
                    self.search.look(mesh, s.feet, s.heading, (reach, tuning.sight.cone, 2.0));
                }
            }
        } else {
            self.look_in = 0.0;
        }
        // Each chooses what to do - searchers clear of where the others are going, including where
        // those that chose before it this frame have just set off for.
        let mut goals: Vec<Option<Vector3<f32>>> = sentries.iter().map(|s| s.goal.filter(|_| s.alert == Some(Alert::Evasion))).collect();
        let mut taken = Vec::new();
        let mut thoughts = Vec::with_capacity(n);
        for (i, sentry) in sentries.iter_mut().enumerate() {
            taken.clear();
            taken.extend(goals.iter().enumerate().filter(|&(j, _)| j != i).filter_map(|(_, g)| *g));
            let (intent, conduct) = sentry.decide(tuning, reasoners, &self.search, mesh, &taken, dt);
            goals[i] = sentry.goal.filter(|_| sentry.alert == Some(Alert::Evasion));
            thoughts.push(Thought { intent, conduct, happened: std::mem::take(&mut sentry.happened) });
        }
        thoughts
    }
}

/// `x` brought to 0 to 1, nothing that is not a number.
fn unit(x: f32) -> f32 {
    if x.is_nan() {
        0.0
    } else {
        x.clamp(0.0, 1.0)
    }
}
