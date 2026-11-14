//! Sharing out the AI's time: which NPCs think this frame, and how much costly work - routes,
//! searches over the navmesh, surveys - may be done in it (ROADMAP 7.3 and 7.4).
//!
//! With many NPCs, thinking for every one every frame is more than a frame has room for, and
//! most of it is wasted on NPCs far from anything that matters. So each NPC is given an
//! importance by the game, from 0 to 1 - fighting, near the player or the camera, or being shot
//! at, the more - and put by it in one of four tiers ([`Tuning::importance`]). The first thinks
//! every frame; each other every so many frames ([`Tuning::every`]), each NPC on a frame of its
//! own in the cycle, so that the load is even rather than all of one tier at once. An NPC that
//! thinks only now and then is handed the time since it last did, so that its timers run as
//! they would; no more than [`Tuning::most_owed`], so that after a long while asleep it does not
//! leap. One that becomes more important than it was thinks at once, not when its turn comes.
//!
//! Work that costs a great deal more than thinking - a route, a search over the navmesh, a
//! survey - is drawn from the frame's allowance of it ([`Tuning::allowance`], by [`Work`]):
//! [`Scheduler::take`] grants it while there is some left, and says no once there is not, for
//! work that can wait - an NPC with a route to follow keeps it a frame longer. Work that cannot
//! wait - an NPC with no route at all - goes ahead whatever is left, through
//! [`Scheduler::force`], and is counted as overdrawn. Counted in units of work, not timed, so
//! that the same game runs the same way every time.
//!
//! What each frame came to - how many thought and how many did not, what was granted, refused
//! and overdrawn - is in [`Scheduler::stats`], to see where a frame's time goes.
//!
//! [`Tuning::DEFAULT`] is the full profile; [`Tuning::LOW`], for slower machines and bigger
//! crowds, thinks less often and does less a frame (ROADMAP 7.4).

/// Kinds of costly work a frame has an allowance of.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Work {
    /// A route over the navmesh ([`crate::navigation::navmesh::NavMesh::route`]).
    Route,
    /// A search outward over the navmesh: what can be got to, how far everything is
    /// ([`crate::navigation::navmesh::NavMesh::costs_from`]), high ground, cover.
    Search,
    /// A slice of a survey of the ground, such as which of it enemies can see
    /// ([`crate::tactics::team::Exposure::survey_part`]), in polygons.
    Survey,
    /// A look about: sight checks, and the rays the game casts for them.
    Look,
}

impl Work {
    /// Every kind, in the order of [`Tuning::allowance`].
    pub const ALL: [Work; WORK_KINDS] = [Work::Route, Work::Search, Work::Survey, Work::Look];

    /// Where it is among [`Work::ALL`].
    pub fn index(self) -> usize {
        self as usize
    }
}

/// How many kinds of [`Work`] there are.
pub const WORK_KINDS: usize = 4;

/// How many tiers of importance there are.
pub const TIERS: usize = 4;

tuning! {
    /// How the AI's time is shared out.
    {
        /// How important an NPC has to be, at least, to be in each tier but the last, most
        /// important first: at 1 always the first.
        importance: [f32; TIERS - 1] = [0.66, 0.33, 0.1],
        /// How often an NPC in each tier thinks, in frames: the first every frame.
        every: [u32; TIERS] = [1, 2, 4, 8],
        /// The most time an NPC that thinks only now and then is handed at once, in seconds.
        most_owed: f32 = 0.25,
        /// How much of each kind of work a frame allows, in the order of [`Work::ALL`]: routes,
        /// searches, polygons of survey, looks about.
        allowance: [u32; WORK_KINDS] = [12, 4, 2000, 64],
        /// How near, in meters, an NPC is still fully important for being near ([`importance`]);
        /// and how far, least.
        near: f32 = 20.0,
        far: f32 = 120.0,
    }
}

impl Tuning {
    /// The full profile: [`Tuning::DEFAULT`].
    pub const FULL: Self = Self::DEFAULT;

    /// For slower machines and bigger crowds: every tier but the first thinks half as often,
    /// and a frame allows half the work.
    pub const LOW: Self = Self {
        importance: [0.75, 0.4, 0.15],
        every: [1, 4, 8, 16],
        most_owed: 0.4,
        allowance: [6, 2, 1000, 32],
        near: 15.0,
        far: 90.0,
    };

    /// Which tier an NPC of `importance` is in: 0, the first, the most important.
    pub fn tier(&self, importance: f32) -> usize {
        self.importance.iter().position(|&least| importance >= least).unwrap_or(TIERS - 1)
    }
}

/// How important an NPC is, from 0 to 1, by how far it is from what matters - the player, the
/// camera - and whether it is caught up in something (fighting, being shot at, after someone):
/// caught up, fully; otherwise fully within [`Tuning::near`], less and less out to
/// [`Tuning::far`], and least beyond. A game may weigh it as it likes; this is a start.
pub fn importance(tuning: &Tuning, distance: f32, caught_up: bool) -> f32 {
    if caught_up {
        return 1.0;
    }
    let span = (tuning.far - tuning.near).max(1.0e-3);
    (1.0 - (distance - tuning.near) / span).clamp(0.0, 1.0)
}

/// What a frame came to.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Stats {
    /// How many NPCs thought, and how many did not, it not being their turn.
    pub thought: u32,
    pub waited: u32,
    /// How many NPCs were in each tier.
    pub tiers: [u32; TIERS],
    /// Of each kind of work, in the order of [`Work::ALL`]: how much was granted within the
    /// allowance, how much refused, and how much forced through past it.
    pub granted: [u32; WORK_KINDS],
    pub refused: [u32; WORK_KINDS],
    pub overdrawn: [u32; WORK_KINDS],
}

/// Shares out the AI's time, frame by frame (see the module's documentation). One for all the
/// NPCs it schedules, each known by its index.
#[derive(Debug, Clone, Default)]
pub struct Scheduler {
    pub tuning: Tuning,
    frame: u64,
    /// For each NPC: the time owed it since it last thought, in seconds, and the tier it was in.
    owed: Vec<f32>,
    tier: Vec<u8>,
    left: [u32; WORK_KINDS],
    stats: Stats,
    last: Stats,
}

impl Scheduler {
    pub fn new(tuning: Tuning) -> Self {
        Self { tuning, ..Self::default() }
    }

    /// Starts a frame: the last frame's [`Stats`] kept, and the allowance of work renewed.
    pub fn begin_frame(&mut self) {
        self.frame += 1;
        self.last = std::mem::take(&mut self.stats);
        self.left = self.tuning.allowance;
    }

    /// Whether NPC `n`, as important as `importance`, thinks this frame, `dt` seconds after the
    /// last: if so, with how long it is since it last did - `dt`, every frame, for one in the
    /// first tier - for it to think for; None if it waits. See the module's documentation.
    pub fn due(&mut self, n: usize, importance: f32, dt: f32) -> Option<f32> {
        if self.owed.len() <= n {
            self.owed.resize(n + 1, 0.0);
            self.tier.resize(n + 1, (TIERS - 1) as u8);
        }
        let tier = self.tuning.tier(importance);
        self.stats.tiers[tier] += 1;
        let promoted = tier < self.tier[n] as usize;
        self.tier[n] = tier as u8;
        self.owed[n] += dt;
        let every = self.tuning.every[tier].max(1) as u64;
        // Each NPC on a frame of its own in the cycle: spread by a mix of its index, so that
        // neighbours by index - a team, made one after another - are not all on the same one.
        let turn = (self.frame + spread(n)) % every == 0;
        if turn || promoted {
            self.stats.thought += 1;
            Some(std::mem::take(&mut self.owed[n]).min(self.tuning.most_owed.max(dt)))
        } else {
            self.stats.waited += 1;
            None
        }
    }

    /// Forgets NPC `n`'s owed time and tier: it is gone, or starting afresh.
    pub fn reset(&mut self, n: usize) {
        if let Some(owed) = self.owed.get_mut(n) {
            *owed = 0.0;
            self.tier[n] = (TIERS - 1) as u8;
        }
    }

    /// Takes one of `work` from the frame's allowance, if there is any left: whether it may be
    /// done. Refused, it should wait for a later frame.
    pub fn take(&mut self, work: Work) -> bool {
        self.take_some(work, 1) > 0
    }

    /// Takes up to `amount` of `work` from the frame's allowance: how much of it may be done.
    pub fn take_some(&mut self, work: Work, amount: u32) -> u32 {
        let k = work.index();
        let granted = amount.min(self.left[k]);
        self.left[k] -= granted;
        self.stats.granted[k] += granted;
        self.stats.refused[k] += amount - granted;
        granted
    }

    /// Does one of `work` whatever is left - work that cannot wait - counting what goes past the
    /// allowance as overdrawn.
    pub fn force(&mut self, work: Work) {
        let k = work.index();
        if self.left[k] > 0 {
            self.left[k] -= 1;
            self.stats.granted[k] += 1;
        } else {
            self.stats.overdrawn[k] += 1;
        }
    }

    /// How much of `work` is left in this frame's allowance.
    pub fn left(&self, work: Work) -> u32 {
        self.left[work.index()]
    }

    /// What the last whole frame came to.
    pub fn stats(&self) -> Stats {
        self.last
    }
}

/// Where in a cycle NPC `n`'s turn falls, as an offset: its index mixed, so that NPCs made one
/// after another are spread through it.
fn spread(n: usize) -> u64 {
    (n as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn importance_puts_it_in_a_tier() {
        let t = Tuning::DEFAULT;
        assert_eq!(t.tier(1.0), 0);
        assert_eq!(t.tier(0.5), 1);
        assert_eq!(t.tier(0.2), 2);
        assert_eq!(t.tier(0.0), 3);
        assert_eq!(importance(&t, 500.0, true), 1.0, "caught up in something");
        assert_eq!(importance(&t, 10.0, false), 1.0, "near");
        assert_eq!(importance(&t, 500.0, false), 0.0, "far off");
        let half = importance(&t, 70.0, false);
        assert!((half - 0.5).abs() < 1.0e-3, "{half}");
    }

    #[test]
    fn each_tier_thinks_as_often_as_it_says_spread_evenly_through_the_cycle() {
        let mut s = Scheduler::new(Tuning::DEFAULT);
        let dt = 1.0 / 60.0;
        let npcs = 64;
        let mut per_frame = Vec::new();
        let mut thought = vec![0; npcs];
        for _ in 0..80 {
            s.begin_frame();
            let mut now = 0;
            for (n, count) in thought.iter_mut().enumerate() {
                if s.due(n, 0.0, dt).is_some() {
                    *count += 1;
                    now += 1;
                }
            }
            per_frame.push(now);
        }
        // The last tier thinks every 8th frame: 10 times in 80, each NPC.
        assert!(thought.iter().all(|&c| (9..=11).contains(&c)), "{thought:?}");
        // And not all at once: about an eighth of them each frame.
        let most = *per_frame[1..].iter().max().unwrap();
        assert!(most <= npcs / 8 * 2 + 2, "{per_frame:?}");
        // The first tier, every frame.
        s.begin_frame();
        assert!((0..npcs).all(|n| s.due(n, 1.0, dt).is_some()));
    }

    #[test]
    fn it_is_handed_the_time_it_waited_and_no_more_than_most_owed() {
        let mut s = Scheduler::new(Tuning::DEFAULT);
        let dt = 1.0 / 60.0;
        let mut handed = 0.0;
        for _ in 0..800 {
            s.begin_frame();
            if let Some(t) = s.due(3, 0.0, dt) {
                handed += t;
            }
        }
        // Every frame's time, but for what it is still owed since its last turn.
        assert!((handed - 800.0 * dt).abs() < 8.0 * dt + 1.0e-3, "{handed}");
        // Asleep a long while, it is handed no more than most_owed at once.
        let mut s = Scheduler::new(Tuning::DEFAULT);
        let mut longest: f32 = 0.0;
        for _ in 0..40 {
            s.begin_frame();
            if let Some(t) = s.due(0, 0.0, 0.1) {
                longest = longest.max(t);
            }
        }
        assert!(longest <= Tuning::DEFAULT.most_owed + 1.0e-6, "{longest}");
    }

    #[test]
    fn one_that_becomes_more_important_thinks_at_once() {
        let mut s = Scheduler::new(Tuning::DEFAULT);
        let dt = 1.0 / 60.0;
        // Find a frame on which NPC 5, far off, waits.
        loop {
            s.begin_frame();
            if s.due(5, 0.0, dt).is_none() {
                break;
            }
        }
        s.begin_frame();
        assert!(s.due(5, 1.0, dt).is_some(), "shot at: it thinks now");
    }

    #[test]
    fn work_is_granted_while_there_is_allowance_and_forced_past_it() {
        let mut s = Scheduler::new(Tuning { allowance: [2, 0, 10, 0], ..Tuning::DEFAULT });
        s.begin_frame();
        assert!(s.take(Work::Route));
        assert!(s.take(Work::Route));
        assert!(!s.take(Work::Route), "none left: it waits");
        s.force(Work::Route);
        assert_eq!(s.take_some(Work::Survey, 25), 10);
        assert!(!s.take(Work::Search));
        s.begin_frame();
        let stats = s.stats();
        assert_eq!(stats.granted, [2, 0, 10, 0]);
        assert_eq!(stats.refused, [1, 1, 15, 0]);
        assert_eq!(stats.overdrawn, [1, 0, 0, 0]);
        assert_eq!(s.left(Work::Route), 2, "a new frame, a new allowance");
    }

    #[test]
    fn the_low_profile_thinks_less_and_does_less() {
        let (full, low) = (Tuning::FULL, Tuning::LOW);
        assert!(low.every.iter().zip(full.every).all(|(l, f)| *l >= f));
        assert!(low.allowance.iter().zip(full.allowance).all(|(l, f)| *l <= f));
        let mut tuning = crate::Tuning::DEFAULT;
        tuning.schedule = low;
        assert!(tuning.validate().is_ok());
    }
}
