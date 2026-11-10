//! Learning, measured: `cargo run --release --example learning`.
//!
//! Twenty sentries face the same choice 150 times: searching, and something noticed - keep to the
//! search, or go and look? Here going to look finds the player 30% of the time, keeping to the
//! search 70%; left to itself the reasoner goes to look. Learners choose by what they have learned
//! (`Experience`); a control group is the same with learning switched off. Printed: how often each
//! group found the player in each block of 25 encounters, then what the evaluation makes of it -
//! and the same for a world where both ways work equally well, where nothing should be claimed.
use hydroxus_ai::{
    navigation::grid::Rng,
    tactics::conduct::{Conduct, Reading, Reasoners, Role},
    think::{
        evaluate::{judge, Evaluation, Judgement, Tally},
        experience::{Experience, Outcome, Venture},
        lesson,
    },
    Tuning,
};

const SENTRIES: usize = 20;
const ENCOUNTERS: usize = 150;

fn run(tuning: &Tuning, look: f32, search: f32, seed: u64) -> (Vec<Evaluation<Conduct>>, Vec<Vec<f32>>) {
    let choice = Reading { investigating: true, searching: true, nerve: 0.5, ..Reading::default() };
    let mut rng = Rng::new(seed);
    let mut reasoners = Reasoners::default();
    let (mut evaluations, mut outcomes) = (Vec::new(), Vec::new());
    for _ in 0..SENTRIES {
        let (mut experience, mut evaluation, mut mine) = (Experience::<()>::new(), Evaluation::new(), Vec::new());
        for _ in 0..ENCOUNTERS {
            let conduct = experience.choose(tuning, &mut reasoners, Role::Sentry, &choice, None);
            let (venture, chance) = if conduct == Conduct::Investigate { (Venture::Investigation, look) } else { (Venture::Search, search) };
            let found = (rng.below(10_000) as f32) < chance * 10_000.0;
            experience.report(tuning, None, if found { Outcome::succeeded(venture) } else { Outcome::came_to_nothing(venture) });
            let outcome = if found { 1.0 } else { 0.0 };
            evaluation.attempt(conduct, outcome);
            mine.push(outcome);
            experience.held();
            experience.step(tuning, 30.0);
        }
        evaluations.push(evaluation);
        outcomes.push(mine);
    }
    (evaluations, outcomes)
}

fn pooled(outcomes: &[Vec<f32>], from: usize, to: usize) -> Tally {
    let mut tally = Tally::default();
    for mine in outcomes {
        for &x in &mine[from..to] {
            tally.add(x);
        }
    }
    tally
}

fn world(name: &str, look: f32, search: f32) {
    let ai = Tuning::DEFAULT;
    let off = Tuning { lesson: lesson::Tuning { sway: 0.0, ..ai.lesson }, ..ai };
    let (learners, learned) = run(&ai, look, search, 1);
    let (_, unlearned) = run(&off, look, search, 2);
    println!("\n{name}: looking finds them {:.0}% of the time, searching {:.0}%", look * 100.0, search * 100.0);
    println!("  encounters   learners found   control found");
    for block in 0..ENCOUNTERS / 25 {
        let (from, to) = (block * 25, block * 25 + 25);
        let (l, c) = (pooled(&learned, from, to), pooled(&unlearned, from, to));
        let (lo, hi) = l.interval(1.96).unwrap();
        println!("  {:3}-{:3}       {:.2} ({lo:.2}-{hi:.2})      {:.2}", from + 1, to, l.mean().unwrap(), c.mean().unwrap());
    }
    let late = (pooled(&learned, 100, 150), pooled(&unlearned, 100, 150));
    println!("  last fifty, learners against control: {:?}", judge(&late.1, &late.0, 1.96, 10));
    let adaptations: Vec<_> = learners.iter().flat_map(|e| e.adaptations().iter().map(move |a| (e, *a))).collect();
    let better = adaptations.iter().filter(|(e, a)| matches!(e.judge(a, 1.96, 10), Judgement::Better { .. })).count();
    let stability = learners.iter().map(|e| e.stability()).sum::<f32>() / SENTRIES as f32;
    let searching = learners.iter().filter(|e| e.habit() == Some(Conduct::Search)).count();
    println!("  {} changes of habit ({stability:.2} per hundred encounters), {better} of them judged better on their own; {searching} of {SENTRIES} search as a rule at the end", adaptations.len());
}

fn main() {
    world("One way works better", 0.3, 0.7);
    world("Neither way works better", 0.5, 0.5);
}
