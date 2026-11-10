//! Checking a [`Tuning`] before it is used: [`Tuning::validate`].
//!
//! A tuning read from a file, or changed by hand, can hold numbers no NPC can work with: a
//! half-life of nothing, a share past the whole, a negative distance, NaN. Most of the crate
//! clamps what it is given rather than misbehave, so such a number shows up as an NPC quietly
//! doing something odd, not as an error. `validate` finds them all at once, before anything runs,
//! and says for each one where it is, what it is, and what it should be:
//!
//! ```
//! let mut tuning = hydroxus_ai::Tuning::DEFAULT;
//! tuning.detection.fill = 0.0;
//! tuning.opinion.wary = 0.5;
//! let invalid = tuning.validate().unwrap_err();
//! assert_eq!(invalid.problems.len(), 2);
//! assert_eq!(invalid.problems[0].field, "detection.fill");
//! println!("{invalid}");
//! // detection.fill is 0, but must be more than 0
//! // opinion.wary is 0.5, but must be less than opinion.friendly (0.3)
//! ```
//!
//! It checks three things, in turn:
//!
//! 1. **Every number is a number:** finite, never NaN or infinite.
//! 2. **No number is negative**, but those that are meant to be: the regard thresholds below
//!    neutral, `opinion.wary` and `opinion.hostile`.
//! 3. **Each module's own rules:** shares and chances from 0 to 1; angles no wider than half a
//!    turn; what is divided by, a half-life or a time to fill, more than nothing; counts of at
//!    least one where none would mean the module never does anything; and settings that
//!    contradict each other - a guard that engages from further off than it can shoot, a rumour
//!    forgotten before it is too faint to tell, hostile regard above wary.
//!
//! A field is reported once, for the first thing wrong with it; and one out of order with another
//! that is wrong in itself is left for that to be put right first.
//!
//! With the `serde` feature, reading a tuning from a file refuses a field it does not have
//! (`unknown field "fil", expected one of "fill", ...`), so that a misspelt name is an error
//! rather than a default quietly kept. A field left out takes its default, as ever.

use crate::Tuning;
use std::fmt;

/// One number in a [`Tuning`] that is not as it should be.
#[derive(Debug, Clone, PartialEq)]
pub struct Problem {
    /// Where it is: the module's field in [`Tuning`], then its own, as `detection.fill`.
    pub field: String,
    /// What it is.
    pub value: f64,
    /// What it should be, as a reader would put it: `more than 0`, `from 0 to 1`.
    pub expected: String,
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} is {}, but must be {}", self.field, shown(self.value), self.expected)
    }
}

/// `value` as the tuning holds it: every number in one is an `f32` or a small whole number, so
/// shown as an `f32` it reads as it was written - 0.7, not 0.699999988079071.
fn shown(value: f64) -> String {
    (value as f32).to_string()
}

/// Everything wrong with a [`Tuning`], one [`Problem`] per field, in the order of [`Tuning`]'s
/// fields.
#[derive(Debug, Clone, PartialEq)]
pub struct Invalid {
    pub problems: Vec<Problem>,
}

impl fmt::Display for Invalid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (n, problem) in self.problems.iter().enumerate() {
            if n > 0 {
                writeln!(f)?;
            }
            write!(f, "{problem}")?;
        }
        Ok(())
    }
}

impl std::error::Error for Invalid {}

/// Whether `field` is meant to go below zero: the regard thresholds below neutral, and how far to
/// a group's leader's right each of it keeps (to its left, less than none).
fn signed(field: &str) -> bool {
    matches!(field, "opinion.wary" | "opinion.hostile") || field.starts_with("group.about_leader[") && field.ends_with(".0")
}

/// The numbers in a field of a tuning, by name, however it holds them.
pub(crate) trait Numbers {
    fn numbers(&self, name: String, out: &mut Vec<(String, f64)>);
}

macro_rules! numbers {
    ($($ty:ty),*) => {$(
        impl Numbers for $ty {
            fn numbers(&self, name: String, out: &mut Vec<(String, f64)>) {
                out.push((name, *self as f64));
            }
        }
    )*};
}
numbers!(f32, u8, u32, usize);

impl<A: Numbers, B: Numbers> Numbers for (A, B) {
    fn numbers(&self, name: String, out: &mut Vec<(String, f64)>) {
        self.0.numbers(format!("{name}.0"), out);
        self.1.numbers(format!("{name}.1"), out);
    }
}

impl<T: Numbers, const N: usize> Numbers for [T; N] {
    fn numbers(&self, name: String, out: &mut Vec<(String, f64)>) {
        for (n, item) in self.iter().enumerate() {
            item.numbers(format!("{name}[{n}]"), out);
        }
    }
}

/// What is found wrong, one field at a time.
struct Check {
    problems: Vec<Problem>,
}

impl Check {
    fn has(&self, field: &str) -> bool {
        self.problems.iter().any(|p| p.field == field)
    }

    /// `field` is `value`, but must be `expected` - unless it is reported already.
    fn wrong(&mut self, field: &str, value: f64, expected: String) {
        if !self.has(field) {
            self.problems.push(Problem { field: field.to_string(), value, expected });
        }
    }

    /// Every number of `module`'s: a number, and not negative unless it is meant to be.
    fn numbers(&mut self, module: &str, numbers: Vec<(String, f64)>) {
        for (name, value) in numbers {
            let field = format!("{module}.{name}");
            if !value.is_finite() {
                self.wrong(&field, value, "a finite number".into());
            } else if value < 0.0 && !signed(&field) {
                self.wrong(&field, value, "0 or more".into());
            }
        }
    }

    /// More than nothing: something divided by, or nothing would ever happen.
    fn positive(&mut self, field: &str, value: impl Into<f64>) {
        let value = value.into();
        if value.is_finite() && value <= 0.0 {
            self.wrong(field, value, "more than 0".into());
        }
    }

    /// At least one: a count that at nothing would have the module never do anything.
    fn some(&mut self, field: &str, value: impl Into<f64>) {
        let value = value.into();
        if value < 1.0 {
            self.wrong(field, value, "at least 1".into());
        }
    }

    /// From `low` to `high`, both included.
    fn within(&mut self, field: &str, value: impl Into<f64>, low: f64, high: f64) {
        let value = value.into();
        if value.is_finite() && !(low..=high).contains(&value) {
            self.wrong(field, value, format!("from {low} to {high}"));
        }
    }

    /// A share or a chance: from 0 to 1.
    fn fraction(&mut self, field: &str, value: impl Into<f64>) {
        self.within(field, value, 0.0, 1.0);
    }

    /// An angle either side of ahead, in radians: from nothing to straight behind.
    fn angle(&mut self, field: &str, value: impl Into<f64>) {
        let value = value.into();
        if value.is_finite() && !(0.0..=std::f64::consts::PI).contains(&value) {
            self.wrong(field, value, "from 0 to π (half a turn)".into());
        }
    }

    /// `field`, `value`, no more than `other`, `bound`.
    fn at_most(&mut self, field: &str, value: impl Into<f64>, other: &str, bound: impl Into<f64>) {
        let (value, bound) = (value.into(), bound.into());
        // The other already wrong in itself, being out of order with it is no news.
        if value.is_finite() && bound.is_finite() && value > bound && !self.has(other) {
            self.wrong(field, value, format!("no more than {other} ({})", shown(bound)));
        }
    }

    /// A range to pick from, `(low, high)`: low no more than high.
    fn range(&mut self, field: &str, (low, high): (f32, f32)) {
        self.at_most(&format!("{field}.0"), low, &format!("{field}.1"), high);
    }

    /// `field`, `value`, less than `other`, `bound`.
    fn below(&mut self, field: &str, value: impl Into<f64>, other: &str, bound: impl Into<f64>) {
        let (value, bound) = (value.into(), bound.into());
        if value.is_finite() && bound.is_finite() && value >= bound && !self.has(other) {
            self.wrong(field, value, format!("less than {other} ({})", shown(bound)));
        }
    }
}

impl Tuning {
    /// Every number in it that no NPC could work with, or that contradicts another - see
    /// [`crate::validate`] for what is checked. `Ok` if there are none.
    ///
    /// A game using one module's tuning on its own can check it in a whole one:
    /// `Tuning { search: mine, ..Tuning::DEFAULT }.validate()`.
    pub fn validate(&self) -> Result<(), Invalid> {
        let mut c = Check { problems: Vec::new() };
        let s = &self.sight;
        let sight = [
            ("range", s.range),
            ("cone", s.cone),
            ("side", s.side),
            ("peripheral", s.peripheral),
            ("crouched", s.crouched),
            ("crawling", s.crawling),
            ("dark", s.dark),
            ("edge", s.edge),
            ("glimpse", s.glimpse),
            ("faint", s.faint),
        ];
        c.numbers("sight", sight.iter().map(|&(name, value)| (name.to_string(), value as f64)).collect());
        let modules: [(&str, Vec<(String, f64)>); 41] = [
            ("alert", self.alert.numbers()),
            ("detection", self.detection.numbers()),
            ("bolts", self.bolts.numbers()),
            ("witness", self.witness.numbers()),
            ("search", self.search.numbers()),
            ("pace", self.pace.numbers()),
            ("investigate", self.investigate.numbers()),
            ("chase", self.chase.numbers()),
            ("threat", self.threat.numbers()),
            ("fire", self.fire.numbers()),
            ("blind", self.blind.numbers()),
            ("cover", self.cover.numbers()),
            ("team", self.team.numbers()),
            ("patrol", self.patrol.numbers()),
            ("grid", self.grid.numbers()),
            ("navmesh", self.navmesh.numbers()),
            ("walk", self.walk.numbers()),
            ("wander", self.wander.numbers()),
            ("steer", self.steer.numbers()),
            ("trip", self.trip.numbers()),
            ("ride", self.ride.numbers()),
            ("drive", self.drive.numbers()),
            ("traffic", self.traffic.numbers()),
            ("flight", self.flight.numbers()),
            ("group", self.group.numbers()),
            ("talk", self.talk.numbers()),
            ("report", self.report.numbers()),
            ("crowd", self.crowd.numbers()),
            ("opinion", self.opinion.numbers()),
            ("rumour", self.rumour.numbers()),
            ("chat", self.chat.numbers()),
            ("faction", self.faction.numbers()),
            ("trouble", self.trouble.numbers()),
            ("ties", self.ties.numbers()),
            ("passing", self.passing.numbers()),
            ("lesson", self.lesson.numbers()),
            ("habit", self.habit.numbers()),
            ("haunt", self.haunt.numbers()),
            ("memory", self.memory.numbers()),
            ("orchestrate", self.orchestrate.numbers()),
            ("priority", self.priority.numbers()),
        ];
        for (module, numbers) in modules {
            c.numbers(module, numbers);
        }
        self.rules(&mut c);
        // In the order of the tuning's own fields, whichever check found them.
        let order: Vec<String> = std::iter::once("sight".to_string())
            .chain(
                [
                    "alert", "detection", "bolts", "witness", "search", "pace", "investigate", "chase", "threat", "fire", "blind",
                    "cover", "team", "patrol", "grid", "navmesh", "walk", "wander", "steer", "trip", "ride", "drive", "traffic",
                    "flight", "group", "talk", "report", "crowd", "opinion", "rumour", "chat", "faction", "trouble", "ties",
                    "passing", "lesson", "habit", "haunt", "memory", "orchestrate", "priority",
                ]
                .map(String::from),
            )
            .collect();
        let rank = |p: &Problem| order.iter().position(|m| p.field.split('.').next() == Some(m.as_str())).unwrap_or(usize::MAX);
        c.problems.sort_by_key(rank);
        if c.problems.is_empty() {
            Ok(())
        } else {
            Err(Invalid { problems: c.problems })
        }
    }

    /// Each module's own rules, past being numbers and not negative.
    fn rules(&self, c: &mut Check) {
        let s = &self.sight;
        c.positive("sight.range", s.range);
        c.angle("sight.side", s.side);
        c.angle("sight.cone", s.cone);
        c.at_most("sight.cone", s.cone, "sight.side", s.side);
        for (name, value) in [
            ("peripheral", s.peripheral),
            ("crouched", s.crouched),
            ("crawling", s.crawling),
            ("dark", s.dark),
            ("edge", s.edge),
            ("glimpse", s.glimpse),
            ("faint", s.faint),
        ] {
            c.fraction(&format!("sight.{name}"), value);
        }

        let a = &self.alert;
        c.positive("alert.wary_half_life", a.wary_half_life);
        for (name, value) in [
            ("noticed", a.noticed),
            ("heard", a.heard),
            ("gunfire", a.gunfire),
            ("spotted", a.spotted),
            ("lost", a.lost),
            ("shot_at", a.shot_at),
            ("hurt", a.hurt),
            ("found_body", a.found_body),
            ("alarm", a.alarm),
        ] {
            c.fraction(&format!("alert.{name}"), value);
        }

        let d = &self.detection;
        c.positive("detection.fill", d.fill);
        c.within("detection.caution", d.caution, 0.0, 1.0);
        c.below("detection.caution", d.caution, "being spotted", 1.0);
        c.fraction("detection.hearing", d.hearing);

        // Ranges to pick a time or a distance from at random: low to high.
        for (field, range) in [
            ("pace.sprint_every", self.pace.sprint_every),
            ("pace.sprint_for", self.pace.sprint_for),
            ("fire.fire_every", self.fire.fire_every),
            ("fire.gang_fire_every", self.fire.gang_fire_every),
            ("blind.blind_fire_every", self.blind.blind_fire_every),
            ("team.stuck", self.team.stuck),
            ("patrol.trip", self.patrol.trip),
            ("patrol.search_trip", self.patrol.search_trip),
            ("patrol.rest", self.patrol.rest),
            ("traffic.cruise", self.traffic.cruise),
            ("crowd.crowd_back", self.crowd.crowd_back),
            ("crowd.crowd_rest", self.crowd.crowd_rest),
        ] {
            c.range(field, range);
        }

        let search = &self.search;
        c.fraction("search.keep", search.keep);
        c.fraction("search.ahead", search.ahead);
        c.positive("search.halved_after", search.halved_after);
        c.positive("search.followed", search.followed);
        c.angle("search.look_sweep", search.look_sweep);

        let pace = &self.pace;
        c.fraction("pace.sprint_reserve", pace.sprint_reserve);
        c.fraction("pace.ease_off", pace.ease_off);
        c.fraction("pace.eased", pace.eased);

        let inv = &self.investigate;
        c.positive("investigate.reach", inv.reach);
        c.positive("investigate.give_up", inv.give_up);
        c.angle("investigate.sweep", inv.sweep);

        let threat = &self.threat;
        c.at_most("threat.too_close", threat.too_close, "threat.unease_range", threat.unease_range);

        let fire = &self.fire;
        c.at_most("fire.engage", fire.engage, "fire.fire_range", fire.fire_range);
        c.at_most("fire.gang_engage", fire.gang_engage, "fire.fire_range", fire.fire_range);
        c.angle("fire.fire_cone", fire.fire_cone);
        c.some("fire.burst_shots", fire.burst_shots);
        c.some("fire.auto_burst", fire.auto_burst);

        c.some("blind.blind_after", self.blind.blind_after);
        c.some("cover.cover_tries", self.cover.cover_tries as f64);

        let team = &self.team;
        c.positive("team.exposure_every", team.exposure_every);
        c.angle("team.flank_angle", team.flank_angle);
        c.fraction("team.go_anyway", team.go_anyway);

        let patrol = &self.patrol;
        c.at_most("patrol.engage", patrol.engage, "patrol.fire_range", patrol.fire_range);
        c.positive("patrol.replan", patrol.replan);

        c.positive("grid.level", self.grid.level);
        c.some("navmesh.most_cells", self.navmesh.most_cells as f64);
        c.some("walk.tries", self.walk.tries as f64);

        let traffic = &self.traffic;
        c.positive("traffic.green", traffic.green);
        c.positive("traffic.accel", traffic.accel);
        c.positive("traffic.ease", traffic.ease);
        c.positive("traffic.brake", traffic.brake);

        c.angle("talk.talk_cone", self.talk.talk_cone);
        c.at_most("report.report_near", self.report.report_near, "report.report_reach", self.report.report_reach);

        let crowd = &self.crowd;
        c.positive("crowd.crowd_every", crowd.crowd_every);
        c.angle("crowd.out_of_view", crowd.out_of_view);
        c.some("crowd.crowd_tries", crowd.crowd_tries as f64);

        let o = &self.opinion;
        c.within("opinion.friendly", o.friendly, -1.0, 1.0);
        c.within("opinion.wary", o.wary, -1.0, 1.0);
        c.within("opinion.hostile", o.hostile, -1.0, 1.0);
        c.below("opinion.wary", o.wary, "opinion.friendly", o.friendly);
        c.below("opinion.hostile", o.hostile, "opinion.wary", o.wary);
        c.positive("opinion.regard_half_life", o.regard_half_life);

        let r = &self.rumour;
        for (name, value) in [
            ("retell", r.retell),
            ("hearsay", r.hearsay),
            ("news", r.news),
            ("tellable", r.tellable),
            ("forgotten", r.forgotten),
            ("swayed", r.swayed),
        ] {
            c.fraction(&format!("rumour.{name}"), value);
        }
        c.at_most("rumour.forgotten", r.forgotten, "rumour.tellable", r.tellable);
        c.positive("rumour.fade_half_life", r.fade_half_life);
        c.some("rumour.kept", r.kept as f64);

        c.fraction("chat.opinionated", self.chat.opinionated);

        let f = &self.faction;
        c.fraction("faction.share", f.share);
        c.fraction("faction.inherit", f.inherit);
        c.positive("faction.standing_half_life", f.standing_half_life);

        let t = &self.trouble;
        c.positive("trouble.fade_half_life", t.fade_half_life);
        c.fraction("trouble.shunned", t.shunned);

        let ties = &self.ties;
        c.fraction("ties.avenge_nerve", ties.avenge_nerve);
        c.fraction("ties.flee_nerve", ties.flee_nerve);
        c.at_most("ties.flee_nerve", ties.flee_nerve, "ties.avenge_nerve", ties.avenge_nerve);

        c.fraction("passing.nod", self.passing.nod);

        let l = &self.lesson;
        c.fraction("lesson.rate", l.rate);
        // At 1, what never works would weigh nothing: barred, where it should only be less likely.
        c.within("lesson.sway", l.sway, 0.0, 1.0);
        c.below("lesson.sway", l.sway, "1, so that nothing is barred", 1.0);
        c.positive("lesson.half_life", l.half_life);
        c.fraction("lesson.told", l.told);
        c.within("lesson.sure", l.sure, 0.0, 1.0);
        c.positive("lesson.sure", l.sure);
        c.positive("lesson.credit", l.credit);

        let h = &self.habit;
        c.positive("habit.half_life", h.half_life);
        c.positive("habit.sure", h.sure);
        c.fraction("habit.told", h.told);

        let haunt = &self.haunt;
        c.positive("haunt.half_life", haunt.half_life);
        c.positive("haunt.sure", haunt.sure);
        c.fraction("haunt.told", haunt.told);
        c.some("haunt.most", haunt.most as f64);

        let m = &self.memory;
        for kind in crate::think::memory::Kind::ALL {
            c.positive(&format!("memory.{}_half_life", format!("{kind:?}").to_lowercase()), m.half_life(kind));
        }
        c.fraction("memory.forgotten", m.forgotten);
        c.fraction("memory.told", m.told);
        c.fraction("memory.outcome_rate", m.outcome_rate);
        c.fraction("memory.consolidate_below", m.consolidate_below);

        let o = &self.orchestrate;
        c.positive("orchestrate.arrived", o.arrived);
        c.positive("orchestrate.look_every", o.look_every);
        c.positive("orchestrate.within", o.within);
        c.positive("orchestrate.sighting_half_life", o.sighting_half_life);
        c.positive("orchestrate.lead_half_life", o.lead_half_life);
        c.fraction("orchestrate.told", o.told);
        c.fraction("orchestrate.switch_lead_by", o.switch_lead_by);
        c.fraction("orchestrate.recheck", o.recheck);
        c.fraction("orchestrate.worth_a_look", o.worth_a_look);
        c.fraction("orchestrate.remembered_danger", o.remembered_danger);
        c.fraction("orchestrate.danger_noted", o.danger_noted);
        c.some("orchestrate.recall_most", o.recall_most as f64);

        let p = &self.priority;
        c.positive("priority.committed_after", p.committed_after);
        // Weighing down by as much as all of it would bar what it weighs: less than 1.
        c.below("priority.confidence", p.confidence, "1, so that nothing is barred", 1.0);
        c.below("priority.traits", p.traits, "1, so that nothing is barred", 1.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_defaults_are_valid() {
        assert_eq!(Tuning::DEFAULT.validate(), Ok(()));
    }

    #[test]
    fn it_says_what_is_wrong_where_and_what_it_should_be() {
        let mut tuning = Tuning::DEFAULT;
        tuning.detection.fill = 0.0;
        tuning.search.keep = 1.5;
        tuning.alert.evasion = -3.0;
        tuning.sight.range = f32::NAN;
        tuning.fire.engage = 30.0;
        let invalid = tuning.validate().unwrap_err();
        let said: Vec<String> = invalid.problems.iter().map(|p| p.to_string()).collect();
        assert_eq!(
            said,
            vec![
                "sight.range is NaN, but must be a finite number",
                "alert.evasion is -3, but must be 0 or more",
                "detection.fill is 0, but must be more than 0",
                "search.keep is 1.5, but must be from 0 to 1",
                "fire.engage is 30, but must be no more than fire.fire_range (24)",
            ]
        );
        assert_eq!(invalid.to_string().lines().count(), 5);
    }

    /// A change made to a tuning.
    type Change = fn(&mut Tuning);

    #[test]
    fn contradictory_settings_are_caught() {
        let contradictions: [(&str, Change); 7] = [
            ("opinion.wary", |t| t.opinion.wary = 0.5),
            ("opinion.hostile", |t| t.opinion.hostile = -0.1),
            ("rumour.forgotten", |t| t.rumour.forgotten = 0.4),
            ("ties.flee_nerve", |t| t.ties.flee_nerve = 0.6),
            ("threat.too_close", |t| t.threat.too_close = 15.0),
            ("sight.cone", |t| t.sight.cone = 2.0),
            ("report.report_near", |t| t.report.report_near = 200.0),
        ];
        for (field, contradict) in contradictions {
            let mut tuning = Tuning::DEFAULT;
            contradict(&mut tuning);
            let invalid = tuning.validate().unwrap_err();
            assert_eq!(invalid.problems.iter().map(|p| p.field.as_str()).collect::<Vec<_>>(), vec![field]);
        }
    }

    #[test]
    fn regard_may_go_below_zero_but_not_past_minus_one() {
        let mut tuning = Tuning::DEFAULT;
        tuning.opinion.wary = -0.4;
        tuning.opinion.hostile = -0.9;
        assert_eq!(tuning.validate(), Ok(()));
        tuning.opinion.hostile = -1.5;
        assert_eq!(tuning.validate().unwrap_err().problems[0].field, "opinion.hostile");
    }

    #[test]
    fn a_field_is_reported_once_for_the_first_thing_wrong_with_it() {
        let mut tuning = Tuning::DEFAULT;
        tuning.lesson.half_life = -1.0;
        tuning.lesson.told = f32::INFINITY;
        let invalid = tuning.validate().unwrap_err();
        assert_eq!(invalid.problems.len(), 2);
        assert_eq!(invalid.problems[0].expected, "0 or more");
        assert_eq!(invalid.problems[1].expected, "a finite number");
    }

    #[test]
    fn counts_of_nothing_are_caught() {
        let mut tuning = Tuning::DEFAULT;
        tuning.haunt.most = 0;
        tuning.navmesh.most_cells = 0;
        tuning.fire.burst_shots = 0;
        let fields: Vec<String> = tuning.validate().unwrap_err().problems.into_iter().map(|p| p.field).collect();
        assert_eq!(fields, vec!["fire.burst_shots", "navmesh.most_cells", "haunt.most"]);
    }

    #[test]
    fn a_range_given_backwards_and_a_pair_part_are_named() {
        let mut tuning = Tuning::DEFAULT;
        tuning.fire.fire_every = (1.3, 0.7);
        tuning.group.about_leader[2].1 = -1.0;
        tuning.group.about_leader[1].0 = -3.0;
        let said: Vec<String> = tuning.validate().unwrap_err().problems.iter().map(|p| p.to_string()).collect();
        assert_eq!(
            said,
            vec![
                "fire.fire_every.0 is 1.3, but must be no more than fire.fire_every.1 (0.7)",
                "group.about_leader[2].1 is -1, but must be 0 or more",
            ],
            "to the leader's left is fine; in front of it is not"
        );
    }

    #[test]
    fn learning_that_would_bar_what_failed_is_refused() {
        let mut tuning = Tuning::DEFAULT;
        tuning.lesson.sway = 1.0;
        let invalid = tuning.validate().unwrap_err();
        assert_eq!(invalid.problems[0].field, "lesson.sway");
        tuning.lesson.sway = 0.0;
        assert_eq!(tuning.validate(), Ok(()), "switched off is fine");
    }
}
