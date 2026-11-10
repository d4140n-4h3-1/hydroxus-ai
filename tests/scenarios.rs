//! Integration scenarios: the crate's modules working together, as a game would drive them.
//!
//! Each test builds one small level - a corridor with a room off it through a doorway - and runs
//! sentries through the handbook's per-frame loop (sense, perceive, decide, act, move) using only
//! the public API: `Sight` and the navmesh's `sees_across` for what they see, `Heard` for what
//! they hear, `Detection` and `Wariness` for how aware and how wary, `Alert` for the phases,
//! `conduct` to choose, `Investigation`, `SearchMap` and routes to act, `report::Witness` and
//! `join::joiners` for word passing between them. What the harness itself adds is only what a
//! game would: moving along routes, and logging what happened when.
//!
//! The last tests run the same scenario at different frame rates, steady and uneven, and step
//! fading state in one long step against many short ones: what happens, and when, should not hang
//! on how the time is cut up.

use hydroxus_ai::{
    alert::{Alert, Incident, Wariness},
    flat, forward, heading_of,
    navigation::{grid::WalkGrid, navmesh::NavMesh, walk::route_to},
    perception::{
        detection::{Change, Detection},
        hearing::Heard,
        sight::{Sight, Stance},
    },
    social::report::{Telling, Witness},
    tactics::{
        conduct::{Conduct, Reading, Reasoners, Role},
        investigate::{Doing, Investigation},
        look::LookAbout,
        join::{joiners, Watch},
        search::SearchMap,
    },
    think::{
        experience::{Experience, Outcome, Venture},
        habit::Habits,
        haunt::Haunts,
        lesson::Lessons,
    },
    Tuning,
};
use nalgebra::Vector3;

const AI: Tuning = Tuning::DEFAULT;
type V = Vector3<f32>;

const PATROL_SPEED: f32 = 1.2;
const WALK_SPEED: f32 = 1.5;
const SEARCH_SPEED: f32 = 2.5;
const RUN_SPEED: f32 = 4.0;
/// The player runs faster than a sentry, so that they can get away.
const PLAYER_SPEED: f32 = 5.5;

/// How often the searchers look over the search map, in seconds - on a clock of its own, so that
/// it looks as often at any frame rate.
const LOOK_EVERY: f32 = 0.5;
/// Somewhere far off the level: the player gone, out of anyone's sight.
const GONE: V = V::new(500.0, 0.0, 500.0);

/// A point on the level's floor, `x` and `z` in meters.
fn at(x: f32, z: f32) -> V {
    V::new(x, 0.0, z)
}

/// The level, 1 m cells: a corridor 40 m long and 4 wide along +x (z 0..4); a doorway passage at
/// x 29..31 leading north (z 4..8) into a room x 24..36, z 8..18.
fn level() -> NavMesh {
    let mut grid = WalkGrid::new(40, 18, 1.0);
    for z in 0..18 {
        for x in 0..40 {
            let corridor = z < 4;
            let doorway = (4..8).contains(&z) && (29..31).contains(&x);
            let room = (8..18).contains(&z) && (24..36).contains(&x);
            grid.set(x, z, corridor || doorway || room);
        }
    }
    NavMesh::build(&grid, V::zeros(), |_, _| 0)
}

/// What happened, to whom: the scenarios' record.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Event {
    Noticed,
    Spotted,
    /// Out of sight while after the player: Alert to Evasion.
    LostContact,
    /// Searching, it saw the player again.
    FoundAgain,
    InvestigationDone,
    /// Its search ran out: Evasion to Caution.
    GaveUp,
    /// Calm again: Caution to nothing.
    Calm,
    /// A witness told it what it saw.
    Told,
    /// Told, it went to search where it was told.
    Responded,
    /// It saw another sentry after the player, and joined in.
    Joined,
}

#[derive(Debug, Clone)]
struct Guard {
    feet: V,
    heading: f32,
    detection: Detection,
    wariness: Wariness,
    alert: Option<Alert>,
    /// Seconds left of its phase, Evasion or Caution.
    left: f32,
    investigation: Option<Investigation>,
    conduct: Option<Conduct>,
    /// Its route, the next point last (as the crate keeps them).
    route: Vec<V>,
    /// Its beat: two posts it walks between, going about its business.
    beat: [V; 2],
    post: usize,
    last_seen: V,
    /// Somewhere it was told of, to search from.
    told: Option<V>,
    /// Searching, looking about where it got to.
    looking: Option<LookAbout>,
    experience: Experience,
}

impl Guard {
    fn new(feet: V, heading: f32, beat: [V; 2]) -> Self {
        Self {
            feet,
            heading,
            detection: Detection::default(),
            wariness: Wariness::default(),
            alert: None,
            left: 0.0,
            investigation: None,
            conduct: None,
            route: Vec::new(),
            beat,
            post: 0,
            last_seen: feet,
            told: None,
            looking: None,
            experience: Experience::new(),
        }
    }

    /// One standing at `feet`, facing `heading`, its beat right where it stands.
    fn posted(feet: V, heading: f32) -> Self {
        Self::new(feet, heading, [feet, feet])
    }

    fn sees_player(&self, mesh: &NavMesh, player: V, stance: Stance) -> f32 {
        if !mesh.sees_across(self.feet, player) {
            return 0.0;
        }
        Sight::DEFAULT.visibility(self.alert, self.feet, self.heading, player, stance, false)
    }

    fn eagerness(&self) -> f32 {
        let searching = if self.alert == Some(Alert::Evasion) { AI.detection.searching } else { 1.0 };
        self.wariness.keenness(&AI.alert) * searching
    }

    /// Walks along its route `speed` meters a second for `dt`, facing the way it goes.
    fn walk(&mut self, speed: f32, dt: f32) {
        let mut left = speed * dt;
        while left > 0.0 {
            let Some(&next) = self.route.last() else { break };
            let to = flat(next - self.feet);
            let d = to.norm();
            if let Some(heading) = heading_of(to) {
                self.heading = heading;
            }
            if d <= left {
                self.feet = next;
                self.route.pop();
                left -= d;
            } else {
                self.feet += to / d * left;
                left = 0.0;
            }
        }
    }

    fn go_to(&mut self, mesh: &NavMesh, to: V) {
        self.route = route_to(mesh, self.feet, to, f32::INFINITY);
    }
}

/// The player's way: points it walks through in turn, `PLAYER_SPEED` meters a second.
#[derive(Debug, Clone, Default)]
struct Path {
    points: Vec<V>,
}

struct World {
    mesh: NavMesh,
    guards: Vec<Guard>,
    player: V,
    stance: Stance,
    path: Path,
    search: SearchMap,
    reasoners: Reasoners,
    /// A citizen that saw something, where it stands, on its way to tell a sentry.
    witness: Option<(V, Witness)>,
    look_in: f32,
    t: f32,
    log: Vec<(f32, usize, Event)>,
}

impl World {
    fn new(guards: Vec<Guard>, player: V) -> Self {
        Self {
            mesh: level(),
            guards,
            player,
            stance: Stance::Standing,
            path: Path::default(),
            search: SearchMap::with(AI.search),
            reasoners: Reasoners::default(),
            witness: None,
            look_in: 0.0,
            t: 0.0,
            log: Vec::new(),
        }
    }

    fn note(&mut self, guard: usize, event: Event) {
        self.log.push((self.t, guard, event));
    }

    /// When `guard` first came to `event`, if it did.
    fn first(&self, guard: usize, event: Event) -> Option<f32> {
        self.log.iter().find(|&&(_, g, e)| g == guard && e == event).map(|&(t, _, _)| t)
    }

    /// A noise at `at`, carrying `loudness` meters along the floor: each guard that hears it the
    /// more aware for it, and one that notices something for it goes to look.
    fn noise(&mut self, at: V, loudness: f32) {
        let Some(heard) = Heard::at(&self.mesh, at, loudness) else { return };
        for g in 0..self.guards.len() {
            let guard = &mut self.guards[g];
            let Some(nearness) = heard.nearness(&self.mesh, guard.feet) else { continue };
            let eagerness = guard.eagerness();
            guard.wariness.note(&AI.alert, Incident::Heard);
            if guard.detection.hear(&AI.detection, nearness, eagerness) == Some(Change::Noticed) {
                guard.investigation.get_or_insert(Investigation::new(&AI.investigate, at));
                self.note(g, Event::Noticed);
            }
        }
    }

    /// Runs the world on for `seconds` in steps of `dt`.
    fn run(&mut self, seconds: f32, dt: f32) {
        self.run_by(seconds, |_| dt);
    }

    /// Runs the world on for `seconds`, the `n`th step `dt(n)` long - the last cut short to
    /// end on time.
    fn run_by(&mut self, seconds: f32, dt: impl Fn(usize) -> f32) {
        let end = self.t + seconds;
        let mut n = 0;
        while self.t < end - 1.0e-6 {
            let step = dt(n).min(end - self.t);
            self.step(step);
            n += 1;
        }
    }

    fn step(&mut self, dt: f32) {
        self.t += dt;
        self.move_player(dt);
        self.step_witness(dt);
        for g in 0..self.guards.len() {
            self.step_guard(g, dt);
        }
        self.join_in();
        // The search: where the player could be spreading as fast as they could go, cleared where
        // the searchers look and do not see them.
        if self.guards.iter().any(|g| g.alert == Some(Alert::Evasion)) && !self.search.is_empty() {
            self.search.spread(&self.mesh, PLAYER_SPEED * dt);
            self.look_in -= dt;
            while self.look_in <= 0.0 {
                self.look_in += LOOK_EVERY;
                let sight = Sight::DEFAULT;
                for guard in self.guards.iter().filter(|g| g.alert == Some(Alert::Evasion)) {
                    let reach = sight.reach(Some(Alert::Evasion), Stance::Standing, false);
                    self.search.look(&self.mesh, guard.feet, guard.heading, (reach, sight.cone, 2.0));
                }
            }
        }
    }

    fn move_player(&mut self, dt: f32) {
        let mut left = PLAYER_SPEED * dt;
        while left > 0.0 && !self.path.points.is_empty() {
            let next = self.path.points[0];
            let to = flat(next - self.player);
            let d = to.norm();
            if d <= left {
                self.player = next;
                self.path.points.remove(0);
                left -= d;
            } else {
                self.player += to / d * left;
                left = 0.0;
            }
        }
    }

    fn step_witness(&mut self, dt: f32) {
        let Some((feet, mut witness)) = self.witness.take() else { return };
        let sentries: Vec<(usize, V)> = self.guards.iter().enumerate().map(|(g, guard)| (g, guard.feet)).collect();
        match witness.step(dt, feet, &sentries) {
            Telling::Told(g) => {
                self.guards[g].told = Some(witness.at);
                self.note(g, Event::Told);
            }
            Telling::MakingFor { sentry, .. } => {
                let to = flat(self.guards[sentry].feet - feet);
                let step = (WALK_SPEED * dt).min(to.norm());
                let feet = feet + to.try_normalize(1.0e-6).unwrap_or_default() * step;
                self.witness = Some((feet, witness));
            }
            Telling::Hiding => self.witness = Some((feet, witness)),
            Telling::GaveUp => (),
        }
    }

    fn step_guard(&mut self, g: usize, dt: f32) {
        let (player, stance) = (self.player, self.stance);
        // Sense.
        let visible = self.guards[g].sees_player(&self.mesh, player, stance);
        let mut events = Vec::new();
        let guard = &mut self.guards[g];
        guard.wariness.step(&AI.alert, dt);
        // Perceive: on Alert it sees them at once; otherwise it becomes aware of them gradually.
        let eagerness = guard.eagerness();
        let change = guard.detection.step(&AI.detection, visible, eagerness, dt);
        let sees = match guard.alert {
            Some(Alert::Alert) => visible > 0.0,
            _ => guard.detection.spotted() && visible > 0.0,
        };
        match change {
            Some(Change::Noticed) if guard.alert.is_none() => {
                events.push(Event::Noticed);
                guard.wariness.note(&AI.alert, Incident::Noticed);
                guard.investigation.get_or_insert(Investigation::new(&AI.investigate, player));
            }
            Some(Change::Spotted) if guard.alert.is_none() => {
                events.push(Event::Spotted);
                guard.wariness.note(&AI.alert, Incident::Spotted);
                guard.investigation = None;
                guard.alert = Some(Alert::Alert);
                guard.route.clear();
            }
            _ if visible > 0.0 => {
                if let Some(look) = guard.investigation.as_mut() {
                    look.saw_again(&AI.investigate, player);
                }
            }
            _ => (),
        }
        if sees {
            guard.last_seen = player;
        }
        // Told where to look: it searches there.
        if let Some(there) = guard.told.take().filter(|_| guard.alert != Some(Alert::Alert)) {
            guard.wariness.note(&AI.alert, Incident::Alarm);
            guard.alert = Some(Alert::Evasion);
            guard.left = guard.wariness.lasts(&AI.alert, Alert::Evasion).unwrap_or_default();
            guard.last_seen = there;
            guard.route.clear();
            self.search.lose(&self.mesh, there, None);
            events.push(Event::Responded);
        }
        // The phases.
        if let Some(phase) = guard.alert {
            if phase != Alert::Alert {
                guard.left -= dt;
            }
            let next = phase.next(sees, guard.left);
            match (phase, next) {
                (Alert::Alert, Some(Alert::Evasion)) => {
                    events.push(Event::LostContact);
                    guard.wariness.note(&AI.alert, Incident::Lost);
                    guard.left = guard.wariness.lasts(&AI.alert, Alert::Evasion).unwrap_or_default();
                    guard.route.clear();
                    self.search.lose(&self.mesh, guard.last_seen, None);
                    guard.experience.report(&AI, Some(&self.mesh), Outcome::failed(Venture::Pursuit).at(guard.last_seen));
                }
                (Alert::Evasion | Alert::Caution, Some(Alert::Alert)) => {
                    events.push(Event::FoundAgain);
                    guard.route.clear();
                    if phase == Alert::Evasion {
                        guard.experience.report(&AI, Some(&self.mesh), Outcome::succeeded(Venture::Search).at(player));
                    }
                }
                (Alert::Evasion, Some(Alert::Caution)) => {
                    events.push(Event::GaveUp);
                    guard.left = guard.wariness.lasts(&AI.alert, Alert::Caution).unwrap_or_default();
                    guard.route.clear();
                    guard.experience.report(&AI, None, Outcome::<()>::came_to_nothing(Venture::Search));
                }
                (Alert::Caution, None) => {
                    events.push(Event::Calm);
                    guard.detection.reset();
                    guard.route.clear();
                }
                _ => (),
            }
            guard.alert = next;
        }
        guard.experience.step(&AI, dt);
        // Decide.
        let reading = Reading {
            hunting: guard.alert == Some(Alert::Alert),
            investigating: guard.investigation.is_some() && guard.alert != Some(Alert::Alert),
            searching: guard.alert == Some(Alert::Evasion),
            nerve: 0.5,
            wariness: guard.wariness.level,
            ..Reading::default()
        };
        let conduct = guard.experience.choose(&AI, &mut self.reasoners, Role::Sentry, &reading, guard.conduct);
        if guard.conduct != Some(conduct) {
            guard.route.clear();
        }
        guard.conduct = Some(conduct);
        // Act and move.
        let mesh = &self.mesh;
        match conduct {
            Conduct::Chase => {
                let target = guard.last_seen;
                if guard.route.first().is_none_or(|&end| flat(end - target).norm() > 1.0) {
                    guard.go_to(mesh, target);
                }
                guard.walk(RUN_SPEED, dt);
            }
            Conduct::Investigate => {
                let mut look = guard.investigation.take().expect("investigating");
                match look.step(&AI.investigate, guard.feet, guard.heading, dt) {
                    Doing::Face { heading } | Doing::LookAround { heading } => {
                        guard.route.clear();
                        guard.heading = heading;
                        guard.investigation = Some(look);
                    }
                    Doing::GoTo { at, afresh } => {
                        if afresh || guard.route.is_empty() {
                            guard.go_to(mesh, at);
                        }
                        guard.walk(WALK_SPEED, dt);
                        guard.investigation = Some(look);
                    }
                    Doing::Done => {
                        guard.route.clear();
                        events.push(Event::InvestigationDone);
                        guard.experience.report(&AI, None, Outcome::<()>::came_to_nothing(Venture::Investigation));
                    }
                }
            }
            Conduct::Search if guard.looking.is_some_and(|look| !look.done()) => {
                // Got where it was going: it looks about there, one way and then the other.
                let look = guard.looking.as_mut().expect("looking");
                if let Some(heading) = look.step(dt) {
                    guard.heading = heading;
                }
            }
            Conduct::Search => {
                guard.looking = None;
                if guard.route.is_empty() {
                    let feet = guard.feet;
                    let taken: Vec<V> = self
                        .guards
                        .iter()
                        .enumerate()
                        .filter(|&(o, other)| o != g && other.alert == Some(Alert::Evasion))
                        .filter_map(|(_, other)| other.route.first().copied())
                        .collect();
                    let best = self.search.best(mesh, feet, &taken, 6.0, 60.0);
                    let guard = &mut self.guards[g];
                    if let Some(best) = best {
                        guard.go_to(mesh, best);
                    }
                }
                let guard = &mut self.guards[g];
                let going = !guard.route.is_empty();
                guard.walk(SEARCH_SPEED, dt);
                if going && guard.route.is_empty() {
                    guard.looking = Some(LookAbout::searching(&AI.search, guard.heading));
                }
            }
            _ => {
                // Going about its business: walking its beat.
                if guard.route.is_empty() && flat(guard.beat[0] - guard.beat[1]).norm() > 0.5 {
                    guard.post = 1 - guard.post;
                    guard.go_to(mesh, guard.beat[guard.post]);
                }
                guard.walk(PATROL_SPEED, dt);
            }
        }
        for event in events {
            self.note(g, event);
        }
    }

    /// Sentries that see another after the player join in, as the game has them.
    fn join_in(&mut self) {
        let watches: Vec<Watch> = self
            .guards
            .iter()
            .map(|g| Watch {
                sentry: true,
                alert: g.alert,
                sees_player: g.alert == Some(Alert::Alert) && g.sees_player(&self.mesh, self.player, self.stance) > 0.0,
                busy: false,
                windup: 0.0,
                feet: g.feet,
                heading: g.heading,
            })
            .collect();
        let mesh = &self.mesh;
        let feet: Vec<V> = self.guards.iter().map(|g| g.feet).collect();
        for g in joiners(&watches, false, |a, b| mesh.sees_across(feet[a], feet[b])) {
            let guard = &mut self.guards[g];
            guard.alert = Some(Alert::Alert);
            guard.detection.alert();
            guard.investigation = None;
            guard.last_seen = self.player;
            guard.route.clear();
            self.log.push((self.t, g, Event::Joined));
        }
    }
}

const EAST: f32 = std::f32::consts::FRAC_PI_2;
const WEST: f32 = -std::f32::consts::FRAC_PI_2;

#[test]
fn a_sentry_hears_an_unfamiliar_sound_goes_to_look_and_goes_back_to_its_beat() {
    let beat = [at(4.0, 2.0), at(12.0, 2.0)];
    // The player hides in the room's corner, out of sight of the corridor.
    let mut world = World::new(vec![Guard::new(at(8.0, 2.0), EAST, beat)], at(24.5, 8.5));
    world.run(2.0, 1.0 / 60.0);
    assert_eq!(world.guards[0].conduct, Some(Conduct::GoAbout));
    // Something clatters down the corridor, 8 m off along the floor.
    let noise = at(18.0, 2.0);
    world.noise(noise, 20.0);
    assert!(world.first(0, Event::Noticed).is_some(), "it noticed the noise: {:?}", world.guards[0].detection);
    assert!(world.guards[0].alert.is_none(), "a noise is no one spotted");
    // It goes to look, getting there, and looking about there...
    let mut closest = f32::MAX;
    for _ in 0..(30 * 60) {
        world.run(1.0 / 60.0, 1.0 / 60.0);
        closest = closest.min(flat(world.guards[0].feet - noise).norm());
        if world.first(0, Event::InvestigationDone).is_some() {
            break;
        }
    }
    assert!(closest < AI.investigate.reach + 0.5, "it went to where it heard it: {closest} m at the closest");
    let done = world.first(0, Event::InvestigationDone).expect("it finished looking");
    assert!(done < 2.0 + AI.investigate.give_up + 0.1, "within its give-up time: {done}");
    // ...and, finding nothing, back to its beat, never having seen the player.
    world.run(10.0, 1.0 / 60.0);
    let guard = &world.guards[0];
    assert_eq!(guard.conduct, Some(Conduct::GoAbout));
    assert!(guard.alert.is_none() && world.first(0, Event::Spotted).is_none());
    assert!(guard.feet.x <= beat[1].x + 0.5 && guard.feet.x >= beat[0].x - 0.5, "on its beat: {:?}", guard.feet);
    // A look that came to nothing: it makes a little less of looking.
    assert!(guard.experience.lessons.worth(Conduct::Investigate) < 0.0);
}

#[test]
fn a_sentry_spots_an_intruder_loses_them_and_searches_where_they_were_last_seen() {
    // The player stands in the corridor, 12 m in front of the sentry.
    let mut world = World::new(vec![Guard::posted(at(8.0, 2.0), EAST)], at(20.0, 2.0));
    world.run(4.0, 1.0 / 60.0);
    let noticed = world.first(0, Event::Noticed).expect("noticed");
    let spotted = world.first(0, Event::Spotted).expect("spotted");
    assert!(noticed < spotted && spotted < 3.0, "noticed at {noticed}, spotted at {spotted}");
    assert_eq!(world.guards[0].conduct, Some(Conduct::Chase));
    // They run for it: up the corridor, through the doorway, into the room's far corner.
    world.path.points = vec![at(30.0, 2.0), at(30.0, 9.0), at(35.5, 17.5)];
    let mut lost_at = None;
    for _ in 0..(20 * 60) {
        world.run(1.0 / 60.0, 1.0 / 60.0);
        if lost_at.is_none() && world.first(0, Event::LostContact).is_some() {
            lost_at = Some(world.guards[0].last_seen);
            // Gone, while the sentry is not looking: off the level altogether.
            world.player = GONE;
            world.path.points.clear();
        }
    }
    let lost_at = lost_at.expect("it lost sight of them");
    let lost = world.first(0, Event::LostContact).unwrap();
    assert_eq!(world.guards[0].alert, Some(Alert::Evasion));
    assert_eq!(world.guards[0].conduct, Some(Conduct::Search));
    // The search started where they were last seen, and the sentry went there.
    let mut map = SearchMap::with(AI.search);
    map.lose(&world.mesh, lost_at, None);
    assert!(map.chance_at(&world.mesh, lost_at) > 0.0);
    let mut world_again = World::new(vec![Guard::posted(at(8.0, 2.0), EAST)], at(20.0, 2.0));
    world_again.run(4.0, 1.0 / 60.0);
    world_again.path.points = vec![at(30.0, 2.0), at(30.0, 9.0), at(35.5, 17.5)];
    let mut closest = f32::MAX;
    let mut gone = false;
    for _ in 0..(40 * 60) {
        world_again.run(1.0 / 60.0, 1.0 / 60.0);
        if !gone && world_again.first(0, Event::LostContact).is_some() {
            gone = true;
            world_again.player = GONE;
            world_again.path.points.clear();
        }
        if gone {
            closest = closest.min(flat(world_again.guards[0].feet - lost_at).norm());
        }
    }
    assert!(closest < 4.0, "it searched where they were last seen: {closest} m at the closest, lost at {lost:.2} s");
    // Chasing them came to nothing: it makes less of chasing for it.
    assert!(world_again.guards[0].experience.lessons.worth(Conduct::Chase) < 0.0);
}

#[test]
fn a_searching_sentry_finds_one_hiding_near_where_they_were_lost() {
    let mut world = World::new(vec![Guard::posted(at(8.0, 2.0), EAST)], at(20.0, 2.0));
    world.run(4.0, 1.0 / 60.0);
    // Into the room and round the corner, out of sight of the doorway - and they stay there.
    world.path.points = vec![at(30.0, 2.0), at(30.0, 9.0), at(24.5, 9.5)];
    world.run(30.0, 1.0 / 60.0);
    let lost = world.first(0, Event::LostContact).expect("lost them");
    let found = world.first(0, Event::FoundAgain).expect("found them again, searching");
    assert!(found > lost && found - lost < AI.alert.evasion, "lost at {lost}, found at {found}");
    assert!(world.guards[0].experience.lessons.worth(Conduct::Search) > 0.0, "a search that worked");
}

#[test]
fn a_witness_tells_a_sentry_and_the_sentry_searches_where_it_happened() {
    let mut world = World::new(vec![Guard::posted(at(4.0, 2.0), EAST), Guard::posted(at(38.0, 2.0), EAST)], GONE);
    // A citizen saw something done in the room, and makes for the nearest sentry to tell it.
    let happened = at(30.0, 12.0);
    world.witness = Some((at(14.0, 2.0), Witness::new(happened, true)));
    world.run(20.0, 1.0 / 60.0);
    let told = world.first(0, Event::Told).expect("the witness told the nearest sentry");
    assert!(world.first(1, Event::Told).is_none(), "and only it");
    assert!(world.witness.is_none());
    // Told, the sentry goes to search there - firsthand, the other sentry knows nothing of it.
    assert!(world.first(0, Event::Responded).is_some_and(|t| t >= told));
    assert_eq!(world.guards[0].alert, Some(Alert::Evasion));
    assert!(world.guards[1].alert.is_none());
    let mut closest = f32::MAX;
    for _ in 0..(20 * 60) {
        world.run(1.0 / 60.0, 1.0 / 60.0);
        closest = closest.min(flat(world.guards[0].feet - happened).norm());
    }
    assert!(closest < 5.0, "it went where it was told: {closest} m at the closest");
}

#[test]
fn a_sentry_that_sees_another_after_the_player_joins_in_and_one_looking_away_does_not() {
    // Guard 0 faces the player. Guard 1, behind it, faces it and so sees it give chase, but not
    // the player, who is beyond its own reach. Guard 2, as far off, faces away.
    let guards = vec![
        Guard::posted(at(20.0, 2.0), EAST),
        Guard::posted(at(5.0, 2.0), EAST),
        Guard::posted(at(5.0, 1.0), WEST),
    ];
    let mut world = World::new(guards, at(36.0, 2.0));
    world.run(6.0, 1.0 / 60.0);
    let spotted = world.first(0, Event::Spotted).expect("guard 0 spotted them");
    let joined = world.first(1, Event::Joined).expect("guard 1 saw it give chase, and joined");
    assert!(joined >= spotted);
    assert!(world.first(1, Event::Spotted).is_none(), "it did not see the player itself");
    // Not seeing the player for itself, it is soon searching where it was told they were - but
    // it is after them, and on its way.
    assert!(matches!(world.guards[1].alert, Some(Alert::Alert | Alert::Evasion)), "{:?}", world.guards[1].alert);
    assert!(world.guards[1].feet.x > 8.0, "set off after them: {:?}", world.guards[1].feet);
    assert!(world.first(2, Event::Joined).is_none() && world.guards[2].alert.is_none(), "guard 2 saw nothing");
}

/// Spots the player, loses them for good, searches, gives up, and calms down: the whole incident.
fn incident(dt: impl Fn(usize) -> f32 + Copy) -> World {
    let mut world = World::new(vec![Guard::new(at(8.0, 2.0), EAST, [at(4.0, 2.0), at(12.0, 2.0)])], at(20.0, 2.0));
    world.run_by(4.0, dt);
    world.player = GONE;
    // Long enough to search, give up and calm down, however wary it is.
    let lasts = (AI.alert.evasion + AI.alert.caution) * (1.0 + AI.alert.longer);
    world.run_by(lasts + 10.0, dt);
    world
}

#[test]
fn after_an_incident_a_sentry_goes_back_to_its_routine_and_its_wariness_fades() {
    let mut world = incident(|_| 1.0 / 60.0);
    let order: Vec<Event> = world.log.iter().filter(|&&(_, g, _)| g == 0).map(|&(_, _, e)| e).collect();
    assert_eq!(order, vec![Event::Noticed, Event::Spotted, Event::LostContact, Event::GaveUp, Event::Calm]);
    let (lost, gave_up, calm) = (
        world.first(0, Event::LostContact).unwrap(),
        world.first(0, Event::GaveUp).unwrap(),
        world.first(0, Event::Calm).unwrap(),
    );
    // What it went through draws its search and its watching out, beyond the plain phases.
    assert!(gave_up - lost > AI.alert.evasion, "searched {} s", gave_up - lost);
    assert!(calm - gave_up > AI.alert.caution, "watchful {} s", calm - gave_up);
    let guard = &world.guards[0];
    assert!(guard.alert.is_none());
    assert_eq!(guard.conduct, Some(Conduct::GoAbout));
    // Back on its beat.
    world.run(20.0, 1.0 / 60.0);
    let guard = &world.guards[0];
    assert!(guard.feet.x >= 3.5 && guard.feet.x <= 12.5 && guard.feet.z > 1.0 && guard.feet.z < 3.0, "on its beat: {:?}", guard.feet);
    // In half an hour its wariness is all but gone.
    let was = guard.wariness.level;
    assert!(was > 0.1, "{was}");
    world.run(1800.0, 0.5);
    assert!(world.guards[0].wariness.level < was * 0.05);
}

/// How long the `n`th frame is, in seconds.
type Frames = fn(usize) -> f32;

/// When each thing in the incident happened, in order.
fn timeline(world: &World) -> Vec<(Event, f32)> {
    world.log.iter().map(|&(t, _, e)| (e, t)).collect()
}

#[test]
fn an_incident_plays_out_the_same_at_any_frame_rate() {
    let reference = timeline(&incident(|_| 1.0 / 60.0));
    let rates: [(&str, Frames); 4] = [
        ("30 Hz", |_| 1.0 / 30.0),
        ("144 Hz", |_| 1.0 / 144.0),
        ("uneven", |n| [1.0 / 25.0, 1.0 / 90.0, 1.0 / 60.0, 1.0 / 200.0, 1.0 / 40.0][n % 5]),
        ("hitching", |n| if n % 50 == 0 { 0.1 } else { 1.0 / 120.0 }),
    ];
    for (name, dt) in rates {
        let other = timeline(&incident(dt));
        let events = |line: &[(Event, f32)]| line.iter().map(|&(e, _)| e).collect::<Vec<_>>();
        assert_eq!(events(&other), events(&reference), "{name}: the same events, in the same order");
        for (&(event, then), &(_, now)) in reference.iter().zip(&other) {
            // Within a couple of the coarsest frames, and a hair of the time for a long phase.
            let slack = 0.25 + then * 0.002;
            assert!((now - then).abs() <= slack, "{name}: {event:?} at {now:.3} s against {then:.3} s at 60 Hz");
        }
    }
}

#[test]
fn fading_state_fades_the_same_in_one_long_step_as_in_many_short_ones() {
    const SECONDS: f32 = 600.0;
    const STEPS: usize = 6000;
    let short = SECONDS / STEPS as f32;

    // Wariness.
    let mut wariness = Wariness::default();
    wariness.note(&AI.alert, Incident::FoundBody);
    let (mut once, mut often) = (wariness, wariness);
    once.step(&AI.alert, SECONDS);
    for _ in 0..STEPS {
        often.step(&AI.alert, short);
    }
    assert!((once.level - often.level).abs() < 1.0e-4, "wariness {} {}", once.level, often.level);

    // Awareness draining, out of sight - including the second before it starts to.
    for seconds in [0.5, 1.0, 2.0, 3.0, 10.0] {
        let mut detection = Detection::default();
        detection.alert();
        let (mut once, mut often) = (detection, detection);
        once.step(&AI.detection, 0.0, 1.0, seconds);
        let steps = (seconds * 600.0) as usize;
        for _ in 0..steps {
            often.step(&AI.detection, 0.0, 1.0, seconds / steps as f32);
        }
        assert!((once.level - often.level).abs() < 1.0e-3, "after {seconds} s unseen: {} in one step, {} in many", once.level, often.level);
    }

    // Lessons, habits, haunts and an experience.
    let mesh = level();
    let mut lessons = Lessons::new();
    lessons.learn(&AI.lesson, Conduct::Search, 1.0);
    let mut habits = Habits::new();
    habits.saw(1u8);
    let mut haunts = Haunts::new();
    haunts.found_at(&AI.haunt, &mesh, at(30.0, 12.0));
    let mut experience = Experience::<u8>::new();
    experience.trail.doing(Conduct::Search);
    experience.report(&AI, Some(&mesh), Outcome::succeeded(Venture::Search).at(at(30.0, 12.0)).player_did(1));
    let (mut l1, mut h1, mut p1, mut e1) = (lessons.clone(), habits.clone(), haunts.clone(), experience.clone());
    let (mut l2, mut h2, mut p2, mut e2) = (lessons, habits, haunts, experience);
    l1.step(&AI.lesson, SECONDS);
    h1.step(&AI.habit, SECONDS);
    p1.step(&AI.haunt, SECONDS);
    e1.step(&AI, SECONDS);
    for _ in 0..STEPS {
        l2.step(&AI.lesson, short);
        h2.step(&AI.habit, short);
        p2.step(&AI.haunt, short);
        e2.step(&AI, short);
    }
    let close = |a: f32, b: f32| (a - b).abs() < 1.0e-4 * a.abs().max(1.0);
    assert!(close(l1.worth(Conduct::Search), l2.worth(Conduct::Search)));
    assert!(close(h1.times(1), h2.times(1)));
    let weights = |h: &Haunts| h.haunts(&mesh).map(|(_, w)| w).collect::<Vec<_>>();
    assert_eq!(weights(&p1).len(), weights(&p2).len());
    assert!(weights(&p1).iter().zip(weights(&p2)).all(|(&a, b)| close(a, b)));
    assert!(close(e1.lessons.worth(Conduct::Search), e2.lessons.worth(Conduct::Search)));
    assert!(close(e1.habits.times(1), e2.habits.times(1)));
    assert!(e1.trail.share(&AI.lesson, Conduct::Search) == e2.trail.share(&AI.lesson, Conduct::Search));
}

#[test]
fn the_scenarios_come_out_the_same_every_time() {
    let a = timeline(&incident(|n| [1.0 / 25.0, 1.0 / 90.0, 1.0 / 60.0][n % 3]));
    let b = timeline(&incident(|n| [1.0 / 25.0, 1.0 / 90.0, 1.0 / 60.0][n % 3]));
    assert_eq!(a, b);
}

#[test]
fn sight_needs_the_player_in_front_whatever_the_frame_rate() {
    // Behind the sentry, close, for a long while: it never becomes aware of them.
    for dt in [1.0 / 30.0, 1.0 / 144.0] {
        let mut world = World::new(vec![Guard::posted(at(20.0, 2.0), EAST)], at(17.0, 2.0));
        world.run(20.0, dt);
        assert!(world.log.is_empty(), "{:?}", world.log);
        assert_eq!(forward(world.guards[0].heading).x.signum(), 1.0, "still facing away");
    }
}

