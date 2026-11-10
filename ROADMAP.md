# Hydroxus AI — Master Development Roadmap

## Status

Updated as milestones finish. **Done** means implemented, tested and the tests run; **Partial**
says what is there and what is not; **Not started** means nothing for it yet beyond what the
crate already did before the roadmap.

| Item | Status | Evidence / what is left |
|------|--------|-------------------------|
| 1.1 Module audit | Not started | Partly done by the 1.4 pass (clippy clean); no written audit yet. Found so far: looking about on reaching a search point is left to every game (Maze's `LOOK_ABOUT`; the scenario harness had to copy it), so `SearchMap::best` alone stalls a searcher at the centre of the likeliest polygon - a missing interface. |
| 1.2 Configuration validation | Not started | `Tuning` reads from a file with `serde`, but nothing rejects NaN, negative or contradictory values. |
| 1.3 Integration testing | **Done** (2026-11-10) | `tests/scenarios.rs`, 10 tests, through the public API only, on one corridor-and-room level with a test-only guard that runs the handbook's per-frame loop. The tests cover: an unfamiliar sound noticed and investigated, then back to the beat; an intruder spotted and chased, contact lost, and the last known place searched; one hiding found again by the search; a witness telling the nearest sentry, which searches there while the other knows nothing; a sentry joining on seeing another give chase while one facing away does not; and back to routine after an incident, with wariness fading. A full incident gives the same events in the same order at 30, 60 and 144 Hz and at uneven and hitching frame rates, within 0.25 s. Fading state is the same in one 600 s step as in 6,000 short ones, and runs are deterministic. This found and fixed a frame-rate bug: `Detection` drained for a whole long step once past `forget_after`, rather than only the part past it. |
| 1.4 Compilation and CI | **Done** but for CI (2026-11-10) | Clippy has 0 warnings with and without features. `cargo test` passes 159 tests with all features and 157 without. Docs build with warnings as errors. MSRV is 1.89 (nalgebra 0.35), set as `rust-version` and checked with `cargo +1.89 test --all-features`. The CI workflow (stable and 1.89, both feature sets, plus clippy and docs) is written but not pushed yet: the push token lacks GitHub's `workflow` scope. There are no doc tests, because the handbook's code is not compiled. |
| 2.1 Orchestration layer | Not started | `tactics::conduct` chooses by utility per role; nothing above it. |
| 2.2 Decision prioritisation | Partial | Utility scoring with inertia (`tactics::utility`); no urgency/confidence/commitment model. |
| 2.3 Behavioural inertia | Partial | `Reasoner` inertia; `Investigation` runs to completion; no general commitment. |
| 3.1 Persistent memory | Partial | Haunts (places), habits (player patterns), rumours/regard (characters), wariness; `serde` save. No unified memory record. |
| 3.2 Experience tracking | **Done** (2026-11-10) | `think::experience`: decisions (trail), outcomes, successes/failures, habits; measurable effect on choice (tests). |
| 3.3 Memory retrieval | Not started | |
| 3.4 Individual differences | Partial | `nerve` per NPC in `conduct`; per-NPC `Experience`. No trait set (cautious, curious…). |
| 4.1 Behavioural adaptation | **Done** (library) | `think::lesson`, `experience`: unsuccessful conduct weighed down, successful up. Not yet wired into Maze's droids. |
| 4.2 Learning evaluation | Partial | `Verdict`s say when an NPC changes its mind; no before/after performance measure. |
| 4.3 Controlled learning | **Done** (library) | Rate-limited, bounded −1..1, hysteresis on views, fading, bounded memory, deterministic; tested over 100,000 steps. |
| 4.4 Frozen ML | Not started | |
| 5.1–5.4 Social and group | Partial | `social` (factions, ties, rumours with first/second hand, report, group), `tactics::team`. Not reviewed against 5.2's list. |
| 6.1–6.3 Navigation | Partial | Navmesh with storeys, doors, ferries, trains; `examples/navmesh_bench.rs`. No scaling suite. |
| 7.1–7.4 Performance | Partial | `BENCHMARKS.md` (2026-11-10) records hardware and results for `ai_bench` and Maze's team battle (`BATTLE_BENCH`, arena and forest). `navmesh_bench` was not run: it needs a dumped grid, and none exists. No scaling from 1 to 1,000 NPCs, no memory measurements, no low-end profile. |
| 8 Demonstrations | Not started | Maze is the only integration. |
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
