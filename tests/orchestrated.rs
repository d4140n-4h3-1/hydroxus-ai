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
}

impl World {
    fn new(posted: &[(V, f32)], player: V) -> Self {
        Self::with(posted.iter().map(|&(feet, heading)| (Sentry::new(Role::Sentry), feet, heading)).collect(), player)
    }

    fn with(sentries: Vec<(Sentry, V, f32)>, player: V) -> Self {
        let bodies = sentries.iter().map(|&(_, feet, heading)| Body { feet, heading, route: Vec::new(), to: None }).collect();
        Self {
            mesh: level(),
            squad: Squad::new(&AI),
            sentries: sentries.into_iter().map(|(s, _, _)| s).collect(),
            bodies,
            reasoners: Reasoners::default(),
            player,
            path: Vec::new(),
            t: 0.0,
            log: Vec::new(),
            intents: Vec::new(),
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
                Senses { feet: body.feet, heading: body.heading, visible, player: self.player }
            })
            .collect();
        let feet: Vec<V> = self.bodies.iter().map(|b| b.feet).collect();
        let mesh = &self.mesh;
        let thoughts = self.squad.step(&AI, mesh, &mut self.reasoners, &mut self.sentries, &senses, false, |a, b| mesh.sees_across(feet[a], feet[b]), dt);
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
                        if body.route.is_empty() && flat(at - body.feet).norm() > AI.orchestrate.arrived {
                            self.sentries[i].no_way(&AI);
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
    let senses = [Senses { feet: V::new(f32::NAN, 0.0, 0.0), heading: f32::INFINITY, visible: f32::NAN, player: V::new(f32::NAN, 0.0, 0.0) }];
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
