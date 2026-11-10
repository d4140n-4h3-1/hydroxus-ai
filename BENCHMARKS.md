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

## Not yet measured

Everything ROADMAP 7.2 asks for: populations of 1 to 1,000 NPCs through the full perceive-decide
loop, memory use, and a low-end configuration.
