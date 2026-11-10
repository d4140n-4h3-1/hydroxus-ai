# Module audit (ROADMAP 1.1)

2026-11-10, hydroxus-ai at the commit that adds this file. Each finding gives its evidence and
what was done: **fixed** (in this milestone, tested), **recorded** (left as is, with the reason),
or **recommended** (for a later milestone, named).

The audit covered all 51 source files: `alert`, `perception` (sight, detection, hearing, bolts,
witness), `navigation` (grid, navmesh, walk, reach, route, wander, trip, ride, steer, drive,
traffic, flight), `tactics` (search, investigate, look, join, pace, chase, threat, fire, blind,
cover, team, patrol, utility, conduct), `social` (group, talk, report, crowd, opinion, rumour,
chat, faction, trouble, ties, passing), `think` (lesson, habit, haunt, experience) and `validate`.
Nothing was consolidated just to have fewer files.

## Duplicated responsibilities

| Finding | Evidence | Action |
|---------|----------|--------|
| Looking about one way and then the other existed twice in the crate's reach: inside `investigate` (`Step::Looking`), and in every game that searches (Maze's `LOOK_ABOUT`/`LOOK_SWEEP` in `inhabitants.rs`; the scenario harness had to copy it). | Without it, `SearchMap::best` keeps sending a searcher to the centre of the polygon it already stands in, and it stalls there. The 1.3 scenario harness hit this. | **Fixed.** `tactics::look::LookAbout` is one sweep. `investigate` uses it (its tests unchanged and passing). `LookAbout::searching` takes the new `search.look_about` (3 s) and `search.look_sweep` (70°), Maze's values. Maze and the scenario harness now use it instead of their copies. |
| `drive.rs` had a private `wrap` identical to the public `patrol::wrap`. | Same body, both `(a + π).rem_euclid(τ) − π`. | **Fixed.** `crate::wrap` at the root; `patrol::wrap` re-exports it, so existing callers keep working. |
| `grid.level` and `walk.level` describe the same thing: how far above or below a floor can be and still be at a height. | Same doc comment and same default, 1.0. Nothing keeps them equal. | **Recorded.** Making one follow the other would change how a game that sets them apart behaves. Recommended for 10.1: one field, the other deprecated. |
| `ride` and `trip` both have `boarding_pace` and `time_to_spare`. | Same names and defaults. | **Recorded, fine:** a train's doors and a ferry are different vehicles, and a game may want them to differ. |
| `fire.engage`/`fire_range` and `patrol.engage`/`fire_range`. | Same names. | **Fine:** droids on foot and drones. `validate` checks each pair's order. |

## Missing interfaces

| Finding | Action |
|---------|--------|
| A searcher's look-about on arrival (above). | **Fixed:** `LookAbout`. |
| An NPC's alert phase is three loose values a game must step together: `Alert`, the seconds left, and `Wariness::lasts` on every change of phase. Maze and the scenario harness both write the same bookkeeping. | **Recommended for 2.1**, as part of the orchestration layer, not bolted onto `alert` now. Done in 2.1: `orchestrate::Sentry`. |
| The search map is looked over on a clock the game keeps (Maze's `LOOK_EVERY`, the harness's). | **Recommended for 2.1**, with the above. Done in 2.1: `orchestrate::Squad`. |
| Per-NPC state could not be saved. `think` and `social` derive `serde`, but `Detection`, `Wariness`, `Alert`, `Investigation`, `SearchMap`, `Witness`, `Unease`, `Mourning` and `BlindFire` did not, so a game could not save a guard mid-search. | **Fixed:** `serde` derives behind the existing feature. `tests/persistence.rs` saves a guard 1.5 s into an incident, loads it, and runs original and copy side by side for 1,800 frames: exactly equal, the search maps within 1e-5. Their chances are summed in hash-map order, which a loaded map does not share. |

## Ownership and borrowing

No global or shared mutable state; no `unsafe`, `Rc`, `RefCell` or `Mutex`. Every piece of NPC
state is a plain value the game owns and passes in by `&mut`, and what the crate reads from
the world comes in as arguments or closures: rays as `Fn` closures, the train as `impl Train`, a
drone's surroundings as `dyn Ways`. The one thread use is a scoped worker pool in
`WalkGrid`'s linking, which re-raises a worker's panic. **Nothing to change.**

## Allocations in per-frame calls

Every `pub fn step`/`think`/`update`/`choose`/`look`/`spread`/`sync`/`lean` was scanned for
allocating constructs.

| Where | What | Action |
|-------|------|--------|
| `trip::Trip::step` | `Doing::Go(route.to_vec())` copies the NPC's whole route back every frame while it walks to a dock. | **Recorded.** Avoiding it changes `Doing`'s public shape (it would say "keep your route"). Recommended for 7.3, measured first. |
| `ride::Ride::step` | A new `Vec` of one or two points each frame for a droid going to or on a train. | **Recorded:** a few dozen riders at most; 7.3 with the above. |
| `traffic::Traffic::step` | Two vectors a frame for the whole city (bikes' places; a per-junction list). | **Recorded:** once per frame, not per NPC; `ai_bench` puts the whole traffic step at 0.05 ms. |
| `Lessons::step`, `Experience::step` | Return a `Vec` of views given up. | **Fine:** an empty `Vec` does not allocate. |
| `SearchMap::best`, `Heard::at` | A cost map per call. | **Fine:** once per decision or per noise, not per frame. |

## Error handling

The crate panics only on broken invariants. Four `unwrap`/`expect`s outside tests each follow a
`push` or a check on the line before, and one re-raises a worker thread's panic. Bad input,
NaN times, NaN outcomes and NaN positions are clamped or ignored, with tests in `think`,
`look`, `detection` and the scenarios. `Tuning::validate` (1.2) is the one `Result`, for settings.

| Finding | Action |
|---------|--------|
| `WalkGrid::set`, `is_walkable`, `set_floor` and `floor` indexed `z * width + x` unchecked. An `x` past the edge quietly read or wrote a cell on the next row, and panicked only past the last cell. | **Fixed:** an `x` off the grid panics, naming the cell and the grid's size. The panic is out of line (`#[cold]`). Measured in `examples/grid_bench.rs` (see BENCHMARKS.md): survey and navmesh build unchanged. `walkable_cells` got faster (0.7 → 0.4 ms on 2.9 M cells), now reading the cells directly. 2.9 M lookups from outside cost 7.6 ms rather than 1.2 ms (about 2 ns each), at survey time only. Kept: correctness before a loop no game runs per frame. |

## Configuration consistency

Every module's tuning is made by the one `tuning!` macro, except `Sight`, which is written by
hand and given the same treatment: `serde(default, deny_unknown_fields)`, and checked by
`validate` (1.2). Defaults are the module's documented `pub const`s. **Nothing to change.**
The two `level` fields are above.

## Public API stability

| Finding | Action |
|---------|--------|
| 62 public structs with public fields, 0 `#[non_exhaustive]` anywhere. A new variant in an enum a game matches on (`Conduct`, `Doing`, `Telling`, `Event`, `Change`) breaks an exhaustive `match`. A new tuning field breaks a struct literal that does not end in `..Tuning::DEFAULT`; this milestone's `search.look_about` and `look_sweep` are such a change. | **Recommended for 10.1:** decide per type before 1.0, and say in the handbook to build tunings from `DEFAULT`. Maze does: one constant, `Tuning::DEFAULT`. |
| `patrol::wrap` moved to the root. | **Kept compatible** by re-export. |

## Tests added by this audit

`tactics::look` (4), `navigation::grid` (2: off the grid refused; every cell its own, two
storeys), `tests/persistence.rs` (1), and `examples/grid_bench.rs` as a benchmark.
