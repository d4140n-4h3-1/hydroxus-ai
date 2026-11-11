//! Group coordination and independent behaviour through the orchestration layer: a squad after
//! the player in an open yard. The game's part is as in `tests/orchestrated.rs`, cut down: rays
//! are `sees_across`, bodies walk straight along their routes.

use hydroxus_ai::{
    alert::Alert,
    flat, heading_of,
    navigation::{grid::WalkGrid, navmesh::NavMesh, walk::route_to},
    orchestrate::{Happened, Intent, Pace, Senses, Sentry, Squad},
    social::word::Word,
    perception::sight::{Sight, Stance},
    tactics::conduct::{Reasoners, Role},
    Tuning,
};
use nalgebra::Vector3;

type V = Vector3<f32>;
const EAST: f32 = std::f32::consts::FRAC_PI_2;
const CAUGHT: f32 = 1.5;

fn at(x: f32, z: f32) -> V {
    V::new(x, 0.0, z)
}

fn facing(from: V, to: V) -> f32 {
    heading_of(to - from).unwrap_or(0.0)
}

/// An open yard, 90 m by 40.
fn yard() -> NavMesh {
    yard_with(|_, _| false)
}

/// The yard, but for where `wall` says.
fn yard_with(wall: impl Fn(usize, usize) -> bool) -> NavMesh {
    let mut grid = WalkGrid::new(90, 40, 1.0);
    for z in 0..40 {
        for x in 0..90 {
            grid.set(x, z, !wall(x, z));
        }
    }
    NavMesh::build(&grid, V::zeros(), |_, _| 0)
}

struct Body {
    feet: V,
    heading: f32,
    route: Vec<V>,
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
    speed: f32,
    t: f32,
    log: Vec<(f32, usize, Happened)>,
    intents: Vec<Intent>,
    ai: Tuning,
}

impl World {
    fn new(ai: Tuning, sentries: Vec<(Sentry, V, f32)>, player: V, speed: f32) -> Self {
        let bodies = sentries.iter().map(|&(_, feet, heading)| Body { feet, heading, route: Vec::new(), to: None }).collect();
        Self {
            mesh: yard(),
            squad: Squad::new(&ai),
            sentries: sentries.into_iter().map(|(s, _, _)| s).collect(),
            bodies,
            reasoners: Reasoners::default(),
            player,
            path: Vec::new(),
            speed,
            t: 0.0,
            log: Vec::new(),
            intents: Vec::new(),
            ai,
        }
    }

    fn nearest(&self) -> f32 {
        self.bodies.iter().map(|b| flat(b.feet - self.player).norm()).fold(f32::INFINITY, f32::min)
    }

    fn step(&mut self, dt: f32) {
        self.t += dt;
        let mut left = self.speed * dt;
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
                Senses { feet: body.feet, heading: body.heading, visible, player: self.player, threat: 0.0 }
            })
            .collect();
        let feet: Vec<V> = self.bodies.iter().map(|b| b.feet).collect();
        let mesh = &self.mesh;
        let thoughts = self.squad.step(&self.ai, mesh, &mut self.reasoners, &mut self.sentries, &senses, false, |a, b| mesh.sees_across(feet[a], feet[b]), dt);
        self.intents.clear();
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

/// Three sentries with radios: one 12 m behind the player, and one either side of them, `side`
/// meters off and `ahead` meters ahead, all facing them; the player makes off east
/// at once, a little slower than a sentry runs, the squad stepped `hz`
/// times a second. How long after they make off, if at all, one of the squad gets to them; and
/// the most different places those after them made for at once.
fn chase_from(ai: Tuning, ahead: f32, side: f32, hz: f32) -> (Option<f32>, usize, World) {
    let start = at(16.0, 20.0);
    let sentries = vec![
        (Sentry::new(Role::Sentry).with_radio(), at(4.0, 20.0), EAST),
        (Sentry::new(Role::Sentry).with_radio(), at(16.0 + ahead, 20.0 - side), facing(at(16.0 + ahead, 20.0 - side), start)),
        (Sentry::new(Role::Sentry).with_radio(), at(16.0 + ahead, 20.0 + side), facing(at(16.0 + ahead, 20.0 + side), start)),
    ];
    let mut world = World::new(ai, sentries, start, 3.6);
    let dt = 1.0 / hz;
    // They make off at once; the squad spots them on the run.
    let off = world.t;
    world.path = vec![at(88.0, 20.0)];
    let mut caught = None;
    let mut most_apart = 0;
    while world.t < off + 25.0 {
        world.step(dt);
        let goals: Vec<V> = world.intents.iter().filter_map(|i| if let Intent::GoTo { at, pace: Pace::Run } = *i { Some(at) } else { None }).collect();
        let distinct = goals.iter().enumerate().filter(|&(k, g)| goals[..k].iter().all(|o| flat(o - g).norm() >= 2.0)).count();
        most_apart = most_apart.max(distinct);
        if world.nearest() <= CAUGHT {
            caught = Some(world.t - off);
            break;
        }
    }
    (caught, most_apart, world)
}

fn all_after_them() -> Tuning {
    Tuning { roles: hydroxus_ai::tactics::roles::Tuning { pursuers: 100, ..Tuning::DEFAULT.roles }, ..Tuning::DEFAULT }
}

#[test]
fn a_squad_with_parts_closes_in_from_more_than_one_side_and_gets_to_them_sooner() {
    let mut sooner = 0;
    let mut runs = 0;
    for ahead in [-4.0, 0.0, 6.0] {
        for side in [12.0, 16.0, 20.0] {
            for hz in [30.0, 60.0, 144.0] {
                let (with_parts, apart, world) = chase_from(Tuning::DEFAULT, ahead, side, hz);
                let (without, apart_without, _) = chase_from(all_after_them(), ahead, side, hz);
                println!("ahead {ahead} side {side} at {hz} Hz: with parts {with_parts:?} ({apart} places at once), all after them {without:?} ({apart_without})");
                runs += 1;
                assert!(world.log.iter().any(|&(_, _, h)| h == Happened::CuttingOff), "someone cut them off");
                assert!(apart >= 2 && apart_without == 1, "{apart} {apart_without}");
                let with_parts = with_parts.expect("with parts, they got to them");
                let without = without.unwrap_or(f32::INFINITY);
                // Never later than all running after them (a frame or two either way aside).
                assert!(with_parts <= without + 0.1, "later: {with_parts} against {without}");
                if with_parts < without - 0.1 {
                    sooner += 1;
                }
                // Flankers ahead of them: clearly sooner, cutting them off rather than trailing.
                if ahead > 0.0 {
                    assert!(with_parts < without - 1.5, "{with_parts} against {without}");
                }
            }
        }
    }
    println!("sooner {sooner} of {runs}");
    // Flankers level with them or behind, too close to the side, end up trailing whatever their
    // part: as soon, no sooner.
    assert!(sooner * 3 >= runs * 2, "sooner in {sooner} of {runs}");
}

#[test]
fn the_one_that_gets_to_them_was_running_after_them_by_its_own_call_whatever_its_part() {
    let (caught, _, world) = chase_from(Tuning::DEFAULT, 6.0, 16.0, 60.0);
    assert!(caught.is_some());
    let catcher = (0..3).min_by(|&a, &b| flat(world.bodies[a].feet - world.player).norm().total_cmp(&flat(world.bodies[b].feet - world.player).norm())).unwrap();
    let cut_off_first = world.log.iter().any(|&(_, s, h)| s == catcher && h == Happened::CuttingOff);
    println!("caught by {catcher}, which cut them off first: {cut_off_first}");
    assert!(cut_off_first, "a flanker got to them");
    assert_eq!(world.sentries[catcher].part(), Some(hydroxus_ai::tactics::roles::Part::Pursue), "close by and seeing them, it went for them");
}

#[test]
fn without_radios_each_has_only_its_own_eyes_and_those_out_of_sight_drop_out_of_the_chase() {
    let run = |radios: bool| {
        let start = at(16.0, 20.0);
        let mk = |feet: V| {
            let s = Sentry::new(Role::Sentry);
            (if radios { s.with_radio() } else { s }, feet, facing(feet, start))
        };
        let mut world = World::new(Tuning::DEFAULT, vec![mk(at(4.0, 20.0)), mk(at(22.0, 4.0)), mk(at(40.0, 36.0))], start, 3.6);
        // A wall along the north of their way, all but the far end: the one north of it sees them
        // past its west end at first, and then not.
        world.mesh = yard_with(|x, z| (26..88).contains(&x) && (24..26).contains(&z));
        world.path = vec![at(88.0, 20.0)];
        // How many sentry-seconds each spends on the chase while not seeing the player itself.
        let mut blind_chasing = 0.0;
        let mut phases_differ = false;
        while world.t < 25.0 && world.nearest() > CAUGHT {
            world.step(1.0 / 60.0);
            let alerts: Vec<_> = world.sentries.iter().map(|s| s.alert()).collect();
            phases_differ |= alerts.iter().any(|a| *a != alerts[0]);
            for (s, body) in world.sentries.iter().zip(&world.bodies) {
                let sees = world.mesh.sees_across(body.feet, world.player)
                    && Sight::DEFAULT.visibility(s.alert(), body.feet, body.heading, world.player, Stance::Standing, false) > 0.0;
                if s.alert() == Some(Alert::Alert) && !sees {
                    blind_chasing += 1.0 / 60.0;
                }
            }
        }
        (blind_chasing, phases_differ, world.log.iter().filter(|&&(_, _, h)| h == Happened::LostContact).count())
    };
    let (with, _, lost_with) = run(true);
    let (without, differ, lost_without) = run(false);
    println!("on the chase out of sight: radios {with:.1} s ({lost_with} lost contact), none {without:.1} s ({lost_without})");
    assert!(with > without + 1.0, "with radios, they stay on the chase out of sight");
    assert!(differ, "without radios, they are not all in the same phase at once");
}

/// Two sentries that saw the player in the yard, and lost them - gone, in a moment - searching.
fn two_searching(ai: Tuning) -> World {
    let start = at(45.0, 20.0);
    let sentries = vec![(Sentry::new(Role::Sentry), at(35.0, 20.0), EAST), (Sentry::new(Role::Sentry), at(55.0, 20.0), -EAST)];
    let mut world = World::new(ai, sentries, start, 0.0);
    world.mesh = yard_with(|x, z| (40..50).contains(&x) && (14..16).contains(&z));
    while world.t < 5.0 && !world.sentries.iter().all(|s| s.alert() == Some(Alert::Alert)) {
        world.step(1.0 / 60.0);
    }
    assert!(world.sentries.iter().all(|s| s.alert() == Some(Alert::Alert)));
    world.player = at(500.0, 500.0);
    while world.t < 15.0 && !world.sentries.iter().all(|s| s.alert() == Some(Alert::Evasion) && s.goal().is_some()) {
        world.step(1.0 / 60.0);
    }
    assert!(world.sentries.iter().all(|s| s.alert() == Some(Alert::Evasion) && s.goal().is_some()), "{:?}", world.log);
    world
}

#[test]
fn a_searcher_gives_up_a_spot_another_has_got_to_and_looked_over_for_somewhere_else() {
    let run = |ai: Tuning| {
        let mut world = two_searching(ai);
        let goal = world.sentries[0].goal().unwrap();
        // The other turns up where it was going, and looks about.
        world.bodies[1].feet = goal;
        let mut changed = None;
        for _ in 0..60 {
            world.step(1.0 / 60.0);
            if world.sentries[0].goal().is_some_and(|g| flat(g - goal).norm() > 1.0) {
                changed = world.sentries[0].goal();
                break;
            }
        }
        (goal, changed, world.log.iter().any(|&(_, s, h)| s == 0 && h == Happened::Reassessed))
    };
    let (goal, changed, reassessed) = run(Tuning::DEFAULT);
    let changed = changed.expect("it chose somewhere else");
    assert!(reassessed);
    assert!(flat(changed - goal).norm() >= Tuning::DEFAULT.orchestrate.apart, "{goal:?} -> {changed:?}");
    // Not reassessing, it keeps on to where the other already is.
    let stubborn = Tuning { orchestrate: hydroxus_ai::orchestrate::Tuning { covered_within: 0.0, ..Tuning::DEFAULT.orchestrate }, ..Tuning::DEFAULT };
    let (_, changed, reassessed) = run(stubborn);
    assert!(changed.is_none() && !reassessed);
}

#[test]
fn searching_on_what_it_saw_itself_it_keeps_to_that_over_weaker_word_and_takes_up_surer() {
    let ai = Tuning::DEFAULT;
    let mut world = two_searching(ai);
    let lost = world.sentries[0].last_seen().unwrap();
    // A sighting of somewhere else, fresh, from one of its own it hasn't tested: believed 0.56.
    let word = |world: &World| {
        let now = world.squad.now();
        (Word::seen((), at(80.0, 35.0), now, 1).passed_on(&ai.word, 1), now)
    };
    // Its own sighting is fresher in mind: two accounts, and it goes by the surer - its own.
    let (fresh, now) = word(&world);
    assert!(!world.sentries[0].hear_word(&ai, fresh, now));
    world.step(1.0 / 60.0);
    assert!(world.log.iter().any(|&(_, s, h)| s == 0 && h == Happened::Doubted));
    assert_eq!(world.sentries[0].last_seen(), Some(lost));
    // Half a minute on, what it saw itself is old: the same word, it goes by.
    while world.t < 35.0 {
        world.step(1.0 / 60.0);
    }
    assert_eq!(world.sentries[0].alert(), Some(Alert::Evasion));
    let (fresh, now) = word(&world);
    assert!(world.sentries[0].hear_word(&ai, fresh, now));
    world.step(1.0 / 60.0);
    assert_eq!(world.sentries[0].last_seen(), Some(at(80.0, 35.0)));
}
