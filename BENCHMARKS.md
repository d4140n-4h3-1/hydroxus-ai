# Benchmarks

Measured results only, with the machine and the conditions they were taken under. Each table
says which commit it was measured at. Re-run them before and after any change meant to make
something faster (ROADMAP rule 19).

## Machine

| | |
|-|-|
| CPU | AMD Ryzen 9 5900X, 12 cores / 24 threads, up to 4.95 GHz |
| Memory | 62 GiB |
| GPU | AMD Radeon RX 7600 (Navi 33), for the game's runs only |
| OS | Manjaro Linux, kernel 7.2.3, Wayland |
| Rust | 1.94.1, `--release` |

## `examples/ai_bench.rs` (2026-11-10, hydroxus-ai 946f697)

`cargo run --release --example ai_bench`, single-threaded, three runs back to back.

| What | Run 1 | Run 2 | Run 3 |
|------|-------|-------|-------|
| Traffic: 441 junctions, 64 bikes, 200 on foot; network built | 0.22 ms | 0.21 ms | 0.22 ms |
| Traffic: a step, mean (worst) | 0.0527 ms (0.104) | 0.0537 ms (0.107) | 0.0529 ms (0.109) |
| Patrol: 50 drones thinking and flying, a frame, mean (worst) | 0.0027 ms (0.014) | 0.0026 ms (0.013) | 0.0027 ms (0.009) |
| Flight: 2,000 routes through a block of 40 buildings | 1.97 ms | 2.04 ms | 2.12 ms |

## `examples/navmesh_bench.rs`

Not run. It reads a grid the game dumped to a file, and there is no such file to hand; the game
no longer has the code that dumped them. The navmesh figures in the README (Nexus: 7,619
polygons, a route in 0.32 ms) are from 2026-11-03 and have not been re-measured.

## Maze's team battle (2026-11-10, game 2ae9336, hydroxus-ai 1d39a5f)

In the game's repository: `MAZE_WINDOWED=1 BATTLE_BENCH=240 BATTLE_SEED=1 cargo run --release
--example team_battle` (or `team_battle_forest`). Ten droids, five a side, fight for 240 s of
play with vsync off; every frame's parts are timed. "think" is the AI (perception, team tactics,
choosing); "move" steers along routes; "show" is animation and posing; "bolts" is shots in flight.
The frame rate is held near 60 by the Wayland compositor despite vsync off. The worst whole frame
is the first, which loads the level.

**Arena** (12,891 frames, 54 fps; 14 shot down, 1,681 bolts, 2,855 m walked, a target in 75% of frames), ms per frame:

| Part | mean | p50 | p95 | p99 | max |
|------|------|-----|-----|-----|-----|
| think | 0.049 | 0.013 | 0.088 | 1.750 | 2.561 |
| move | 0.002 | 0.002 | 0.002 | 0.004 | 0.015 |
| show | 0.288 | 0.277 | 0.443 | 0.566 | 1.077 |
| bolts | 0.010 | 0.003 | 0.064 | 0.096 | 0.395 |
| whole frame | 18.594 | 16.723 | 18.646 | 32.765 | (load) |

**Forest** (13,521 frames, 56 fps; 3 shot down, 772 bolts, 3,021 m walked, a target in 51% of frames), ms per frame:

| Part | mean | p50 | p95 | p99 | max |
|------|------|-----|-----|-----|-----|
| think | 0.071 | 0.011 | 0.290 | 1.978 | 4.948 |
| move | 0.002 | 0.002 | 0.003 | 0.003 | 0.010 |
| show | 0.242 | 0.242 | 0.388 | 0.490 | 0.692 |
| bolts | 0.006 | 0.001 | 0.056 | 0.097 | 0.363 |
| whole frame | 17.727 | 16.735 | 18.110 | 32.753 | (load) |

Against the first runs on 2026-11-08 (same seed and length): arena think 0.052 ms mean, 2.5 ms
worst; forest 0.079 ms, 5.5 ms. The same within run-to-run noise.

One of the two arena runs on 2026-11-10 stopped before writing its report, with no error or panic in
its log; its exit status was not captured. The re-run completed normally (exit 0, 241 s). The
cause is not known.

## After milestone 1.2 (2026-11-10, hydroxus-ai 0b0b98f, game 5c7b530)

The first battle runs with the `Detection` frame-rate fix (946f697). `Tuning::validate` and
the refusal of unknown field names run only when a game asks for them or reads a file, so neither
touches a frame. Both battles exited 0 after 240 s.

`ai_bench`, three runs:

| What | Run 1 | Run 2 | Run 3 |
|------|-------|-------|-------|
| Traffic: network built | 0.22 ms | 0.23 ms | 0.22 ms |
| Traffic: a step, mean (worst) | 0.0519 ms (0.233) | 0.0519 ms (0.087) | 0.0515 ms (0.082) |
| Patrol: 50 drones, a frame, mean (worst) | 0.0028 ms (0.028) | 0.0025 ms (0.008) | 0.0025 ms (0.008) |
| Flight: 2,000 routes | 2.00 ms | 1.97 ms | 1.99 ms |

Run 1's worst traffic step (0.233 ms) is a one-off; the other two runs and every earlier run stay
near 0.1 ms.

Team battle, seed 1, 240 s, ms per frame (mean / p50 / p95 / p99 / max):

| Part | Arena | Forest |
|------|-------|--------|
| think | 0.052 / 0.012 / 0.101 / 1.894 / 2.952 | 0.063 / 0.007 / 0.241 / 1.981 / 5.050 |
| move | 0.002 / 0.002 / 0.003 / 0.003 / 0.016 | 0.002 / 0.002 / 0.002 / 0.003 / 0.011 |
| show | 0.280 / 0.263 / 0.466 / 0.563 / 1.652 | 0.233 / 0.242 / 0.327 / 0.406 / 0.773 |
| bolts | 0.011 / 0.003 / 0.063 / 0.103 / 0.327 | 0.005 / 0.001 / 0.047 / 0.091 / 0.418 |
| whole frame, p50 / p95 / p99 | 16.728 / 18.418 / 32.688 | 16.727 / 19.581 / 32.716 |
| Frames (fps) | 12,795 (53) | 11,912 (50) |
| Shot down, bolts, walked, frames with a target | 15, 1,999, 2,722 m, 85% | 1, 623, 2,178 m, 59% |

The AI ("think") is within run-to-run noise of the 2026-11-08 and earlier 2026-11-10 runs:
0.049–0.052 ms mean in the arena, 0.063–0.079 ms in the forest.

**Limitation: the same seed is not the same battle.** `BATTLE_SEED` fixes the random choices,
but each frame's length comes from the real clock, so the battles drift apart. The arena went
9–5 with 14 shot down in one run and 10–5 with 15 in the next; the forest 1–2 with 3, then 1–0
with 1. Fewer fights mean less for the AI to think about, so differences of 10–20% in "think"
between runs are expected without any change to the code. A fixed-step mode for the benchmark
would make runs comparable frame for frame (ROADMAP 7.1).

## `examples/grid_bench.rs` (2026-11-10, milestone 1.1)

`cargo run --release --example grid_bench`: a 1,200 by 1,200 cell grid, two storeys (2.9 M cells,
rooms and doorways), set cell by cell, read back, and built into a navmesh of 23,318 polygons;
mean of 5 runs. "Before" is 9a561b0, built in a separate worktree with the same benchmark copied in.
Re-run whenever `WalkGrid` or `NavMesh::build` changes.

| Part | Before | After |
|------|--------|-------|
| Survey: `set` + `set_floor` every cell | 18.1 ms | 18.3 ms |
| `walkable_cells` | 0.7 ms | 0.4 ms |
| `is_walkable` + `floor`, every cell, from outside | 1.2 ms | 7.6 ms |
| `NavMesh::build` | 56.3 ms | 58.3 ms (earlier runs of both: 56–59 ms) |

The bounds check on a cell's `x` (an `x` past the edge used to land silently on the next row)
stops the compiler from vectorizing a tight loop of lookups from outside the crate: about 2 ns
each. The first attempt put the `assert!` inline, which also slowed the survey (to 20.2 ms) and
the lookups (to 15 ms). Moving the panic out of line (`#[cold]`) and having `walkable_cells` read
the cells directly brought those back. Lookups happen only while surveying a level at load; the
navmesh is the only ground at runtime.

## Not yet measured

Everything ROADMAP 7.2 asks for: populations of 1 to 1,000 NPCs through the full perceive-decide
loop, memory use, and a low-end configuration.
