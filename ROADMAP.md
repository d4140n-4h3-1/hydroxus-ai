# Hydroxus AI — Master Development Roadmap

## Status

Updated as milestones finish. **Done** means implemented, tested and the tests run; **Partial**
says what is there and what is not; **Not started** means nothing for it yet beyond what the
crate already did before the roadmap.

| Item | Status | Evidence / what is left |
|------|--------|-------------------------|
| 1.1 Module audit | **Done** (2026-11-10) | Written up in `docs/audit.md`. Fixed: the search look-about missing from the library (`tactics::look::LookAbout`, which `investigate` now uses too; Maze switched to it); `wrap` duplicated in `drive`; `WalkGrid` reading or writing the wrong cell for an `x` past the edge (now refused, measured with the new `examples/grid_bench.rs`); per-NPC state that couldn't be saved (serde derives, `tests/persistence.rs`). Recorded or recommended, with reasons: `grid.level` and `walk.level` overlap; per-frame route copies in `trip` and `ride` (7.3); an alert-phase bundle and search clock (2.1); `#[non_exhaustive]` and building tunings from `DEFAULT` (10.1). |
| 1.2 Configuration validation | **Done** (2026-11-10) | `Tuning::validate()` (`src/validate.rs`) returns every problem, one per field, as `field is value, but must be …`. It checks that every number is finite and not negative unless meant to be; shares, angles, divisors, counts and random ranges; and contradictions between fields. With `serde`, unknown field names are refused (`deny_unknown_fields`), so a misspelling can't pass silently for a missing setting. Tests: 9 unit tests, a doc test, and `tests/validate_files.rs`, which sets each of the 244 decimal settings in a serialized default to −5 in turn and checks that exactly that field is reported. Decision: rules live in one place rather than in each module, and a module's tuning on its own is checked inside a whole one (`Tuning { search: mine, ..DEFAULT }`). Not covered: orderings whose intent I couldn't confirm from the code (`pace`, `patrol.arrived`/`reached`, `grid.max_climb` against `level`). |
| 1.3 Integration testing | **Done** (2026-11-10) | `tests/scenarios.rs`, 10 tests, through the public API only, on one corridor-and-room level with a test-only guard that runs the handbook's per-frame loop. The tests cover: an unfamiliar sound noticed and investigated, then back to the beat; an intruder spotted and chased, contact lost, and the last known place searched; one hiding found again by the search; a witness telling the nearest sentry, which searches there while the other knows nothing; a sentry joining on seeing another give chase while one facing away does not; and back to routine after an incident, with wariness fading. A full incident gives the same events in the same order at 30, 60 and 144 Hz and at uneven and hitching frame rates, within 0.25 s. Fading state is the same in one 600 s step as in 6,000 short ones, and runs are deterministic. This found and fixed a frame-rate bug: `Detection` drained for a whole long step once past `forget_after`, rather than only the part past it. |
| 1.4 Compilation and CI | **Done** but for CI (2026-11-10) | Clippy has 0 warnings with and without features. `cargo test` passes 159 tests with all features and 157 without. Docs build with warnings as errors. MSRV is 1.89 (nalgebra 0.35), set as `rust-version` and checked with `cargo +1.89 test --all-features`. The CI workflow (stable and 1.89, both feature sets, plus clippy and docs) is written but not pushed yet: the push token lacks GitHub's `workflow` scope. There are no doc tests, because the handbook's code is not compiled. |
| 2.1 Orchestration layer | **Done** (2026-11-10) | `src/orchestrate.rs`: `Sentry` (one NPC's mind) and `Squad` (the shared search map and its clock, joining in, searchers splitting up). Each frame, `Senses` go in and an `Intent` (`Routine` / `GoTo{at, pace}` / `Face`) comes out, with `Happened` events. It covers routine and beat patrol, observation, investigation, suspicion (Caution), threat response (chase), search, group coordination, recovery and return to routine. Optional: nothing else in the crate names it, and a test checks that. `tests/orchestrated.rs` has 12 tests (the roadmap scenarios through the layer, searchers splitting up, `no_way`, beat resumed, frame-rate independence, nonsense input, independence). A squad saved and loaded mid-search runs identically (`tests/persistence.rs`). `examples/guard.rs` is a runnable demo. Cost: `ai_bench`, `BENCHMARKS.md`. Found and fixed while building it: a sentry that joined a chase it could see but couldn't see the player itself flipped between Alert and Evasion every frame; it now stays on the chase while it sees it, as Maze does. Maze itself doesn't use the layer; it keeps its own wiring. |
| 2.2 Decision prioritisation | **Done** (2026-11-10) | `tactics::priority`: urgency, threat, confidence in the sighting and in a lead (that is, the information available), commitment, and traits (aggression, caution, curiosity). Each is a bounded multiplier per conduct on top of the conduct reasoner's fixed order and its learned experience (`Experience::choose_with`). Each factor is exactly 1 with nothing to go on, so Maze, which doesn't pass any, is unchanged. `factors()` explains a choice. Oscillation: inertia plus commitment; a test with input wavering around a tie gets at most 1 change in 600 frames. In `orchestrate`, each sentry builds its context every frame (urgency from its phase and threat; sighting and lead confidence fading with time; how long it has been doing what it does). A noise while searching becomes a lead to weigh against the search; a lead passed over until cold is let go. Tests: 7 unit tests, 3 orchestration tests (fresh search kept over a faint noise and cold one left for a clear lead; the curious follow a lead where the incurious search; context explains). |
| 2.3 Behavioural inertia | **Done** (2026-11-10) | In `orchestrate`, configurable through `orchestrate::Tuning` (`hold`, `finish_within`, `switch_lead_by`, `attentive_for`, `attentive_keenness`, `linger`). Completing an investigation: within 4 m of its spot it is held ×2 and a new lead waits. Maintaining a search: a spot being looked about is finished first. Finishing a group assignment: the squad's search spot is that assignment. Remaining attentive to a disturbance: after a look that found nothing or a noise too faint to investigate, the sentry looks that way, then watches and listens 1.5× as keenly for 8 s. A new lead takes over only if 0.25 surer; otherwise it queues and is taken up later unless cold. A sighting still overrides everything. Tests (6): finish first then the second noise; surer lead takes over, a lesser one waits; faint noise means attentive; attentive notices sooner; a storm of 20 noises in 12 s leaves no look abandoned near its spot and at least 3 completed; switched off, it behaves as before. The first three fail with the hold settings at 0. Two design flaws found and fixed while testing: holding the walk to a search spot left searchers never free on a small level, and the look-about used when nowhere is likelier held it permanently. |
| 3.1 Persistent memory | **Done** (2026-11-10) | `think::memory`: one `Record` per thing remembered, for all eight of the roadmap's kinds (locations, characters, encounters, observed events, dangers, group relationships, and investigations with their outcome, success or failure). Noting the same thing again reinforces the existing record. Retention is configurable: a half-life per kind, a cap per kind (the faintest go first), and a forgetting floor. Fading is identical in one long step or many short ones (strength is recomputed from when it was noted). Records can be told on secondhand, recalled strongest first by kind, place or who, and saved with `serde`. The existing aggregates (`haunt`, `habit`, `rumour`, `faction`, `lesson`) stay as they were. In `orchestrate`, sentries record what happens and act on it: a spot checked lately is a weaker lead (a glance, not a walk, until the memory fades), and a place where it came to harm raises its threat there. Tests: 10 unit, 3 orchestration, plus memory kept through save/load. |
| 3.2 Experience tracking | **Done** (2026-11-10) | `think::experience`: decisions (trail), outcomes, successes/failures, habits; measurable effect on choice (tests). |
| 3.3 Memory retrieval | **Done** (2026-11-10) | In `think::memory`. Relevance scoring and contextual retrieval: `Cue` (kinds, place and radius, who, what) and `recall_for` give the best few, scored by strength, closeness, who and what matches and importance; 0.5 µs on a full 224-record memory, so no index is needed (`ai_bench`). Event importance slows decay up to 4×. Consolidation merges faint records of the same thing nearby into one stronger one, and a full kind consolidates before it forgets. In `orchestrate`, a searcher recalls where it found or lost the player before near the loss and weighs its choice of search spot by that (`SearchMap::best_by`). Found and fixed: leaning a search map right after `lose` (`Haunts::lean`, as documented and as Maze uses it) has no effect beyond the loss polygon; there's now `best_by` plus `Haunts::weigh`. Also added `tell_from(at, ago)` for reports with an age (part of 5.2). Tests: 4 unit, 1 integration (a haunt 24 m off unaffected by lean, chosen by weighing), 1 orchestration (on a mirrored two-room level the guard enters the room it remembers, either way round). |
| 3.4 Individual differences | Partial | `nerve` per NPC in `conduct`; per-NPC `Experience`; `priority::Traits` (aggression, caution, curiosity) weighing choices, with a test of two sentries behaving measurably differently in the same situation (2.2). Not yet: observant, social, independent; traits shaping perception and talk. |
| 4.1 Behavioural adaptation | **Done** (library) | `think::lesson`, `experience`: unsuccessful conduct weighed down, successful up. Not yet wired into Maze's droids. |
| 4.2 Learning evaluation | **Done** (2026-11-10) | `think::evaluate`: per-approach tallies (outcome, attempts, confidence interval); changes of habit (previous and new behaviour, when, outcomes under each); Welch's-test judgement (better, worse, no clear difference) so that random changes aren't rewarded; stability as changes per 100 attempts. `tests/learning.rs`: across 5 seeded worlds where searching beats looking (70% against 30%), 20 learners clearly beat 20 controls with learning off, by 0.28–0.39 success, with 15–19 of 20 ending on searching and 1.1–1.7 habit changes per 100 encounters. In a null world, 684 changes of habit were judged better only 0.9% of the time, and learners did no better late than early. `tests/orchestrated.rs`: over 8 repeated searches, a guard that keeps its memory finds a player who always hides in the same room in 9.5 s after the first time (24.6 s), while a fresh guard takes 24.6 s every time. `examples/learning.rs` prints the curves. **Measured weakness for 4.3:** learners relapse now and then. Once a learner stops investigating, its record of investigating fades back toward neutral while searching's own failures count against it, so the worst learner in a world still searched only 51–63% of the time, against 83–85% for the group. |
| 4.3 Controlled learning | **Done** (2026-11-10) | Rate-limited, bounded −1..1, hysteresis on views, fading, bounded memory, deterministic (2026-11-08). Then, from 4.2's measurements, **settling**: a lesson's evidence slows later outcomes (full rate for 4, then ÷(1 + 0.1 per extra), floor 0.05), so a run of bad luck doesn't overturn what is well learned (relapse); evidence fades, so it relearns after a while. Plus deterministic **exploration**: a well-known lesson is weighed slightly below a less-tried one (≤ 0.3 of sway), without which settling locked a learner unlucky with the better approach onto the worse one (1 in 20 in one world searched once in 150). Measured on the same five worlds: 18–20 of 20 end on what works (was 15–19); habit changes 0.4–1.0 per 100 (was 1.1–1.7); everything searched 0.88–0.92 (was 0.83–0.85); the worst learner 0.64–0.81 (was 0.51–0.63). In the null world, habit changes 422 (was 684), false credit 1.9%. When the world swaps halfway, learners come round in a median of 11–13 encounters and end at 0.67–0.71 (of 0.7). The evaluator also gained a pooled-variance floor: five lucky outcomes alike had made Welch's test certain. Per-frame cost is unchanged side by side (250 sentries 0.158–0.177 ms against 0.159–0.178 ms), after making `bias` a single lookup (the first version cost about 10%). |
| 4.4 Frozen ML | Not started | |
| 5.1 Social awareness | **Done** (2026-11-11) | Reviewed against the list. Already there: trust in sources (`word::Trust`, 5.2), reputation (each faction's standing with the player, `faction`, outlasting its NPCs), faction awareness (`Factions::relation`: how each takes harm done to another's), witness information (`report`, `word`), rumours (`rumour`, `trouble`), shared knowledge (`word::Heard`, rumours and trouble told on). Missing, now added: **suspicion** (`social::suspicion`) - suspecting the player of trouble no one saw them do from one's own sightings of them near it about then, never certainty (at most 0.8), told on weaker at the hearer's trust in the teller, cleared by an alibi (seen far off at the time, by itself or by word it gives credence to - conflicting observations weighed by belief), fading, and making the NPC warier without changing its regard; and **relationships between NPCs** (`social::relation`) - affinity from their factions plus time together, help and harm, easing back, not necessarily mutual, deciding whether one shares news with another and how far it trusts that one's untested word (`Trust::introduce`). Also `Word::credence`: belief in what word says of *then*, not fading with age, for alibis. Tests: 7 suspicion, 4 relation, 2 word unit tests; `tests/social.rs` (3): a suspicion spreading through a town of four factions along who gets on with whom, kin believed sooner than strangers, rivals kept in the dark, an alibi by radio clearing it for the guard only while the neighbour suspects still; harm keeping a friend in the dark for hours; all of it saved and loaded mid-way carrying on the same. Not done: `orchestrate` doesn't use either (its sentries are all one faction, and suspicion is for townsfolk); Maze doesn't use them yet. |
| 5.3 Group coordination | **Done** (2026-11-11) | Reviewed against the list. Already there: shared objectives (the squad's hunt and search map), task distribution among searchers (spots `apart`), group awareness (joining a chase it sees, word by radio or voice), formation when walking together (`social::group`), team tactics in a fight (`tactics::team`), and not every NPC receiving the same information (word reaches only those in range with the channel; each its own trust and memory). Missing, now added: **group roles in a chase** (`tactics::roles`: the nearest few pursue, the rest cut the player off ahead, each to its own side, with hysteresis and aggression; wired into `orchestrate` as `Sentry::part()` and `Happened::CuttingOff`), and **staying on the chase by radio** (a chaser with a radio stays Alert while another in radio range sees the player; without one, only its own eyes). Measured (`tests/group.rs`, 27 runs over flank positions and 30/60/144 Hz): never later to the player than all pursuing, sooner in 21 of 27, by 1.6–4.7 s with flankers ahead (0.2–0.5 s level or behind), the same in the 6 where flankers end up trailing whatever their part; 3 places made for at once against 1. Radios: a walled-off flanker stays on the chase out of sight 4.1 s with, 0.8 s and loses contact without. Cost: see BENCHMARKS (no cost with parts off; +0.03 ms a frame at 250 from cutters without radios dropping into searching, and the worst frame down from about 4.3 to 0.8 ms). Not done: formations while patrolling in `orchestrate`; parts beyond pursue and cut off (holding an exit, say). |
| 5.4 Independent behaviour | **Done** (2026-11-11) | Recognizing changing circumstances: the phases, plus a cutter that sees the player within `sees_close` goes for them whatever its part (test: the flanker that gets to them had cut them off first and was pursuing at the catch). Reporting relevant information: sightings passed on (5.2). Reassessing assignments: a searcher gives up a spot another searcher has got to (`covered_within`, `Happened::Reassessed`; test, and not with it switched off). Responding to conflicting observations: searching on its own sighting, it doubts word of elsewhere unless clearly surer (test: the same fresh word doubted at first, acted on half a minute on); alibis for suspicion (5.1). Acceptance: coordinated without copies, as measured in 5.3. Found and fixed while measuring: the first version built every sentry a list of all the searchers every frame, quadratic (+0.035 ms at 250); now only searchers do. |
| 5.2 Information exchange | **Done** (2026-11-10) | `social::word`: firsthand or secondhand (hops, witness, last teller), age (when seen, staleness), confidence (less each retelling), source reliability (`Trust`, learned from words proving right or wrong), and channels with ranges (voice where it carries, radio both having one, alarm). `Heard` keeps the best word per thing, its own eyes first. In `orchestrate`: radios, sightings passed on at once by radio or voice, belief deciding whether to act, `Happened::Doubted`, trust confirmed or refuted by how the search goes, `hear_word` for outside sources. Tests: 5 unit, 4 orchestration (radio far, none without, voice near and unseen; stale doubted and fresh acted on; a source that misleads twice is doubted, another still believed; its own sighting before a told one). Found and fixed while testing, both in the layer's search: an emptied search map left searchers standing about (it now re-seeds from where the player was lost, as far as they could have gone), and a searcher at a hub polygon kept choosing the spot it stood on (best counts neighbours' chance), never going into the rooms beyond (it now excludes its own spot). A test fails without either fix. Maze had the hub deadlock too (it picked search spots with the same `best`, not excluding a droid's own spot); fixed there in Maze da5c5ad, along with word between its droids. |
| 6.1–6.3 Navigation | Partial | Navmesh with storeys, doors, ferries, trains; `examples/navmesh_bench.rs`. No scaling suite. |
| 7.1–7.4 Performance | Partial | `BENCHMARKS.md` (2026-11-10) records hardware and results for `ai_bench` and Maze's team battle (`BATTLE_BENCH`, arena and forest). `navmesh_bench` was not run: it needs a dumped grid, and none exists. Re-measured after 1.2: no change. The battle benchmark isn't reproducible from one run to the next, because frame lengths come from the real clock; a fixed-step mode is needed. No scaling from 1 to 1,000 NPCs, no memory measurements, no low-end profile. |
| 8 Demonstrations | Partial | B, adaptive behaviour: `examples/learning.rs`. A minimal guard: `examples/guard.rs`. The rest are not started. Maze is the only full integration. |
| 9 Documentation | Partial | `HANDBOOK.md` (single file), rustdoc on every public item. No `docs/handbook/` chapters, no CHANGELOG. |
| 10 Release | Not started | |

---

## Project Objective

Hydroxus AI is an engine-independent, modular NPC intelligence library written in Rust.

Its purpose is to provide game developers with autonomous, adaptive, explainable NPC behavior
without requiring expensive large language models or excessive computational resources.

The system should prioritize:

- Deterministic decision-making where appropriate.
- Persistent memory and experience.
- Learning from previous encounters.
- Adaptive behavior.
- Individual NPC personalities and behavioral differences.
- Cooperative group intelligence.
- Efficient CPU and memory utilization.
- Engine independence.
- Developer accessibility.

The game engine remains responsible for world execution, physics, animation, and rendering.
Hydroxus AI provides the intelligence.

IMPORTANT: Preserve the existing architecture and functioning implementations. Do not rewrite
working modules merely to improve aesthetics. Prefer incremental improvements, clear interfaces,
and tests.

---

# PHASE 1 — EXISTING ARCHITECTURE STABILIZATION

Status: PRIORITY

Objective: Ensure that the existing modules are reliable and can operate together without
introducing regressions.

## 1.1 Module Audit

Review the existing modules: Perception, Sight, Hearing, Navigation, Grid, Route, Steering,
Tactics, Threat assessment, Alert systems, Search, Group behavior, Social interactions, Rumors,
Pacing, Trip detection, Learning and adaptability.

Tasks:

- Identify duplicated responsibilities.
- Identify missing interfaces.
- Check ownership and borrowing patterns.
- Identify unnecessary allocations.
- Check error handling.
- Verify configuration consistency.
- Review public API stability.

Do not consolidate modules simply for fewer files.

## 1.2 Configuration Validation

Implement or improve validation for:

- Invalid numerical values.
- Negative distances.
- Invalid probabilities.
- Out-of-range behavioral parameters.
- Missing required configuration.
- Contradictory settings.

Provide useful error messages.

## 1.3 Integration Testing

Create tests covering interactions between modules.

Required scenarios:

- NPC detects an unfamiliar sound.
- NPC investigates a suspected disturbance.
- NPC detects an intruder.
- NPC loses visual contact.
- NPC searches the last known location.
- NPC communicates relevant information.
- NPC responds to another NPC's observations.
- NPC returns to routine after an incident.

Include frame-rate and elapsed-time independence tests.

## 1.4 Compilation and CI

- Verify the complete Cargo workspace.
- Run all unit tests.
- Run integration tests.
- Run documentation tests.
- Check Clippy warnings.
- Establish CI.
- Document the supported Rust version.

Acceptance criteria: clean compilation; all tests passing; no unexplained warnings; reproducible
test results.

---

# PHASE 2 — INTELLIGENCE ORCHESTRATION

Status: HIGH PRIORITY

Objective: Allow the individual modules to function as a coherent intelligence system.

## 2.1 Optional Behavior Orchestration

Develop an optional orchestration layer. It should coordinate: routine behavior, patrol,
observation, investigation, suspicion, threat response, search, group coordination, recovery,
return to routine.

The orchestration layer must not make the underlying modules dependent on one another.
Developers should retain the ability to use individual modules without adopting the orchestrator.

## 2.2 Decision Prioritization

Implement consistent decision prioritization. Consider: urgency, threat level, confidence,
available information, existing commitments, behavioral personality, previous experience.

Prevent unnecessary oscillation between decisions.

## 2.3 Behavioral Inertia

NPCs should not abandon an activity every time a minor new observation occurs. Implement
configurable behavioral persistence. Examples: completing an investigation, maintaining a search,
remaining attentive to a disturbance, finishing a group assignment.

Acceptance criteria: NPC behavior should remain coherent across multiple simultaneous events.

---

# PHASE 3 — MEMORY AND EXPERIENCE

Status: CORE INTELLIGENCE DEVELOPMENT

Objective: Allow NPCs to retain and use information from previous experiences.

## 3.1 Persistent Memory

Implement memory records for: locations, characters, previous encounters, observed events, known
dangers, group relationships, successful investigations, failed investigations.

Memory should support configurable retention and expiration.

## 3.2 Experience Tracking

Allow NPCs to accumulate experience through simulated encounters. Track: decisions made,
outcomes, successes, failures, repeated observations, behavioral patterns.

Experience must have measurable consequences.

## 3.3 Memory Retrieval

Develop contextual memory retrieval. An NPC should be able to retrieve relevant information
rather than repeatedly processing every historical event. Investigate: relevance scoring, memory
decay, event importance, contextual retrieval, memory consolidation.

## 3.4 Individual Differences

NPCs should not necessarily react identically to identical events. Support configurable
behavioral characteristics. Examples: cautious, curious, aggressive, observant, social,
independent. These characteristics should influence decision-making without requiring separate
AI implementations.

Acceptance criteria: two NPCs with different experiences or characteristics can exhibit
measurably different behavior.

---

# PHASE 4 — LEARNING AND ADAPTABILITY

Status: ACTIVE DEVELOPMENT

Objective: Allow NPCs to improve their decisions through experience.

## 4.1 Behavioral Adaptation

NPCs should recognize when previous approaches are ineffective. Examples: repeatedly
unsuccessful searches, ineffective investigation approaches, repeatedly losing track of an
objective, ineffective group coordination, environmental changes.

## 4.2 Learning Evaluation

Measure whether an adaptation actually improves performance. Track: previous behavior, new
behavior, outcome, confidence, number of attempts, learning stability. Avoid rewarding random
behavioral changes.

## 4.3 Controlled Learning

Implement safeguards against: unstable behavioral oscillation, runaway parameter changes,
overfitting to individual encounters, excessive memory consumption, unpredictable
decision-making.

## 4.4 Optional Frozen ML Integration

Keep future machine-learning integration separate from the deterministic intelligence core.
Investigate: frozen model inference, optional learned behavioral policies, model-independent
interfaces, deterministic fallbacks, CPU and memory overhead. ML must remain optional.

Acceptance criteria: NPCs demonstrate measurable improvement across repeated simulated
encounters without requiring a large language model.

---

# PHASE 5 — SOCIAL AND GROUP INTELLIGENCE

Objective: Create convincing social interactions and coordinated behavior.

## 5.1 Social Awareness

Implement or improve: relationships, trust, suspicion, reputation, faction awareness, witness
information, rumors, shared knowledge.

## 5.2 Information Exchange

NPCs should communicate relevant observations. Account for: communication range, available
communication channels, information confidence, information age, source reliability, whether
information was directly observed. NPCs must distinguish firsthand observations from secondhand
information.

## 5.3 Group Coordination

Support: shared objectives, group roles, formation management, task distribution, group
awareness, individual autonomy. Avoid requiring every NPC to receive identical information.

## 5.4 Independent Behavior

Group members should retain individual decision-making. They should be capable of: recognizing
changing circumstances, reporting relevant information, reassessing their assignments,
responding to conflicting observations.

Acceptance criteria: groups should exhibit coordinated behavior without every NPC behaving as a
synchronized copy of another NPC.

---

# PHASE 6 — NAVIGATION AND ENVIRONMENTAL AWARENESS

Objective: Make the intelligence system usable in complex game worlds.

## 6.1 Navigation Validation

Test: large environments, multiple floors, dynamic obstacles, crowded environments, narrow
passages, changing routes, repeated pathfinding.

## 6.2 Environmental Awareness

Improve the ability to reason about: visibility, sound propagation, obstacles, traversable
areas, environmental changes, last-known locations.

## 6.3 Navigation Scalability

Investigate: path caching, shared navigation information, update scheduling, adaptive search
sampling, efficient spatial queries. Do not sacrifice correctness for benchmark improvements.

Acceptance criteria: navigation remains reliable as the number of NPCs and environmental
complexity increase.

---

# PHASE 7 — PERFORMANCE AND SCALABILITY

Objective: Make Hydroxus AI practical for low-end hardware.

## 7.1 Benchmarking

Establish reproducible benchmarks. Measure: CPU utilization, memory consumption, update time,
NPC count, decision frequency, navigation overhead, memory retrieval overhead.

## 7.2 Scaling Tests

Test progressively larger NPC populations: 1, 10, 50, 100, 250, 500, and 1,000 NPCs where
practical. Record actual hardware and test conditions.

## 7.3 Optimization

Investigate: update scheduling, spatial partitioning, allocation reduction, caching, shared
calculations, selective perception updates, memory management. Retain a baseline before
modifying performance-sensitive code.

## 7.4 Low-End Hardware

Establish a low-resource configuration. Allow developers to adjust: perception frequency,
decision frequency, memory limits, navigation update frequency, social simulation frequency.

Acceptance criteria: provide documented performance results rather than unverified claims about
hardware compatibility.

---

# PHASE 8 — DEMONSTRATION ENVIRONMENTS

Objective: Demonstrate that Hydroxus AI offers practical intelligence rather than merely a
collection of algorithms. Create independent demonstrations using the existing Cargo environment
where possible.

- **A — Individual Awareness:** an NPC detecting, investigating, remembering, and responding to
  environmental events.
- **B — Adaptive Behavior:** an NPC changing its approach after repeated unsuccessful attempts.
- **C — Persistent Memory:** an NPC using information from previous encounters.
- **D — Social Information:** NPCs sharing information and distinguishing direct observations
  from rumors.
- **E — Group Intelligence:** multiple NPCs coordinating while retaining individual
  decision-making.
- **F — Behavioral Diversity:** different NPCs reacting differently to the same situation
  because of their individual characteristics and experiences.
- **G — Environmental Adaptation:** NPCs responding to changing navigation and environmental
  conditions.
- **H — Performance:** increasing NPC populations while recording CPU and memory consumption.

Every demonstration should include: source code, configuration, reproduction instructions,
expected behavior, actual results, performance information where applicable.

---

# PHASE 9 — DEVELOPER DOCUMENTATION

Objective: Make Hydroxus AI understandable and usable by independent game developers.

## 9.1 Implementation Handbook

Create a versioned handbook under `docs/handbook/`. Suggested chapters:

1. Introduction to Hydroxus AI
2. Architecture and Design Philosophy
3. Getting Started with Rust
4. Integrating Hydroxus AI into a Game
5. Perception Systems
6. Navigation
7. Tactical Decision-Making
8. Memory and Learning
9. Social Intelligence
10. Group Coordination
11. Behavioral Customization
12. Performance Optimization
13. Debugging and Diagnostics
14. Testing and Benchmarking
15. Fyrox Integration
16. Complete Example Projects

Each chapter should explain: the underlying concept, the relevant architecture, Rust
implementation, game-engine integration, configuration, testing, customization.

## 9.2 Examples

Provide working examples for: minimal NPC, perception, navigation, investigation, persistent
memory, adaptive behavior, group intelligence. Examples must compile against the corresponding
release.

## 9.3 API Documentation

Document public interfaces; explain configuration parameters; include examples; explain errors;
document compatibility; maintain changelog.

---

# PHASE 10 — RELEASE PREPARATION

Objective: Prepare Hydroxus AI for public adoption.

## 10.1 Stability

Complete test coverage review; verify public API; resolve known critical issues; establish
semantic versioning; verify supported Rust versions.

## 10.2 Repository

Improve README; add architecture documentation; add example projects; add benchmark reports; add
contribution guidelines; add licensing information; document limitations.

## 10.3 Release Demonstration

A comprehensive demonstration showcasing: independent NPC intelligence, persistent memory,
learning, adaptability, social behavior, group coordination, engine independence, performance.

## 10.4 Public Release

Publish a stable release only after the project has reproducible tests, documentation, and
demonstrations.

---

# Development rules

1. Inspect the existing repository before modifying anything.
2. Read the README and existing documentation.
3. Preserve existing working implementations.
4. Do not perform unnecessary architectural rewrites.
5. Work incrementally.
6. Complete one logical milestone at a time.
7. Add tests for every significant change.
8. Run tests after modifications.
9. Never claim a test passed without executing it.
10. Record any tests that cannot be executed and explain why.
11. Keep engine-specific code outside the intelligence core.
12. Keep optional ML integrations separate.
13. Avoid unnecessary dependencies.
14. Document architectural decisions.
15. Update the roadmap as milestones are completed.
16. Identify regressions immediately.
17. Do not silently remove existing functionality.
18. Do not substitute mock implementations for completed features.
19. Benchmark before and after performance optimizations.
20. Prefer explainable, reproducible behavior.

## End-of-session report

At the end of each development session: files inspected, files modified, features implemented,
tests added, tests executed, test results, known issues, performance changes, remaining tasks,
recommended next milestone - keeping completed, partially completed and unverified work apart.

## Final project objective

Hydroxus AI should become a modular, efficient, engine-independent NPC intelligence framework,
distinguished by persistent experience, adaptive behavior, individual intelligence, social
awareness, cooperative behavior, explainable decision-making, low computational overhead and
developer accessibility - demonstrating that sophisticated NPC behavior does not necessarily
require large language models or expensive inference infrastructure.
