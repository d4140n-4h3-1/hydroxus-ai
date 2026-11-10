//! Learning evaluated over repeated simulated encounters (ROADMAP 4.2).
//!
//! Sentries face the same choice encounter after encounter: searching, and something noticed -
//! keep to the search, or go and look? The world decides how often each finds the player. They
//! choose by their role's reasoner weighed by what they have learned (`Experience`), and report
//! how it went; an `Evaluation` keeps the evidence. A control group is the same in every way but
//! with learning switched off (the lesson tuning's `sway` at nothing).

use hydroxus_ai::{
    navigation::grid::Rng,
    tactics::conduct::{Conduct, Reading, Reasoners, Role},
    think::{
        evaluate::{judge, Evaluation, Judgement, Tally},
        experience::{Experience, Outcome, Venture},
    },
    Tuning,
};

const AI: Tuning = Tuning::DEFAULT;
const SENTRIES: usize = 20;
const ENCOUNTERS: usize = 150;
/// How long between encounters, in seconds: what it learned fades meanwhile.
const BETWEEN: f32 = 30.0;

/// Searching, and something noticed: both open.
const CHOICE: Reading = Reading {
    hunting: false,
    foe: false,
    shot_at: false,
    investigating: true,
    searching: true,
    reporting: false,
    sheltering: false,
    mourning: false,
    afraid: false,
    watching: false,
    avoiding: false,
    regarding: false,
    partnered: false,
    nerve: 0.5,
    wariness: 0.0,
};

fn found(rng: &mut Rng, chance: f32) -> bool {
    (rng.below(10_000) as f32) < chance * 10_000.0
}

/// One group of sentries through `ENCOUNTERS` encounters in a world where looking into what it
/// noticed finds the player with chance `look` and keeping to the search with chance `search`.
/// Each sentry's evaluation, and each encounter's outcome (1 found, 0 not) for every sentry.
fn run(tuning: &Tuning, look: f32, search: f32, seed: u64) -> (Vec<Evaluation<Conduct>>, Vec<Vec<f32>>) {
    run_for(tuning, |_| (look, search), ENCOUNTERS, seed)
}

/// As `run`, for `encounters`, the world's chances in each encounter `world(n)`: (look, search).
fn run_for(tuning: &Tuning, world: impl Fn(usize) -> (f32, f32), encounters: usize, seed: u64) -> (Vec<Evaluation<Conduct>>, Vec<Vec<f32>>) {
    let mut rng = Rng::new(seed);
    let mut reasoners = Reasoners::default();
    let mut evaluations = Vec::new();
    let mut outcomes = Vec::new();
    for _ in 0..SENTRIES {
        let mut experience = Experience::<()>::new();
        let mut evaluation = Evaluation::new();
        let mut mine = Vec::new();
        for n in 0..encounters {
            let (look, search) = world(n);
            let conduct = experience.choose(tuning, &mut reasoners, Role::Sentry, &CHOICE, None);
            let (venture, chance) = match conduct {
                Conduct::Investigate => (Venture::Investigation, look),
                Conduct::Search => (Venture::Search, search),
                other => panic!("only looking and searching are open: {other:?}"),
            };
            let success = found(&mut rng, chance);
            experience.report(tuning, None, if success { Outcome::succeeded(venture) } else { Outcome::came_to_nothing(venture) });
            evaluation.attempt(conduct, if success { 1.0 } else { 0.0 });
            mine.push(if success { 1.0 } else { 0.0 });
            experience.held();
            experience.step(tuning, BETWEEN);
        }
        evaluations.push(evaluation);
        outcomes.push(mine);
    }
    (evaluations, outcomes)
}

/// Every sentry's outcomes in encounters `range`, together.
fn pooled(outcomes: &[Vec<f32>], range: std::ops::Range<usize>) -> Tally {
    let mut tally = Tally::default();
    for mine in outcomes {
        for &x in &mine[range.clone()] {
            tally.add(x);
        }
    }
    tally
}

fn off() -> Tuning {
    Tuning { lesson: hydroxus_ai::think::lesson::Tuning { sway: 0.0, ..AI.lesson }, ..AI }
}

#[test]
fn where_one_way_works_better_learners_measurably_improve_and_the_unlearning_do_not() {
    // Five worlds, each its own luck. Each is held to doing clearly better than its control; the
    // first encounters against the last are judged over all five together, since in a world
    // where the first few went luckily, three encounters each tell too little.
    let (mut early, mut late) = (Tally::default(), Tally::default());
    for seed in [1, 7, 13, 21, 42] {
        let (e, l) = one_way_better(seed);
        for (into, from) in [(&mut early, e), (&mut late, l)] {
            for x in from {
                into.add(x);
            }
        }
    }
    assert!(matches!(judge(&early, &late, 1.96, 10), Judgement::Better { by } if by > 0.15), "early {:?} late {:?}", early.mean(), late.mean());
}

/// One world where one way works better: held to the learners doing clearly better than the
/// control, and settling on what works. Every learner's first three outcomes, and last fifty.
fn one_way_better(seed: u64) -> (Vec<f32>, Vec<f32>) {
    // Looking into what it noticed finds them 30% of the time; keeping to the search, 70%. The
    // reasoner alone looks first (0.85 against 0.8).
    let (learners, learned) = run(&AI, 0.3, 0.7, seed);
    let (controls, unlearned) = run(&off(), 0.3, 0.7, seed + 1000);
    // Before they have learned anything much: their first three encounters, 60 in all.
    let early = pooled(&learned, 0..3);
    let late = pooled(&learned, 100..150);
    let control_late = pooled(&unlearned, 100..150);
    println!(
        "seed {seed}: learners found them {:.2} of the time in their first three encounters, {:.2} in their last fifty; the control {:.2}",
        early.mean().unwrap(),
        late.mean().unwrap(),
        control_late.mean().unwrap()
    );
    // Better than the control - clearly, not by chance.
    assert!(matches!(judge(&control_late, &late, 1.96, 10), Judgement::Better { by } if by > 0.25), "control {:?} learners {:?}", control_late.mean(), late.mean());
    // The control never changes its ways; it does no better late than early.
    assert!(controls.iter().all(|e| e.adaptations().is_empty() && e.habit() == Some(Conduct::Investigate)));
    assert_eq!(judge(&pooled(&unlearned, 0..50), &control_late, 1.96, 10), Judgement::NoClearDifference);
    // Each learner came to search more than look - most of them as a rule by the end - changing
    // its ways seldom; none of its changes made it do worse. It lapses now and then: once it has
    // stopped looking, what it learned of looking fades back toward nothing (half in 20 encounters
    // here) while searching's own failures count against it, so the two meet again, and it looks
    // a while before it relearns. Hence the group, not each one, is held to always improving.
    let mut better = 0;
    let (mut on_search, mut changes, mut searches) = (0, 0.0, 0);
    let mut worst = 1.0f32;
    for e in &learners {
        let (searched, looked) = (e.tally(Conduct::Search).n, e.tally(Conduct::Investigate).n);
        worst = worst.min(searched as f32 / (searched + looked) as f32);
        searches += searched;
        on_search += usize::from(e.habit() == Some(Conduct::Search));
        changes += e.stability();
        for a in e.adaptations() {
            match e.judge(a, 1.96, 5) {
                Judgement::Better { .. } if a.to == Conduct::Search => better += 1,
                Judgement::Worse { .. } if a.to == Conduct::Search => panic!("seed {seed}: taking to searching judged worse: {a:?}"),
                _ => (),
            }
        }
    }
    let changes = changes / SENTRIES as f32;
    println!("seed {seed}: {on_search} of {SENTRIES} search as a rule at the end; {changes:.2} changes of habit per hundred encounters; {better} changes to searching judged better on their own");
    // Settled on what works, and seldom talked out of it by a run of bad luck (ROADMAP 4.3) - in
    // 4.2's measurements, before settling and trying the other way, these were 15 of 20, 1.7 changes
    // a hundred, 0.83 in all and 0.51 for the least of any one.
    assert!(on_search >= 18, "{on_search} of {SENTRIES} search as a rule at the end");
    assert!(changes <= 1.5, "{changes} changes of habit per hundred encounters");
    assert!(worst >= 0.6, "one searched only {worst:.2} of the time");
    let share = searches as f32 / (SENTRIES * ENCOUNTERS) as f32;
    println!("seed {seed}: searched {share:.2} of the time in all, the least of any one {worst:.2}");
    assert!(share >= 0.85, "searched {share:.2} of the time in all");
    // Some took to searching after only a handful of looks - too few to judge each change on its
    // own - which is why the groups are judged together above.
    assert!(better > 0);
    let firsts = learned.iter().flat_map(|mine| mine[0..3].iter().copied()).collect();
    let lasts = learned.iter().flat_map(|mine| mine[100..150].iter().copied()).collect();
    (firsts, lasts)
}

#[test]
fn where_neither_way_works_better_no_improvement_is_claimed() {
    // Both find them half the time. Learners change their ways now and then, on runs of luck; the
    // evaluation must not take that for learning.
    let mut adaptations = 0;
    let mut claimed = 0;
    for seed in 0..10 {
        let (learners, learned) = run(&AI, 0.5, 0.5, 100 + seed);
        for e in &learners {
            for a in e.adaptations() {
                adaptations += 1;
                if matches!(e.judge(a, 1.96, 10), Judgement::Better { .. }) {
                    claimed += 1;
                }
            }
        }
        // Nor do they do better late than early on the whole.
        let (early, late) = (pooled(&learned, 0..50), pooled(&learned, 100..150));
        assert!(!matches!(judge(&early, &late, 1.96, 10), Judgement::Better { by } if by > 0.1), "seed {seed}: {:?} -> {:?}", early.mean(), late.mean());
    }
    assert!(adaptations > 20, "they did change their ways: {adaptations}");
    let rate = claimed as f32 / adaptations as f32;
    println!("no difference to find: {adaptations} changes of habit, {claimed} judged better ({:.1}%)", rate * 100.0);
    assert!(rate <= 0.10, "{claimed} of {adaptations} changes judged better where nothing was");
}

#[test]
fn when_the_world_changes_settled_learners_still_change_their_minds() {
    // 150 encounters where searching works (70%) and looking does not (30%); then 150 where it is
    // the other way round. Settled on searching, they must not stay settled on it.
    let swapped = |n: usize| if n < 150 { (0.3, 0.7) } else { (0.7, 0.3) };
    for seed in [3, 8, 34] {
        let (learners, learned) = run_for(&AI, swapped, 300, seed);
        let mut turned = Vec::new();
        for e in &learners {
            assert_eq!(e.habit(), Some(Conduct::Investigate), "seed {seed}: it came round to looking");
            let back = e.adaptations().iter().filter(|a| a.at > 150 && a.to == Conduct::Investigate).map(|a| a.at - 150).next();
            // One already looking as a rule when the world changed has nothing to come round from.
            turned.push(back.unwrap_or(0));
        }
        turned.sort_unstable();
        let median = turned[turned.len() / 2];
        // Just after the change, before they could have learned it - five encounters each.
        let before = pooled(&learned, 150..155);
        let after = pooled(&learned, 250..300);
        println!("seed {seed}: came round to looking a median {median} encounters after the change (all: {turned:?}); found them {:.2} just after, {:.2} at the end", before.mean().unwrap(), after.mean().unwrap());
        assert!(median <= 40, "seed {seed}: a median {median} encounters to come round");
        assert!(matches!(judge(&before, &after, 1.96, 10), Judgement::Better { by } if by > 0.15), "seed {seed}");
        assert!(after.mean().unwrap() >= 0.6, "seed {seed}: near the 0.7 there is to be had");
    }
}
