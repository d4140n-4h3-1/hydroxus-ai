//! The orchestration layer driven as a game would drive it: the game senses (where each sentry
//! stands, how visible the player is - its rays are `sees_across` here), hands that to
//! `Squad::step`, and carries out each `Intent` - routing, walking at the pace given, turning.
//! The same level as `tests/scenarios.rs`: a corridor 40 m by 4 along +x, a doorway passage at
//! x 29..31 north into a room x 24..36, z 8..18.

use hydroxus_ai::{
    alert::Alert,
    flat, heading_of,
    navigation::{grid::WalkGrid, navmesh::NavMesh, walk::route_to},
    orchestrate::{Happened, Intent, Pace, Senses, Sentry, Squad},
    tactics::priority::{self, Traits},
    think::memory::Kind,
    perception::sight::{Sight, Stance},
    tactics::conduct::{Conduct, Reasoners, Role},
    Tuning,
};
use nalgebra::Vector3;

const AI: Tuning = Tuning::DEFAULT;
type V = Vector3<f32>;
const EAST: f32 = std::f32::consts::FRAC_PI_2;
const WEST: f32 = -std::f32::consts::FRAC_PI_2;
const PLAYER_SPEED: f32 = 5.5;
const GONE: V = V::new(500.0, 0.0, 500.0);

fn at(x: f32, z: f32) -> V {
    V::new(x, 0.0, z)
}

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

/// What the game keeps of a sentry: its body.
#[derive(Debug, Clone)]
struct Body {
    feet: V,
    heading: f32,
    route: Vec<V>,
    /// Where its route goes.
    to: Option<V>,
}

struct World {
    mesh: NavMesh,
    squad: Squad,
    sentries: Vec<Sentry>,
    bodies: Vec<Body>,
    reasoners: Reasoners,
    player: V,
    path: Vec<V>,
    t: f32,
    log: Vec<(f32, usize, Happened)>,
    intents: Vec<Intent>,
    /// The tuning it runs by: `AI`, unless a test changes it.
    ai: Tuning,
    /// How dangerous things are for every sentry: shot at, say.
    threat: f32,
}

impl World {
    fn new(posted: &[(V, f32)], player: V) -> Self {
        Self::with(posted.iter().map(|&(feet, heading)| (Sentry::new(Role::Sentry), feet, heading)).collect(), player)
    }

    fn with(sentries: Vec<(Sentry, V, f32)>, player: V) -> Self {
        Self::on(level(), sentries, player)
    }

    /// On a level of its own.
    fn on(mesh: NavMesh, sentries: Vec<(Sentry, V, f32)>, player: V) -> Self {
        let bodies = sentries.iter().map(|&(_, feet, heading)| Body { feet, heading, route: Vec::new(), to: None }).collect();
        Self {
            mesh,
            squad: Squad::new(&AI),
            sentries: sentries.into_iter().map(|(s, _, _)| s).collect(),
            bodies,
            reasoners: Reasoners::default(),
            player,
            path: Vec::new(),
            t: 0.0,
            log: Vec::new(),
            intents: Vec::new(),
            ai: AI,
            threat: 0.0,
        }
    }

    fn first(&self, sentry: usize, what: Happened) -> Option<f32> {
        self.log.iter().find(|&&(_, s, h)| s == sentry && h == what).map(|&(t, _, _)| t)
    }

    fn run(&mut self, seconds: f32, dt: f32) {
        self.run_by(seconds, |_| dt);
    }

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
        // The player.
        let mut left = PLAYER_SPEED * dt;
        while left > 0.0 && !self.path.is_empty() {
            let to = flat(self.path[0] - self.player);
            if to.norm() <= left {
                left -= to.norm();
                self.player = self.path.remove(0);
            } else {
                self.player += to.normalize() * left;
                left = 0.0;
            }
        }
        // Sense: the game's part.
        let senses: Vec<Senses> = self
            .bodies
            .iter()
            .zip(&self.sentries)
            .map(|(body, sentry)| {
                let visible = if self.mesh.sees_across(body.feet, self.player) {
                    Sight::DEFAULT.visibility(sentry.alert(), body.feet, body.heading, self.player, Stance::Standing, false)
                } else {
                    0.0
                };
                Senses { feet: body.feet, heading: body.heading, visible, player: self.player, threat: self.threat }
            })
            .collect();
        let feet: Vec<V> = self.bodies.iter().map(|b| b.feet).collect();
        let mesh = &self.mesh;
        let thoughts = self.squad.step(&self.ai, mesh, &mut self.reasoners, &mut self.sentries, &senses, false, |a, b| mesh.sees_across(feet[a], feet[b]), dt);
        self.intents.clear();
        // Act: the game carries out each intent.
        for (i, thought) in thoughts.into_iter().enumerate() {
            for happened in thought.happened {
                self.log.push((self.t, i, happened));
            }
            self.intents.push(thought.intent);
            let body = &mut self.bodies[i];
            match thought.intent {
                Intent::Routine => body.route.clear(),
                Intent::Face { heading } => {
                    body.route.clear();
                    body.to = None;
                    body.heading = heading;
                }
                Intent::GoTo { at, pace } => {
                    if body.to.is_none_or(|to| flat(to - at).norm() > 0.5) || body.route.is_empty() {
                        body.route = route_to(&self.mesh, body.feet, at, f32::INFINITY);
                        body.to = Some(at);
                        if body.route.is_empty() && flat(at - body.feet).norm() > self.ai.orchestrate.arrived {
                            self.sentries[i].no_way(&self.ai);
                        }
                    }
                    let speed = match pace {
                        Pace::Walk => 1.5,
                        Pace::Search => 2.5,
                        Pace::Run => 4.0,
                    };
                    let mut left = speed * dt;
                    while left > 0.0 {
                        let Some(&next) = body.route.last() else { break };
                        let to = flat(next - body.feet);
                        if let Some(heading) = heading_of(to) {
                            body.heading = heading;
                        }
                        if to.norm() <= left {
                            left -= to.norm();
                            body.feet = next;
                            body.route.pop();
                        } else {
                            body.feet += to.normalize() * left;
                            left = 0.0;
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn hears_a_sound_investigates_and_goes_back_to_its_routine() {
    let mut world = World::new(&[(at(8.0, 2.0), EAST)], at(24.5, 8.5));
    world.run(1.0, 1.0 / 60.0);
    assert_eq!(world.intents[0], Intent::Routine, "calm, the game's own routine");
    let noise = at(18.0, 2.0);
    let mesh = level();
    world.squad.noise(&AI, &mesh, &mut world.sentries, noise, 20.0);
    let mut closest = f32::MAX;
    for _ in 0..(30 * 60) {
        world.run(1.0 / 60.0, 1.0 / 60.0);
        closest = closest.min(flat(world.bodies[0].feet - noise).norm());
    }
    assert!(world.first(0, Happened::Noticed).is_some());
    assert!(closest < AI.investigate.reach + 0.5, "went to look: {closest}");
    assert!(world.first(0, Happened::InvestigationDone).is_some());
    assert_eq!(world.intents[0], Intent::Routine, "and back to its routine");
    assert!(world.sentries[0].alert().is_none() && world.first(0, Happened::Spotted).is_none());
    assert!(world.sentries[0].experience.lessons.worth(Conduct::Investigate) < 0.0, "a look that found nothing");
}

#[test]
fn spots_chases_loses_and_searches_where_they_were_last_seen() {
    let mut world = World::new(&[(at(8.0, 2.0), EAST)], at(20.0, 2.0));
    world.run(4.0, 1.0 / 60.0);
    assert!(world.first(0, Happened::Noticed) < world.first(0, Happened::Spotted));
    assert!(matches!(world.intents[0], Intent::GoTo { pace: Pace::Run, .. }), "chasing: {:?}", world.intents[0]);
    world.path = vec![at(30.0, 2.0), at(30.0, 9.0), at(35.5, 17.5)];
    let mut lost = None;
    let mut closest = f32::MAX;
    for _ in 0..(40 * 60) {
        world.run(1.0 / 60.0, 1.0 / 60.0);
        if lost.is_none() && world.first(0, Happened::LostContact).is_some() {
            lost = world.sentries[0].last_seen();
            world.player = GONE;
            world.path.clear();
        }
        if let Some(lost) = lost {
            closest = closest.min(flat(world.bodies[0].feet - lost).norm());
        }
    }
    assert!(lost.is_some(), "it lost them");
    assert!(closest < 4.0, "searched where they were last seen: {closest}");
    assert!(world.sentries[0].experience.lessons.worth(Conduct::Chase) < 0.0);
}

#[test]
fn a_search_finds_one_hiding_near_where_they_were_lost() {
    let mut world = World::new(&[(at(8.0, 2.0), EAST)], at(20.0, 2.0));
    world.run(4.0, 1.0 / 60.0);
    world.path = vec![at(30.0, 2.0), at(30.0, 9.0), at(24.5, 9.5)];
    world.run(30.0, 1.0 / 60.0);
    let lost = world.first(0, Happened::LostContact).expect("lost them");
    let found = world.first(0, Happened::FoundAgain).expect("found them again");
    assert!(found > lost && found - lost < AI.alert.evasion, "{lost} {found}");
}

#[test]
fn told_where_to_look_it_searches_there_and_the_rest_know_nothing_of_it() {
    let mut world = World::new(&[(at(4.0, 2.0), EAST), (at(38.0, 2.0), WEST)], GONE);
    let happened = at(30.0, 12.0);
    world.sentries[0].tell(happened);
    let mut closest = f32::MAX;
    for _ in 0..(20 * 60) {
        world.run(1.0 / 60.0, 1.0 / 60.0);
        closest = closest.min(flat(world.bodies[0].feet - happened).norm());
    }
    assert!(world.first(0, Happened::Told).is_some());
    assert!(closest < 5.0, "{closest}");
    assert!(world.sentries[1].alert().is_none() && world.log.iter().all(|&(_, s, _)| s == 0));
}

#[test]
fn one_that_sees_another_give_chase_joins_in_and_one_looking_away_does_not() {
    let mut world = World::new(&[(at(20.0, 2.0), EAST), (at(5.0, 2.0), EAST), (at(5.0, 1.0), WEST)], at(36.0, 2.0));
    world.run(6.0, 1.0 / 60.0);
    let spotted = world.first(0, Happened::Spotted).expect("sentry 0 spotted them");
    assert!(world.first(1, Happened::Joined).is_some_and(|t| t >= spotted));
    assert!(world.first(1, Happened::Spotted).is_none());
    assert!(world.sentries[1].alert().is_some() && world.bodies[1].feet.x > 8.0, "on its way");
    assert!(world.sentries[2].alert().is_none() && world.first(2, Happened::Joined).is_none());
}

#[test]
fn one_that_still_sees_another_on_the_chase_stays_on_it_rather_than_flickering() {
    // The guard at the far end spots the player; the one on the beat sees it give chase. The
    // player runs into the room: whichever loses sight of them first still sees the other on the
    // chase, and stays on it - each loses them once, not every other frame.
    let sentries = vec![
        (Sentry::new(Role::Sentry).with_beat(vec![at(3.0, 2.0), at(14.0, 2.0)]), at(8.0, 2.0), EAST),
        (Sentry::new(Role::Sentry), at(38.0, 2.0), WEST),
    ];
    let mut world = World::with(sentries, at(20.0, 2.0));
    world.run(3.0, 1.0 / 60.0);
    world.path = vec![at(30.0, 2.0), at(30.0, 9.0), at(35.5, 17.5)];
    world.run(4.0, 1.0 / 60.0);
    world.player = GONE;
    world.path.clear();
    world.run(10.0, 1.0 / 60.0);
    for sentry in 0..2 {
        let count = |what| world.log.iter().filter(|&&(_, s, h)| s == sentry && h == what).count();
        assert_eq!((count(Happened::LostContact), count(Happened::Joined) + count(Happened::Spotted)), (1, 1), "sentry {sentry}: {:?}", world.log);
        assert_eq!(world.sentries[sentry].alert(), Some(Alert::Evasion));
    }
}

/// Two sentries side by side, spotting the player, who is then gone: both searching. When they
/// lost them.
fn two_searching(traits: [Traits; 2]) -> (World, f32) {
    let sentries = traits.iter().zip([at(8.0, 1.0), at(8.0, 3.0)]).map(|(&t, feet)| (Sentry::new(Role::Sentry).with_traits(t), feet, EAST)).collect();
    let mut world = World::with(sentries, at(20.0, 2.0));
    world.run(4.0, 1.0 / 60.0);
    world.player = GONE;
    world.run(0.5, 1.0 / 60.0);
    let lost = world.first(0, Happened::LostContact).expect("lost them");
    assert!(world.first(1, Happened::LostContact).is_some());
    (world, lost)
}

#[test]
fn a_fresh_search_is_kept_to_over_a_faint_noise_but_a_cold_one_is_left_for_a_clear_lead() {
    let (mut fresh, _) = two_searching([Traits::NEUTRAL; 2]);
    fresh.sentries[0].hear(&AI, at(14.0, 2.0), 0.3);
    fresh.run(1.0, 1.0 / 60.0);
    assert!(fresh.first(0, Happened::Noticed).is_some(), "it noticed");
    assert_eq!(fresh.sentries[0].conduct(), Some(Conduct::Search), "{:?}", fresh.sentries[0].context(&AI));
    // Passed over, the faint lead goes cold and is let go: once the search runs out, it goes back
    // to its routine, not off to look into a noise from a minute before.
    while fresh.first(0, Happened::GaveUp).is_none() {
        fresh.run(1.0, 1.0 / 60.0);
        assert!(fresh.t < 200.0);
    }
    fresh.run(2.0, 1.0 / 60.0);
    assert_eq!(fresh.sentries[0].conduct(), Some(Conduct::GoAbout));
    assert_eq!(fresh.intents[0], Intent::Routine);

    let (mut cold, lost) = two_searching([Traits::NEUTRAL; 2]);
    cold.run(lost + 25.0 - cold.t, 1.0 / 60.0);
    assert_eq!(cold.sentries[0].alert(), Some(Alert::Evasion), "still searching");
    cold.sentries[0].hear(&AI, at(14.0, 2.0), 0.9);
    // Once it has finished the spot it is searching, it goes to look.
    assert!(goes_to_look(&mut cold, 0, 6.0), "{:?}", cold.sentries[0].context(&AI));
}

/// Whether sentry `s` goes to look into what it noticed in the next `seconds`.
fn goes_to_look(world: &mut World, s: usize, seconds: f32) -> bool {
    (0..(seconds * 60.0) as usize).any(|_| {
        world.run(1.0 / 60.0, 1.0 / 60.0);
        world.sentries[s].conduct() == Some(Conduct::Investigate)
    })
}

#[test]
fn the_curious_follow_a_new_lead_where_the_incurious_keep_to_the_search() {
    let curious = Traits { curiosity: 0.95, ..Traits::NEUTRAL };
    let incurious = Traits { curiosity: 0.05, ..Traits::NEUTRAL };
    let (mut world, lost) = two_searching([curious, incurious]);
    world.run(lost + 15.0 - world.t, 1.0 / 60.0);
    for s in 0..2 {
        world.sentries[s].hear(&AI, at(14.0, 2.0), 0.7);
    }
    let mut incurious = world.sentries[1].conduct() == Some(Conduct::Investigate);
    let mut curious = false;
    for _ in 0..(6 * 60) {
        world.run(1.0 / 60.0, 1.0 / 60.0);
        curious |= world.sentries[0].conduct() == Some(Conduct::Investigate);
        incurious |= world.sentries[1].conduct() == Some(Conduct::Investigate);
    }
    assert!(curious && !incurious, "curious {curious}, incurious {incurious}");
}

#[test]
fn its_context_says_why() {
    let (world, _) = two_searching([Traits::NEUTRAL; 2]);
    let context = world.sentries[0].context(&AI);
    assert!(context.urgency > 0.5 && context.urgency < 1.0, "searching: pressing, not as pressing as a chase");
    assert!(context.sighting.is_some_and(|s| s > 0.9), "lost a moment ago");
    assert!(context.lead.is_none());
    assert_eq!(context.doing.map(|(c, _)| c), Some(Conduct::Search));
    let weighed = priority::factors(&AI.priority, &Traits::NEUTRAL, &context, Conduct::Search);
    assert!(weighed.confidence > 1.0 && weighed.urgency == 1.0 && weighed.traits == 1.0);
}

#[test]
fn searchers_split_up_rather_than_all_go_to_the_same_place() {
    let mut world = World::new(&[(at(26.0, 2.0), EAST), (at(27.0, 1.0), EAST), (at(27.0, 3.0), EAST)], at(30.0, 2.0));
    world.run(3.0, 1.0 / 60.0);
    assert!((0..3).all(|s| world.first(s, Happened::Spotted).is_some() || world.first(s, Happened::Joined).is_some()));
    // Gone into the room and away: three searchers, three different places to look.
    world.path = vec![at(30.0, 9.0)];
    world.run(0.6, 1.0 / 60.0);
    world.player = GONE;
    world.path.clear();
    let mut apart = false;
    for _ in 0..(10 * 60) {
        world.run(1.0 / 60.0, 1.0 / 60.0);
        let goals: Vec<V> = world.sentries.iter().filter_map(|s| s.goal()).collect();
        if goals.len() >= 2 {
            let pairs = (0..goals.len()).flat_map(|a| (a + 1..goals.len()).map(move |b| (a, b)));
            let all_apart = pairs.clone().all(|(a, b)| flat(goals[a] - goals[b]).norm() >= AI.orchestrate.apart - 1.0e-3);
            apart |= all_apart;
            // No two ever make for the very same spot.
            assert!(pairs.clone().all(|(a, b)| flat(goals[a] - goals[b]).norm() > 1.0e-3), "{goals:?}");
        }
    }
    assert!(apart, "went searching different places");
    assert!(world.sentries.iter().all(|s| s.alert() == Some(Alert::Evasion)));
}

#[test]
fn with_no_way_to_where_it_noticed_something_it_gives_the_look_up() {
    let mut world = World::new(&[(at(8.0, 2.0), EAST)], GONE);
    world.sentries[0].hear(&AI, at(18.0, 2.0), 1.0);
    // Setting off to look, the game finds no way there - a door shut since, say.
    while !matches!(world.intents.first(), Some(Intent::GoTo { .. })) {
        world.run(1.0 / 60.0, 1.0 / 60.0);
    }
    world.sentries[0].no_way(&AI);
    world.run(0.5, 1.0 / 60.0);
    assert!(world.first(0, Happened::Noticed).is_some());
    assert!(world.first(0, Happened::InvestigationDone).is_some(), "{:?}", world.log);
    // Attentive, it stands looking that way a moment, and then goes back to its routine.
    assert!(world.sentries[0].attentive() && matches!(world.intents[0], Intent::Face { .. }));
    world.run(AI.orchestrate.linger, 1.0 / 60.0);
    assert_eq!(world.intents[0], Intent::Routine);
}

/// Spotted, lost for good, searched for, given up, calm: the whole incident, for a sentry on a beat.
fn incident(dt: impl Fn(usize) -> f32 + Copy) -> World {
    let sentry = Sentry::new(Role::Sentry).with_beat(vec![at(4.0, 2.0), at(12.0, 2.0)]);
    let mut world = World::with(vec![(sentry, at(8.0, 2.0), EAST)], at(20.0, 2.0));
    world.run_by(4.0, dt);
    world.player = GONE;
    world.run_by((AI.alert.evasion + AI.alert.caution) * (1.0 + AI.alert.longer) + 10.0, dt);
    world
}

#[test]
fn after_an_incident_it_goes_back_to_walking_its_beat() {
    let mut world = incident(|_| 1.0 / 60.0);
    let order: Vec<Happened> = world.log.iter().map(|&(_, _, h)| h).collect();
    assert_eq!(order, vec![Happened::Noticed, Happened::Spotted, Happened::LostContact, Happened::GaveUp, Happened::Calm]);
    // Walking its beat: from one post to the other and back.
    let mut xs = Vec::new();
    for _ in 0..(30 * 60) {
        world.run(1.0 / 60.0, 1.0 / 60.0);
        xs.push(world.bodies[0].feet.x);
        assert!(matches!(world.intents[0], Intent::GoTo { pace: Pace::Walk, .. }), "{:?}", world.intents[0]);
    }
    let (least, most) = xs.iter().fold((f32::MAX, f32::MIN), |(a, b), &x| (a.min(x), b.max(x)));
    assert!(least < 5.0 && most > 11.0, "between its posts: {least}..{most}");
    let was = world.sentries[0].wariness.level;
    world.run(1800.0, 0.5);
    assert!(world.sentries[0].wariness.level < was * 0.05, "its wariness fades");
}

/// How long the `n`th frame is, in seconds.
type Frames = fn(usize) -> f32;

#[test]
fn an_incident_plays_out_the_same_at_any_frame_rate() {
    let line = |world: &World| world.log.iter().map(|&(t, _, h)| (h, t)).collect::<Vec<_>>();
    let reference = line(&incident(|_| 1.0 / 60.0));
    let rates: [(&str, Frames); 4] = [
        ("30 Hz", |_| 1.0 / 30.0),
        ("144 Hz", |_| 1.0 / 144.0),
        ("uneven", |n| [1.0 / 25.0, 1.0 / 90.0, 1.0 / 60.0, 1.0 / 200.0, 1.0 / 40.0][n % 5]),
        ("hitching", |n| if n % 50 == 0 { 0.1 } else { 1.0 / 120.0 }),
    ];
    for (name, dt) in rates {
        let other = line(&incident(dt));
        assert_eq!(other.iter().map(|&(h, _)| h).collect::<Vec<_>>(), reference.iter().map(|&(h, _)| h).collect::<Vec<_>>(), "{name}");
        for (&(h, then), &(_, now)) in reference.iter().zip(&other) {
            assert!((now - then).abs() <= 0.25 + then * 0.002, "{name}: {h:?} at {now} against {then}");
        }
    }
    assert_eq!(line(&incident(|n| [1.0 / 25.0, 1.0 / 90.0][n % 2])), line(&incident(|n| [1.0 / 25.0, 1.0 / 90.0][n % 2])), "deterministic");
}

#[test]
fn nonsense_from_the_game_is_let_go() {
    let mut world = World::new(&[(at(8.0, 2.0), EAST)], at(20.0, 2.0));
    let mesh = level();
    let senses = [Senses { feet: V::new(f32::NAN, 0.0, 0.0), heading: f32::INFINITY, visible: f32::NAN, player: V::new(f32::NAN, 0.0, 0.0), threat: f32::NAN }];
    for dt in [f32::NAN, -1.0, f32::INFINITY, 1.0 / 60.0] {
        let thoughts = world.squad.step(&AI, &mesh, &mut world.reasoners, &mut world.sentries, &senses, false, |_, _| true, dt);
        assert_eq!(thoughts.len(), 1);
        assert!(thoughts[0].happened.is_empty());
    }
    assert!(world.sentries[0].alert().is_none() && world.sentries[0].feet() == V::zeros());
    world.sentries[0].tell(V::new(f32::NAN, 0.0, 0.0));
    world.sentries[0].hear(&AI, at(10.0, 2.0), f32::NAN);
    world.squad.noise(&AI, &mesh, &mut world.sentries, at(10.0, 2.0), f32::INFINITY);
    world.run(1.0, 1.0 / 60.0);
    assert!(world.log.is_empty() || world.log.iter().all(|&(_, _, h)| h != Happened::Told));
    // More senses than sentries, or fewer: only those with both.
    let two = [senses[0], senses[0]];
    assert_eq!(world.squad.step(&AI, &mesh, &mut world.reasoners, &mut world.sentries, &two, false, |_, _| true, 0.1).len(), 1);
    assert!(world.squad.step(&AI, &mesh, &mut world.reasoners, &mut world.sentries, &[], false, |_, _| true, 0.1).is_empty());
}

#[test]
fn the_modules_do_not_depend_on_the_orchestration_layer() {
    // Optional: nothing but the crate root (to declare it and gather its tuning) and validation
    // names it.
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut stack = vec![root];
    let mut users = Vec::new();
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else if std::fs::read_to_string(&path).unwrap().contains("orchestrate") {
                users.push(path.file_name().unwrap().to_string_lossy().into_owned());
            }
        }
    }
    users.sort();
    assert_eq!(users, vec!["lib.rs", "orchestrate.rs", "validate.rs"]);
}


// Persistence (ROADMAP 2.3).

/// Where sentry 0 is going to look, if it is investigating.
fn looking_into(world: &World) -> Option<V> {
    world.sentries[0].investigating().filter(|_| world.sentries[0].conduct() == Some(Conduct::Investigate))
}

/// Runs `world` on `seconds`, giving back each spot sentry 0 went to look into, in order, and how
/// near it got to each.
fn spots_looked_into(world: &mut World, seconds: f32) -> Vec<(V, f32)> {
    let mut spots: Vec<(V, f32)> = Vec::new();
    for _ in 0..(seconds * 60.0) as usize {
        world.run(1.0 / 60.0, 1.0 / 60.0);
        if let Some(spot) = looking_into(world) {
            if spots.last().is_none_or(|&(last, _)| flat(last - spot).norm() > 0.5) {
                spots.push((spot, f32::MAX));
            }
        }
        if let Some((spot, closest)) = spots.last_mut() {
            *closest = closest.min(flat(*spot - world.bodies[0].feet).norm());
        }
    }
    spots
}

#[test]
fn it_finishes_looking_where_it_nearly_is_before_a_second_noise_and_then_looks_there_too() {
    let mut world = World::new(&[(at(8.0, 2.0), EAST)], GONE);
    let (first, second) = (at(15.0, 2.0), at(2.0, 2.0));
    world.sentries[0].hear(&AI, first, 0.8);
    // Nearly there, another noise as loud, the other way.
    while looking_into(&world).is_none_or(|_| flat(first - world.bodies[0].feet).norm() > AI.orchestrate.finish_within) {
        world.run(1.0 / 60.0, 1.0 / 60.0);
        assert!(world.t < 20.0);
    }
    world.sentries[0].hear(&AI, second, 0.8);
    let spots = spots_looked_into(&mut world, 40.0);
    let near = |(spot, closest): (V, f32), to: V| flat(spot - to).norm() < 0.5 && closest < AI.investigate.reach + 0.5;
    assert!(spots.len() == 2 && near(spots[0], first) && near(spots[1], second), "{spots:?}");
    let done: Vec<f32> = world.log.iter().filter(|&&(_, _, h)| h == Happened::InvestigationDone).map(|&(t, _, _)| t).collect();
    assert_eq!(done.len(), 2, "both looked into to the end: {:?}", world.log);
}

#[test]
fn a_clearly_surer_lead_takes_over_one_just_set_off_for_and_a_lesser_one_waits_its_turn() {
    // Just set off for a faint noise far off; a loud one close by: that first, then the faint one.
    let mut world = World::new(&[(at(8.0, 2.0), EAST)], GONE);
    let (faint, loud) = (at(20.0, 2.0), at(4.0, 2.0));
    world.sentries[0].hear(&AI, faint, 0.45);
    world.run(1.2, 1.0 / 60.0);
    assert_eq!(looking_into(&world), Some(faint));
    world.sentries[0].hear(&AI, loud, 0.9);
    let spots = spots_looked_into(&mut world, 40.0);
    assert!(spots.len() >= 2 && flat(spots[0].0 - loud).norm() < 0.5 && flat(spots[1].0 - faint).norm() < 0.5, "{spots:?}");

    // A loud noise, then one not clearly surer: it keeps to the first, the second waiting.
    let mut world = World::new(&[(at(8.0, 2.0), EAST)], GONE);
    world.sentries[0].hear(&AI, loud, 0.8);
    world.run(1.2, 1.0 / 60.0);
    world.sentries[0].hear(&AI, faint, 0.9);
    let spots = spots_looked_into(&mut world, 40.0);
    assert!(spots.len() >= 2 && flat(spots[0].0 - loud).norm() < 0.5 && flat(spots[1].0 - faint).norm() < 0.5, "{spots:?}");
}

#[test]
fn a_noise_too_faint_to_look_into_has_it_look_that_way_and_listen_the_keener_a_while() {
    let mut world = World::new(&[(at(8.0, 2.0), EAST)], GONE);
    world.sentries[0].hear(&AI, at(2.0, 2.0), 0.2);
    world.run(0.5, 1.0 / 60.0);
    assert!(world.first(0, Happened::Noticed).is_none(), "not enough to go and look");
    assert!(world.sentries[0].attentive());
    // It turns to look behind it, where it heard it, and then back to its routine.
    assert!(matches!(world.intents[0], Intent::Face { heading } if (heading - WEST).abs() < 0.1), "{:?}", world.intents[0]);
    world.run(AI.orchestrate.linger, 1.0 / 60.0);
    assert_eq!(world.intents[0], Intent::Routine);
    world.run(AI.orchestrate.attentive_for, 1.0 / 60.0);
    assert!(!world.sentries[0].attentive(), "for a while, not for ever");
}

#[test]
fn attentive_it_makes_out_someone_sooner() {
    // Two sentries, each facing east down the corridor; one has just heard something faint
    // ahead. Once what it heard has drained from its awareness, someone steps out at the far end.
    let noticed = |heard: bool| {
        let mut world = World::new(&[(at(4.0, 2.0), EAST)], GONE);
        if heard {
            world.sentries[0].hear(&AI, at(12.0, 2.0), 0.2);
        }
        world.run(3.0, 1.0 / 60.0);
        assert!(world.sentries[0].detection.level < 1.0e-3);
        assert_eq!(world.sentries[0].attentive(), heard);
        world.player = at(31.0, 2.0);
        let from = world.t;
        world.run(4.0, 1.0 / 60.0);
        world.first(0, Happened::Noticed).map(|t| t - from)
    };
    let (keen, plain) = (noticed(true).expect("noticed"), noticed(false).expect("noticed"));
    assert!(keen < plain * 0.8, "attentive {keen:.2} s, not {plain:.2} s");
}

#[test]
fn a_storm_of_noises_does_not_scatter_it() {
    // Twenty noises about the corridor in twelve seconds, loud and faint, while it investigates.
    let mut world = World::new(&[(at(20.0, 2.0), EAST)], GONE);
    let mut seed = 7u32;
    let mut next = move || {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (seed >> 8) as f32 / (1u32 << 24) as f32
    };
    let mut target: Option<V> = None;
    let mut switches = 0;
    for frame in 0..(60 * 60) {
        if frame % 36 == 0 && frame < 12 * 60 {
            let (x, loud) = (2.0 + next() * 36.0, 0.2 + next() * 0.8);
            world.sentries[0].hear(&AI, at(x, 2.0), loud);
        }
        let done_before = world.log.iter().filter(|&&(_, _, h)| h == Happened::InvestigationDone).count();
        world.run(1.0 / 60.0, 1.0 / 60.0);
        let finished = world.log.iter().filter(|&&(_, _, h)| h == Happened::InvestigationDone).count() > done_before;
        let now = looking_into(&world);
        if let (Some(was), Some(now)) = (target, now) {
            if flat(was - now).norm() > 0.5 {
                let near = flat(was - world.bodies[0].feet).norm() <= AI.orchestrate.finish_within;
                assert!(finished || !near, "turned from a look it was {:.1} m from finishing", flat(was - world.bodies[0].feet).norm());
                switches += 1;
            }
        }
        // A look finished is no longer one to turn from.
        target = if finished { now } else { now.or(target) };
    }
    let done = world.log.iter().filter(|&&(_, _, h)| h == Happened::InvestigationDone).count();
    assert!(done >= 3, "it finished looking into some: {done}");
    assert!(switches <= done + 3, "{switches} changes of where to look, {done} looks finished");
    assert_eq!(world.intents[0], Intent::Routine, "and in the end, back to its routine");
}

#[test]
fn persistence_can_be_switched_off() {
    // With nothing held, a second noise as loud takes over at once, near the first spot or not.
    let mut world = World::new(&[(at(8.0, 2.0), EAST)], GONE);
    world.ai.orchestrate.hold = 0.0;
    world.ai.orchestrate.finish_within = 0.0;
    world.ai.orchestrate.switch_lead_by = 0.0;
    let (first, second) = (at(15.0, 2.0), at(2.0, 2.0));
    world.sentries[0].hear(&world.ai.clone(), first, 0.8);
    while looking_into(&world).is_none_or(|_| flat(first - world.bodies[0].feet).norm() > 4.0) {
        world.run(1.0 / 60.0, 1.0 / 60.0);
    }
    world.sentries[0].hear(&world.ai.clone(), second, 0.8);
    world.run(0.2, 1.0 / 60.0);
    assert_eq!(looking_into(&world), Some(second));
    assert!(world.first(0, Happened::InvestigationDone).is_none(), "the first given up unfinished");
}

// Memory (ROADMAP 3.1).

#[test]
fn what_happens_to_it_is_remembered() {
    let world = incident(|_| 1.0 / 60.0);
    let memory = &world.sentries[0].memory;
    let spotted = memory.recall(Kind::Encounter);
    assert!(spotted.len() == 1 && spotted[0].at.is_some_and(|seen| flat(seen - at(20.0, 2.0)).norm() < 0.5), "{spotted:?}");
    let lost = memory.recall(Kind::Place);
    assert!(lost.len() == 1 && lost[0].firsthand, "where it lost them: {lost:?}");

    let mut world = World::new(&[(at(8.0, 2.0), EAST)], GONE);
    world.sentries[0].hear(&AI, at(15.0, 2.0), 0.8);
    world.run(25.0, 1.0 / 60.0);
    let looked = world.sentries[0].memory.recall(Kind::Investigation);
    assert!(looked.len() == 1 && looked[0].outcome.is_some_and(|o| o < 0.0), "a look that found nothing: {looked:?}");

    let mut world = World::new(&[(at(4.0, 2.0), EAST)], GONE);
    world.sentries[0].tell(at(30.0, 12.0));
    world.run(0.5, 1.0 / 60.0);
    let told = world.sentries[0].memory.recall(Kind::Event);
    assert!(told.len() == 1 && !told[0].firsthand, "what it was told, secondhand");
}

#[test]
fn it_does_not_walk_back_to_look_where_it_just_looked_till_it_has_half_forgotten() {
    let mut world = World::new(&[(at(8.0, 2.0), EAST)], GONE);
    let spot = at(15.0, 2.0);
    let looks = |world: &World| world.log.iter().filter(|&&(_, _, h)| h == Happened::Noticed).count();
    world.sentries[0].hear(&AI, spot, 0.8);
    world.run(25.0, 1.0 / 60.0);
    assert_eq!((looks(&world), world.first(0, Happened::InvestigationDone).is_some()), (1, true));
    // The same noise from the same spot, a little after: a glance that way, not another walk.
    world.run(10.0, 1.0 / 60.0);
    world.sentries[0].hear(&AI, spot, 0.8);
    world.run(0.5, 1.0 / 60.0);
    assert_eq!(looks(&world), 1, "{:?}", world.log);
    assert!(world.sentries[0].attentive() && matches!(world.intents[0], Intent::Face { .. }));
    // Ten minutes on, the look half forgotten: it goes to look again.
    world.run(600.0, 0.5);
    world.sentries[0].hear(&AI, spot, 0.8);
    world.run(0.5, 1.0 / 60.0);
    assert_eq!(looks(&world), 2);
    assert_eq!(world.sentries[0].investigating(), Some(spot));
}

#[test]
fn where_it_was_shot_at_feels_dangerous_when_it_is_back_there() {
    let sentry = Sentry::new(Role::Sentry).with_beat(vec![at(4.0, 2.0), at(30.0, 2.0)]);
    let mut world = World::with(vec![(sentry, at(4.0, 2.0), EAST)], GONE);
    // Shot at, standing at the first post.
    world.threat = 0.9;
    world.run(0.2, 1.0 / 60.0);
    world.threat = 0.0;
    assert_eq!(world.sentries[0].memory.recall(Kind::Danger).len(), 1);
    let (mut there, mut away) = (0.0f32, 1.0f32);
    for _ in 0..(80 * 60) {
        world.run(1.0 / 60.0, 1.0 / 60.0);
        let threat = world.sentries[0].context(&AI).threat;
        let x = world.bodies[0].feet.x;
        if x < 5.0 {
            there = there.max(threat);
        }
        if x > 20.0 {
            away = away.min(threat);
        }
    }
    assert!(there > 0.3, "back at the post where it was shot at: {there}");
    assert_eq!(away, 0.0, "elsewhere, nothing");
}

// Memory retrieval (ROADMAP 3.3).

#[test]
fn a_searcher_looks_first_where_it_remembers_finding_them_before() {
    use hydroxus_ai::think::memory::Record;
    let two_rooms = mirrored_rooms;
    // Told the player is in the corridor's middle, it comes up the passage to search: by then
    // they could be in either room. Which does it go into first?
    let first_room = |remembered: Option<V>| {
        let mut sentry = Sentry::new(Role::Sentry);
        if let Some(there) = remembered {
            sentry.memory.note(&AI.memory, Record::new(Kind::Investigation).at(there).outcome(1.0).importance(0.8));
        }
        let mut world = World::on(two_rooms(), vec![(sentry, at(18.0, 1.0), 0.0)], GONE);
        // A witness saw them there five seconds ago, and has just come to tell it.
        world.sentries[0].tell_from(at(18.0, 8.0), 5.0);
        for _ in 0..(40 * 60) {
            world.run(1.0 / 60.0, 1.0 / 60.0);
            let feet = world.bodies[0].feet;
            if feet.z > 14.5 {
                return Some(if feet.x < 18.0 { "west" } else { "east" });
            }
        }
        None
    };
    let plain = first_room(None).expect("it searched a room");
    assert_eq!(first_room(Some(at(5.0, 20.0))), Some("west"), "remembering finding them in the west room (plain: {plain})");
    assert_eq!(first_room(Some(at(31.0, 20.0))), Some("east"), "remembering finding them in the east room (plain: {plain})");

    // What it recalled, and why: the memory fits the cue of where it lost them.
    let mut sentry = Sentry::new(Role::Sentry);
    sentry.memory.note(&AI.memory, Record::new(Kind::Investigation).at(at(37.0, 2.0)).outcome(1.0).importance(0.8));
    sentry.memory.note(&AI.memory, Record::new(Kind::Investigation).at(at(10.0, 2.0)).outcome(-0.5));
    sentry.memory.note(&AI.memory, Record::new(Kind::Investigation).at(at(39.0, 400.0)).outcome(1.0));
    let recalled = sentry.recall_haunts(&AI, at(30.0, 6.0));
    assert_eq!(recalled.len(), 1, "where it found them, near enough; not where it found nothing, nor far off: {recalled:?}");
    assert!(recalled[0].1 > 0.0);
}

/// Mirrored about x = 18: a corridor (z 6..10, x 0..36) with a room off it either side of the
/// middle, out of sight of each other and of the corridor - a doorway at x 7..9 north into a west
/// room, x 2..14, and one at x 27..29 into an east room, x 22..34, both z 14..22 - and a passage
/// south of the middle (x 17..19, z 0..6), out of sight of most of it. The passage, and the
/// corridor's middle 4 m, are their own areas, so that the navmesh is cut the same both ways.
fn mirrored_rooms() -> NavMesh {
    let mut grid = WalkGrid::new(36, 24, 1.0);
    for z in 0..24 {
        for x in 0..36 {
            let passage = z < 6 && (17..19).contains(&x);
            let corridor = (6..10).contains(&z);
            let doorways = (10..14).contains(&z) && ((7..9).contains(&x) || (27..29).contains(&x));
            let rooms = (14..22).contains(&z) && ((2..14).contains(&x) || (22..34).contains(&x));
            grid.set(x, z, passage || corridor || doorways || rooms);
        }
    }
    NavMesh::build(&grid, V::zeros(), |x, z| if z < 6 { 1 } else if (16..20).contains(&x) && z < 10 { 2 } else { 0 })
}

// Learning evaluated (ROADMAP 4.2).

#[test]
fn over_repeated_searches_a_sentry_that_remembers_finds_them_sooner_and_one_that_does_not_does_not() {
    use hydroxus_ai::think::evaluate::{judge, Judgement, Tally};
    // A player who always hides in the east room; told of them in the corridor some seconds ago
    // each time, a sentry comes up the passage to search. Left to itself it tries the west room
    // first. How long, each time, till it finds them?
    let hideout = at(31.0, 20.0);
    let episode = |sentry: Sentry, ago: f32| -> (Sentry, f32) {
        let mut world = World::on(mirrored_rooms(), vec![(sentry, at(18.0, 1.0), 0.0)], hideout);
        world.sentries[0].tell_from(at(18.0, 8.0), ago);
        for _ in 0..(60 * 60) {
            world.run(1.0 / 60.0, 1.0 / 60.0);
            if world.first(0, Happened::FoundAgain).is_some() {
                break;
            }
        }
        let took = world.first(0, Happened::FoundAgain).unwrap_or(60.0);
        (world.sentries.remove(0), took)
    };
    let carry = |had: &Sentry| {
        let mut next = Sentry::new(Role::Sentry);
        next.memory = had.memory.clone();
        next.experience = had.experience.clone();
        next
    };
    let ages = [3.0, 4.5, 6.0, 3.5, 5.0, 7.0, 4.0, 5.5];
    let mut remembering = Sentry::new(Role::Sentry);
    let (mut learned, mut unlearned) = (Vec::new(), Vec::new());
    for &ago in &ages {
        let (had, took) = episode(carry(&remembering), ago);
        remembering = had;
        learned.push(took);
        let (_, took) = episode(Sentry::new(Role::Sentry), ago);
        unlearned.push(took);
    }
    println!("seconds to find them, remembering: {learned:?}; fresh each time: {unlearned:?}");
    // The first time, both are the same: neither knows anything.
    assert!((learned[0] - unlearned[0]).abs() < 1.0e-3);
    // After that, the one that remembers finds them sooner, and the fresh one no sooner than it
    // did the first time. (The world is deterministic, and how old the report is turns out not to
    // change the times here, so each side has no spread and the means alone decide.)
    let tally = |xs: &[f32]| {
        let mut t = Tally::default();
        for &x in xs {
            t.add(-x);
        }
        t
    };
    let (after, control) = (tally(&learned[1..]), tally(&unlearned[1..]));
    assert!(matches!(judge(&control, &after, 1.96, 5), Judgement::Better { by } if by > 2.0), "{:?} against {:?}", after.mean(), control.mean());
    assert!(unlearned[1..].iter().all(|&t| t > learned[0] - 3.0));
}

// Information exchange (ROADMAP 5.2).

#[test]
fn word_of_a_sighting_goes_by_radio_far_and_by_voice_near_and_not_at_all_without_either() {
    use hydroxus_ai::social::word::Word;
    // Sentry 0 in the corridor spots the player; sentry 1 is off in the room, out of sight of both
    // and too far to be shouted to.
    let run = |radios: bool| {
        let (mut a, mut b) = (Sentry::new(Role::Sentry), Sentry::new(Role::Sentry));
        if radios {
            a = a.with_radio();
            b = b.with_radio();
        }
        let mut world = World::with(vec![(a, at(8.0, 2.0), EAST), (b, at(31.0, 15.0), 0.0)], at(20.0, 2.0));
        world.run(25.0, 1.0 / 60.0);
        world
    };
    let world = run(true);
    let spotted = world.first(0, Happened::Spotted).expect("sentry 0 spotted them");
    let told = world.first(1, Happened::Told).expect("by radio, sentry 1 was told");
    assert!(told >= spotted && told - spotted < 0.1, "at once: {spotted} {told}");
    assert!(world.first(1, Happened::FoundAgain).or(world.first(1, Happened::Joined)).is_some(), "it came, and found them where it was told: {:?}", world.log);
    let event = world.sentries[1].memory.recall(Kind::Event);
    assert!(event.len() == 1 && !event[0].firsthand, "told: secondhand");
    assert!(world.sentries[1].trust.of(&AI.word, 0) > AI.word.trust, "sentry 0's word proved good");
    let world = run(false);
    assert!(world.first(1, Happened::Told).is_none() && world.sentries[1].alert().is_none(), "without radios, nothing: {:?}", world.log);

    // Near enough to be shouted to, its back to both sentry 0 and the player, so that it sees
    // neither: by voice, no radio needed.
    let mut world = World::new(&[(at(8.0, 2.0), EAST), (at(2.0, 2.0), WEST)], at(20.0, 2.0));
    world.run(3.0, 1.0 / 60.0);
    let spotted = world.first(0, Happened::Spotted).expect("sentry 0 spotted them");
    let told = world.first(1, Happened::Told).expect("told by voice");
    assert!(told - spotted < 0.1, "{:?}", world.log);
    // Told first; whatever it saw for itself, it saw after, having turned to go.
    assert!(world.first(1, Happened::Joined).or(world.first(1, Happened::Spotted)).is_none_or(|t| t > told), "{:?}", world.log);
    let _ = Word::seen((), at(0.0, 0.0), 0.0, 0);
}

#[test]
fn stale_word_is_doubted_and_fresh_word_acted_on() {
    use hydroxus_ai::social::word::Word;
    let mut world = World::new(&[(at(4.0, 2.0), EAST)], GONE);
    world.run(130.0, 0.25);
    let now = world.squad.now();
    // Seen two minutes ago, passed on once: not worth going after.
    let stale = Word::seen((), at(30.0, 12.0), now - 120.0, 50).passed_on(&AI.word, 50);
    assert!(!world.sentries[0].hear_word(&AI, stale, now));
    world.run(0.5, 1.0 / 60.0);
    assert!(world.first(0, Happened::Doubted).is_some() && world.sentries[0].alert().is_none());
    // Seen a moment ago: it goes.
    let fresh = Word::seen((), at(30.0, 12.0), world.squad.now() - 1.0, 50).passed_on(&AI.word, 50);
    assert!(world.sentries[0].hear_word(&AI, fresh, world.squad.now()));
    world.run(0.5, 1.0 / 60.0);
    assert!(world.first(0, Happened::Told).is_some() && world.sentries[0].alert() == Some(Alert::Evasion));
}

#[test]
fn word_from_one_whose_word_keeps_proving_wrong_comes_to_be_doubted_and_a_trusted_one_still_believed() {
    use hydroxus_ai::social::word::Word;
    // A citizen (77) keeps telling the sentry the player is in the room. They never are.
    let mut world = World::new(&[(at(4.0, 2.0), EAST)], GONE);
    let mut went = 0;
    for _ in 0..6 {
        let now = world.squad.now();
        let word = Word::seen((), at(30.0, 12.0), now, 77).passed_on(&AI.word, 77);
        if !world.sentries[0].hear_word(&AI, word, now) {
            break;
        }
        went += 1;
        // It searches, finds nothing, gives up, and calms down.
        world.run(300.0, 0.25);
        assert!(world.sentries[0].alert().is_none() || world.sentries[0].alert() == Some(Alert::Caution));
    }
    world.run(0.1, 1.0 / 60.0);
    let doubted = world.log.iter().filter(|&&(_, _, h)| h == Happened::Doubted).count();
    println!("went on 77's word {went} times before doubting it; trust {:.2}", world.sentries[0].trust.of(&AI.word, 77));
    assert!(doubted == 1 && (1..=3).contains(&went), "went {went} times");
    assert!(world.sentries[0].trust.of(&AI.word, 77) < AI.word.trust);
    // The same word, from someone else it has no reason to doubt: it goes.
    let now = world.squad.now();
    assert!(world.sentries[0].hear_word(&AI, Word::seen((), at(30.0, 12.0), now, 78).passed_on(&AI.word, 78), now));
}

#[test]
fn what_it_sees_for_itself_comes_before_what_it_is_told() {
    use hydroxus_ai::social::word::Word;
    let mut world = World::new(&[(at(8.0, 2.0), EAST)], at(20.0, 2.0));
    world.run(3.0, 1.0 / 60.0);
    assert_eq!(world.sentries[0].alert(), Some(Alert::Alert));
    let now = world.squad.now();
    // Told they are somewhere else entirely, while it can see them: it takes no notice.
    assert!(!world.sentries[0].hear_word(&AI, Word::seen((), at(30.0, 15.0), now, 9).passed_on(&AI.word, 9), now));
    world.run(0.5, 1.0 / 60.0);
    assert_eq!(world.sentries[0].alert(), Some(Alert::Alert));
    assert!(matches!(world.intents[0], Intent::GoTo { at, pace: Pace::Run } if flat(at - world.player).norm() < 1.0));
}


#[test]
fn a_search_that_comes_to_nothing_widens_rather_than_stands_about() {
    // Told the player is right where it stands, in the open corridor: one look, and there is
    // nowhere left they could be - so near. In time, though, they could be further.
    let mut world = World::new(&[(at(20.0, 2.0), EAST)], GONE);
    world.sentries[0].tell(at(20.0, 2.0));
    let mut furthest = 0.0f32;
    for _ in 0..(20 * 60) {
        world.run(1.0 / 60.0, 1.0 / 60.0);
        furthest = furthest.max(flat(world.bodies[0].feet - at(20.0, 2.0)).norm());
    }
    assert!(furthest > 6.0, "it went searching further afield: {furthest:.1} m at most");
    assert_eq!(world.sentries[0].alert(), Some(Alert::Evasion));
}

