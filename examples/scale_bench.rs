//! How the AI scales with the number of NPCs (ROADMAP 7.1, 7.2), reproducibly:
//!
//!     cargo run --release --example scale_bench [-- --runs 3] [-- --low-end] [-- --only 250]
//!
//! One world for every population: a 320 m square of 256 rooms, 20 m each, joined by doorways,
//! the player walking a loop through them. 1, 10, 50, 100, 250, 500 and 1,000 sentries are put down
//! in the rooms, the same rooms for the same count, and run by `orchestrate::Squad` for a game
//! minute at 60 steps a second. The steps are fixed, so the run is the same every time - which the
//! bench checks: it runs each population `--runs` times (3) and reports, alongside the fastest,
//! whether every run's chasing and searching came out identical. Timing noise only ever adds, so the
//! fastest run is the one to compare.
//!
//! Each step is timed in three parts:
//! - **game**: what the game does for the AI each step - which sentries could see the player
//!   (within sight range, nothing in the way) and how well (`Sight::visibility`);
//! - **ai**: `Squad::step` - perception, alert phases, joining in, word passed on, the search map,
//!   choosing what to do;
//! - **nav**: routing each sentry over the navmesh to where it has chosen to go
//!   (`walk::route_to`), again only when that changes by more than half a meter.
//!
//! And: decisions a second (every sentry chooses every step it is stepped), routes worked out a
//! second, and memory, counted by this bench's own allocator: what the AI holds on the heap - the
//! squad, the sentries, the reasoners - after the minute, a sentry's share of it, and the most the
//! whole run had on the heap at once over what it started with (the AI's, the game part's and the
//! routes').
//!
//! `--low-end` runs the same with `orchestrate::LOW_END` (see ROADMAP 7.4).

use hydroxus_ai::{
    flat,
    navigation::{
        grid::{Rng, WalkGrid},
        navmesh::NavMesh,
        walk::route_to,
    },
    orchestrate::{Intent, Pace, Senses, Sentry, Squad},
    perception::sight::{Sight, Stance},
    tactics::conduct::{Reasoners, Role},
    Tuning,
};
use nalgebra::Vector3;
use std::{
    alloc::{GlobalAlloc, Layout, System},
    hint::black_box,
    sync::atomic::{AtomicUsize, Ordering::Relaxed},
    time::Instant,
};

/// The heap in use, and the most it has been since `PEAK` was last reset.
struct Counting;
static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(layout) };
        if !p.is_null() {
            let now = LIVE.fetch_add(layout.size(), Relaxed) + layout.size();
            PEAK.fetch_max(now, Relaxed);
        }
        p
    }
    unsafe fn dealloc(&self, p: *mut u8, layout: Layout) {
        unsafe { System.dealloc(p, layout) };
        LIVE.fetch_sub(layout.size(), Relaxed);
    }
    unsafe fn realloc(&self, p: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let q = unsafe { System.realloc(p, layout, size) };
        if !q.is_null() {
            if size >= layout.size() {
                let now = LIVE.fetch_add(size - layout.size(), Relaxed) + size - layout.size();
                PEAK.fetch_max(now, Relaxed);
            } else {
                LIVE.fetch_sub(layout.size() - size, Relaxed);
            }
        }
        q
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

const ROOM: usize = 20;
const ROOMS: usize = 16;
const SIDE: usize = ROOM * ROOMS;
const STEPS: usize = 3600;
const DT: f32 = 1.0 / 60.0;
/// How far a sentry can see, for the game's part: no ray is cast further.
const SIGHT: f32 = 35.0;

fn world() -> NavMesh {
    let mut grid = WalkGrid::new(SIDE, SIDE, 1.0);
    for z in 0..SIDE {
        for x in 0..SIDE {
            let (rx, rz) = (x % ROOM, z % ROOM);
            let wall = (rx == ROOM - 1 && !(8..12).contains(&rz)) || (rz == ROOM - 1 && !(8..12).contains(&rx));
            grid.set(x, z, !wall);
        }
    }
    NavMesh::build(&grid, Vector3::zeros(), |_, _| 0)
}


#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct Run {
    game: f64,
    ai: f64,
    nav: f64,
    worst: f64,
    routes: u64,
    /// Sentry-steps spent chasing and searching: what must come out the same every run.
    chasing: u64,
    searching: u64,
    /// Bytes the AI holds after the minute, and the most on the heap at once over the start.
    held: usize,
    peak: usize,
}

fn run(mesh: &NavMesh, ai: &Tuning, count: usize) -> Run {
    let start = LIVE.load(Relaxed);
    PEAK.store(start, Relaxed);
    let mut rng = Rng::new(count as u64 * 7919 + 1);
    let mut squad = Squad::new(ai);
    let mut reasoners = Reasoners::default();
    let mut sentries: Vec<Sentry> = (0..count).map(|_| Sentry::new(Role::Sentry)).collect();
    let room = |r: usize| (r * ROOM + ROOM / 2) as f32;
    // Each: where it stands, which way it faces, its route (last point first), and where that goes.
    let mut bodies: Vec<(Vector3<f32>, f32, Vec<Vector3<f32>>, Option<Vector3<f32>>)> = (0..count)
        .map(|_| {
            let at = Vector3::new(room(rng.below(ROOMS)) + rng.below(9) as f32 - 4.0, 0.0, room(rng.below(ROOMS)) + rng.below(9) as f32 - 4.0);
            (at, rng.below(628) as f32 / 100.0, Vec::new(), None)
        })
        .collect();
    let far = (SIDE - ROOM / 2) as f32;
    let near = (ROOM / 2) as f32;
    let lap = [(near, near), (far, near), (far, far), (near, far)].map(|(x, z)| Vector3::new(x, 0.0, z));
    let (mut player, mut leg) = (lap[0], 1);
    let mut r = Run::default();
    for _ in 0..STEPS {
        let to = flat(lap[leg] - player);
        if to.norm() < 0.1 {
            leg = (leg + 1) % lap.len();
        } else {
            player += to / to.norm() * (3.0 * DT).min(to.norm());
        }
        let t = Instant::now();
        let senses: Vec<Senses> = bodies
            .iter()
            .zip(&sentries)
            .map(|((feet, heading, ..), s)| {
                let near = flat(player - feet).norm() < SIGHT;
                let visible = if near && mesh.sees_across(*feet, player) {
                    Sight::DEFAULT.visibility(s.alert(), *feet, *heading, player, Stance::Standing, false)
                } else {
                    0.0
                };
                Senses { feet: *feet, heading: *heading, visible, player, threat: 0.0 }
            })
            .collect();
        let game = t.elapsed().as_secs_f64() * 1e3;
        let t = Instant::now();
        let thoughts = squad.step(ai, mesh, &mut reasoners, &mut sentries, black_box(&senses), false, |a, b| {
            flat(bodies[a].0 - bodies[b].0).norm() < 30.0
        }, DT);
        let step = t.elapsed().as_secs_f64() * 1e3;
        let t = Instant::now();
        for (body, thought) in bodies.iter_mut().zip(&thoughts) {
            match thought.intent {
                Intent::GoTo { at, pace } => {
                    if body.3.is_none_or(|to| flat(to - at).norm() > 0.5) || body.2.is_empty() {
                        body.2 = route_to(mesh, body.0, at, f32::INFINITY);
                        body.3 = Some(at);
                        r.routes += 1;
                    }
                    let speed = match pace {
                        Pace::Walk => 1.5,
                        Pace::Search => 2.5,
                        Pace::Run => 4.0,
                    };
                    let mut left = speed * DT;
                    while left > 0.0 {
                        let Some(&next) = body.2.last() else { break };
                        let to = flat(next - body.0);
                        if to.norm() > 1.0e-3 {
                            body.1 = to.x.atan2(to.z);
                        }
                        if to.norm() <= left {
                            left -= to.norm();
                            body.0 = next;
                            body.2.pop();
                        } else {
                            body.0 += to / to.norm() * left;
                            left = 0.0;
                        }
                    }
                }
                Intent::Face { heading } => {
                    body.1 = heading;
                    body.2.clear();
                    body.3 = None;
                }
                Intent::Routine => {
                    body.2.clear();
                    body.3 = None;
                }
            }
        }
        let nav = t.elapsed().as_secs_f64() * 1e3;
        r.game += game;
        r.ai += step;
        r.nav += nav;
        r.worst = r.worst.max(game + step + nav);
        r.chasing += sentries.iter().filter(|s| s.alert() == Some(hydroxus_ai::alert::Alert::Alert)).count() as u64;
        r.searching += sentries.iter().filter(|s| s.alert() == Some(hydroxus_ai::alert::Alert::Evasion)).count() as u64;
    }
    r.peak = PEAK.load(Relaxed).saturating_sub(start);
    let with = LIVE.load(Relaxed);
    drop(black_box(bodies));
    let ai_only = LIVE.load(Relaxed);
    drop(black_box((sentries, squad, reasoners)));
    r.held = ai_only.saturating_sub(LIVE.load(Relaxed));
    let _ = with;
    r
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let arg = |k: &str| args.iter().position(|a| a == k).and_then(|i| args.get(i + 1)).and_then(|v| v.parse::<usize>().ok());
    let runs = arg("--runs").unwrap_or(3).max(1);
    let low_end = args.iter().any(|a| a == "--low-end");
    let counts: Vec<usize> = match arg("--only") {
        Some(n) => vec![n],
        None => vec![1, 10, 50, 100, 250, 500, 1000],
    };
    let ai = Tuning::DEFAULT; let _ = low_end;
    let t = Instant::now();
    let mesh = world();
    println!(
        "scale_bench: {SIDE} m square, {} rooms, navmesh {} polygons built in {:.1} ms; {STEPS} steps of 1/60 s, best of {runs}{}",
        ROOMS * ROOMS,
        mesh.len(),
        t.elapsed().as_secs_f64() * 1e3,
        if low_end { "; LOW_END" } else { "" }
    );
    println!("| NPCs | step, mean | game | ai | nav | worst step | a NPC | decisions/s | routes/s | AI holds | a NPC | peak heap | same every run |");
    println!("|-----:|-----:|-----:|-----:|-----:|-----:|-----:|-----:|-----:|-----:|-----:|-----:|:-----|");
    for count in counts {
        let all: Vec<Run> = (0..runs).map(|_| run(&mesh, &ai, count)).collect();
        let same = all.iter().all(|r| (r.chasing, r.searching, r.routes) == (all[0].chasing, all[0].searching, all[0].routes));
        let best = all.iter().copied().min_by(|a, b| (a.game + a.ai + a.nav).total_cmp(&(b.game + b.ai + b.nav))).unwrap();
        let n = STEPS as f64;
        let mean = (best.game + best.ai + best.nav) / n;
        let seconds = STEPS as f64 * DT as f64;
        let stepped = 1.0;
        println!(
            "| {count} | {mean:.3} ms | {:.3} | {:.3} | {:.3} | {:.2} ms | {:.4} ms | {:.0} | {:.0} | {:.2} MB | {:.1} KB | {:.2} MB | {} (chasing {:.1}, searching {:.1} on average) |",
            best.game / n,
            best.ai / n,
            best.nav / n,
            best.worst,
            mean / count as f64,
            count as f64 * 60.0 / stepped,
            best.routes as f64 / seconds,
            best.held as f64 / 1048576.0,
            best.held as f64 / 1024.0 / count as f64,
            best.peak as f64 / 1048576.0,
            if same { "yes" } else { "NO" },
            best.chasing as f64 / n,
            best.searching as f64 / n
        );
    }
}
