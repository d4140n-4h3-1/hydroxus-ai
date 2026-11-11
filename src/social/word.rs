//! Word: what one NPC passes on to another of what it saw - where the player was, mostly - by
//! voice, by radio, or by alarm, and how much the one it reaches makes of it.
//!
//! Where [`super::rumour`] is what the town says of the player's deeds, spreading as talk, a
//! [`Word`] is news to act on: who saw what, where, and when. Each keeps:
//!
//! - **Whether it was seen firsthand.** `hops` is how many times it has been passed on since the
//!   one that saw it (`witness`) did: 0 for what the holder saw itself. `from` is who passed it on
//!   last - the one whose word it is taking.
//! - **How old it is.** `seen` is when it was seen, on the game's clock: it says where the player
//!   *was*, and the older it is, the less it says where they are.
//! - **How sure.** `confidence` is how sure the witness was, less each time it is passed on (the
//!   tuning's `retold`).
//!
//! How much a hearer believes it ([`Word::belief`]) is its confidence, times how fresh it is
//! (halving every `stale_half_life` seconds), times - unless the hearer saw it itself - how far it
//! trusts the one it came from ([`Trust`]): what it learns of each source as its word proves right
//! ([`Trust::confirmed`]) or wrong ([`Trust::refuted`]), starting from the tuning's `trust`.
//!
//! Whether word gets there at all ([`reaches`]) is down to the channel: shouted, only so far
//! (`voice_range`), and only where the game says the voice carries; by radio, much further
//! (`radio_range`), but only between two that both have one; an alarm, to everyone within
//! `alarm_range`. [`Heard`] keeps the best word an NPC has on each thing - what it saw itself
//! before what it was told of the same, unless that is long out of date.

use nalgebra::Vector3;

/// How far a shout carries, a radio reaches, and an alarm is heard, in meters.
pub const VOICE_RANGE: f32 = 12.0;
pub const RADIO_RANGE: f32 = 150.0;
pub const ALARM_RANGE: f32 = 40.0;
/// How sure word is once passed on, of how sure it was: four fifths.
pub const RETOLD: f32 = 0.8;
/// How long, in seconds, till word of where someone was is half as good: twenty seconds.
pub const STALE_HALF_LIFE: f32 = 20.0;
/// How far an NPC trusts a source it knows nothing of, from 0 to 1.
pub const TRUST: f32 = 0.7;
/// How far each time a source's word proves right or wrong moves its trust, at first.
pub const TRUST_RATE: f32 = 0.3;

tuning!(
    /// How word is passed on and taken (see the module's constants).
    {
        /// How far a shout carries, a radio reaches, and an alarm is heard, in meters.
        voice_range: f32 = VOICE_RANGE,
        radio_range: f32 = RADIO_RANGE,
        alarm_range: f32 = ALARM_RANGE,
        /// How sure word is once passed on, of how sure it was.
        retold: f32 = RETOLD,
        /// How long till word of where someone was is half as good, in seconds.
        stale_half_life: f32 = STALE_HALF_LIFE,
        /// How far an NPC trusts a source it knows nothing of.
        trust: f32 = TRUST,
        /// How far each time a source proves right or wrong moves its trust, at first.
        trust_rate: f32 = TRUST_RATE,
    }
);

/// How word is passed on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Channel {
    /// Said aloud: near, where the voice carries.
    Voice,
    /// Over the radio: far, between two that both have one.
    Radio,
    /// An alarm: everyone near enough.
    Alarm,
}

/// Whether word sent on `channel` by one at `from` reaches one at `to`: `radios`, whether each
/// has a radio; `carries`, whether the game finds a voice carries from the one to the other
/// (round no walls, say) - asked only for [`Channel::Voice`].
pub fn reaches(tuning: &Tuning, channel: Channel, from: Vector3<f32>, to: Vector3<f32>, radios: (bool, bool), carries: impl FnOnce() -> bool) -> bool {
    let away = (to - from).norm();
    if !away.is_finite() {
        return false;
    }
    match channel {
        Channel::Voice => away <= tuning.voice_range && carries(),
        Channel::Radio => radios.0 && radios.1 && away <= tuning.radio_range,
        Channel::Alarm => away <= tuning.alarm_range,
    }
}

/// Word of something seen: what, where, when, by whom, how sure, and how it came to be held.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Word<T> {
    /// What it is of, by the game's own kinds: the player seen, a body found.
    pub what: T,
    /// Where.
    pub at: Vector3<f32>,
    /// When it was seen, in seconds on the game's clock.
    pub seen: f32,
    /// Who saw it, and who passed it on last - the same, from the one that saw it.
    pub witness: u32,
    pub from: u32,
    /// How many times it has been passed on since it was seen: 0, the holder saw it itself.
    pub hops: u8,
    /// How sure of it, from 0 to 1, as the one passing it on has it.
    pub confidence: f32,
}

impl<T: Copy> Word<T> {
    /// `what`, seen at `at` at time `now` by `witness`, for itself: firsthand and sure.
    pub fn seen(what: T, at: Vector3<f32>, now: f32, witness: u32) -> Self {
        Self { what, at, seen: now, witness, from: witness, hops: 0, confidence: 1.0 }
    }

    /// As sure as `confidence`, rather than sure.
    pub fn confidence(self, confidence: f32) -> Self {
        Self { confidence, ..self }
    }

    /// Whether the holder saw it itself.
    pub fn firsthand(&self) -> bool {
        self.hops == 0
    }

    /// How old it is at `now`, in seconds.
    pub fn age(&self, now: f32) -> f32 {
        (now - self.seen).max(0.0)
    }

    /// It, as `teller` passes it on: one more hop, from it, the less sure by the tuning's `retold`.
    pub fn passed_on(&self, tuning: &Tuning, teller: u32) -> Self {
        Self { from: teller, hops: self.hops.saturating_add(1), confidence: self.confidence * tuning.retold.clamp(0.0, 1.0), ..*self }
    }

    /// How much one that holds it, with `trust` of its sources, believes it at `now`, from 0 to 1:
    /// how sure it is, how fresh, and - told it - how far it trusts the one it came from.
    pub fn belief(&self, tuning: &Tuning, trust: &Trust, now: f32) -> f32 {
        if !self.confidence.is_finite() || !self.seen.is_finite() || !now.is_finite() {
            return 0.0;
        }
        let fresh = 0.5f32.powf(self.age(now) / tuning.stale_half_life.max(1.0e-3));
        let source = if self.firsthand() { 1.0 } else { trust.of(tuning, self.from) };
        (self.confidence.clamp(0.0, 1.0) * fresh * source).clamp(0.0, 1.0)
    }
}

impl<T> Word<T> {
    /// How far one that holds it, with `trust` of its sources, believes it was so when it was
    /// seen, from 0 to 1: how sure it is and how far it trusts the one it came from, however old.
    /// For what it says of then - an alibi, say (see [`crate::social::suspicion`]) - where
    /// [`Word::belief`] is for what it says of now.
    pub fn credence(&self, tuning: &Tuning, trust: &Trust) -> f32 {
        if !self.confidence.is_finite() {
            return 0.0;
        }
        let source = if self.hops == 0 { 1.0 } else { trust.of(tuning, self.from) };
        (self.confidence.clamp(0.0, 1.0) * source).clamp(0.0, 1.0)
    }
}

/// How far an NPC trusts each source of word, from what it has made of their word so far.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Trust {
    /// Each source it knows of: how far it trusts it, and how many times its word was tested.
    sources: Vec<(u32, f32, f32)>,
}

impl Trust {
    /// Trusting everyone as far as the tuning's `trust`.
    pub fn new() -> Self {
        Self::default()
    }

    /// How far it trusts `source`, from 0 to 1.
    pub fn of(&self, tuning: &Tuning, source: u32) -> f32 {
        self.sources.iter().find(|s| s.0 == source).map_or(tuning.trust.clamp(0.0, 1.0), |s| s.1)
    }

    /// Starts trusting `source` as far as `trust`, from 0 to 1, rather than the tuning's `trust` -
    /// a friend sooner, an enemy less (see [`crate::social::relation::trust`]) - unless its word
    /// has been tested already: then what it has made of that stands.
    pub fn introduce(&mut self, source: u32, trust: f32) {
        if !trust.is_finite() {
            return;
        }
        let trust = trust.clamp(0.0, 1.0);
        match self.sources.iter_mut().find(|s| s.0 == source) {
            Some(s) if s.2 == 0.0 => s.1 = trust,
            Some(_) => {}
            None => self.sources.push((source, trust, 0.0)),
        }
    }

    /// `source`'s word proved right: it trusts it the more.
    pub fn confirmed(&mut self, tuning: &Tuning, source: u32) {
        self.test(tuning, source, 1.0);
    }

    /// `source`'s word proved wrong: it trusts it the less.
    pub fn refuted(&mut self, tuning: &Tuning, source: u32) {
        self.test(tuning, source, 0.0);
    }

    /// Each move the less the more often it has tested the source: one lie after many truths
    /// does not undo it, but a run of them does.
    fn test(&mut self, tuning: &Tuning, source: u32, toward: f32) {
        let start = tuning.trust.clamp(0.0, 1.0);
        let at = match self.sources.iter().position(|s| s.0 == source) {
            Some(n) => n,
            None => {
                self.sources.push((source, start, 0.0));
                self.sources.len() - 1
            }
        };
        let (_, trust, tested) = &mut self.sources[at];
        let rate = tuning.trust_rate.clamp(0.0, 1.0) / (1.0 + *tested * 0.1);
        *trust += (toward - *trust) * rate;
        *tested += 1.0;
    }
}

/// The best word an NPC has on each thing.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Heard<T> {
    words: Vec<Word<T>>,
}

impl<T> Default for Heard<T> {
    fn default() -> Self {
        Self { words: Vec::new() }
    }
}

impl<T: Copy + PartialEq> Heard<T> {
    /// Nothing heard.
    pub fn new() -> Self {
        Self::default()
    }

    /// It takes `word` in at `now`: kept, if it has nothing on the same thing yet or this is
    /// believed more than what it has - so what it saw itself comes before what it was told of
    /// the same, unless that is the fresher by enough to be believed the more. Whether it was
    /// kept. Word with no sense in it is let go.
    pub fn hear(&mut self, tuning: &Tuning, trust: &Trust, word: Word<T>, now: f32) -> bool {
        if !word.at.iter().all(|v| v.is_finite()) || !word.seen.is_finite() || !word.confidence.is_finite() {
            return false;
        }
        let belief = word.belief(tuning, trust, now);
        match self.words.iter_mut().find(|w| w.what == word.what) {
            Some(had) if had.belief(tuning, trust, now) >= belief => false,
            Some(had) => {
                *had = word;
                true
            }
            None => {
                self.words.push(word);
                true
            }
        }
    }

    /// The word it has on `what`, if any.
    pub fn of(&self, what: T) -> Option<&Word<T>> {
        self.words.iter().find(|w| w.what == what)
    }

    /// Forgets word believed less than `least` at `now`.
    pub fn forget(&mut self, tuning: &Tuning, trust: &Trust, now: f32, least: f32) {
        self.words.retain(|w| w.belief(tuning, trust, now) >= least);
    }

    /// Everything it has word of.
    pub fn words(&self) -> &[Word<T>] {
        &self.words
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T: Tuning = Tuning::DEFAULT;

    #[derive(Debug, Clone, Copy, PartialEq)]
    enum Saw {
        Player,
        Body,
    }

    fn at(x: f32) -> Vector3<f32> {
        Vector3::new(x, 0.0, 0.0)
    }

    #[test]
    fn each_channel_reaches_its_own_way() {
        let (here, near, far, very_far) = (at(0.0), at(10.0), at(100.0), at(200.0));
        assert!(reaches(&T, Channel::Voice, here, near, (false, false), || true));
        assert!(!reaches(&T, Channel::Voice, here, near, (false, false), || false), "not through a wall");
        assert!(!reaches(&T, Channel::Voice, here, far, (true, true), || true), "a shout only carries so far");
        assert!(reaches(&T, Channel::Radio, here, far, (true, true), || false));
        assert!(!reaches(&T, Channel::Radio, here, far, (true, false), || true), "both need a radio");
        assert!(!reaches(&T, Channel::Radio, here, very_far, (true, true), || true));
        assert!(reaches(&T, Channel::Alarm, here, at(35.0), (false, false), || false));
        assert!(!reaches(&T, Channel::Alarm, here, at(45.0), (false, false), || true));
        assert!(!reaches(&T, Channel::Voice, here, Vector3::new(f32::NAN, 0.0, 0.0), (true, true), || true));
    }

    #[test]
    fn firsthand_word_is_believed_more_than_told_and_fresh_more_than_stale() {
        let trust = Trust::new();
        let seen = Word::seen(Saw::Player, at(5.0), 10.0, 1);
        assert!(seen.firsthand());
        assert_eq!(seen.belief(&T, &trust, 10.0), 1.0);
        // Passed on once: less sure, and from someone trusted only so far.
        let told = seen.passed_on(&T, 1);
        assert!(!told.firsthand() && told.hops == 1 && told.from == 1 && told.witness == 1);
        assert!((told.belief(&T, &trust, 10.0) - RETOLD * TRUST).abs() < 1.0e-6);
        // And again: less still.
        let again = told.passed_on(&T, 2);
        assert!(again.belief(&T, &trust, 10.0) < told.belief(&T, &trust, 10.0));
        assert_eq!((again.hops, again.from, again.witness), (2, 2, 1));
        // Twenty seconds on, half as good.
        assert!((seen.belief(&T, &trust, 10.0 + STALE_HALF_LIFE) - 0.5).abs() < 1.0e-6);
        // But as good as ever of where they were then.
        assert_eq!(seen.credence(&T, &trust), 1.0);
        assert!((told.credence(&T, &trust) - RETOLD * TRUST).abs() < 1.0e-6);
    }

    #[test]
    fn a_source_whose_word_keeps_proving_wrong_is_trusted_less_and_one_lie_does_not_undo_many_truths() {
        let mut trust = Trust::new();
        assert_eq!(trust.of(&T, 4), TRUST);
        for _ in 0..4 {
            trust.refuted(&T, 4);
        }
        assert!(trust.of(&T, 4) < 0.3, "{}", trust.of(&T, 4));
        let mut honest = Trust::new();
        for _ in 0..20 {
            honest.confirmed(&T, 9);
        }
        let had = honest.of(&T, 9);
        honest.refuted(&T, 9);
        assert!(had > 0.95 && honest.of(&T, 9) > 0.8, "{had} -> {}", honest.of(&T, 9));
        // Word from a liar is believed less than the same word from someone it trusts.
        let word = Word::seen(Saw::Player, at(0.0), 0.0, 3).passed_on(&T, 4);
        let same_from_nine = Word { from: 9, ..word };
        let mut both = trust.clone();
        both.sources.extend(honest.sources.iter().copied());
        assert!(word.belief(&T, &both, 0.0) < same_from_nine.belief(&T, &both, 0.0) * 0.5);
    }

    #[test]
    fn a_source_introduced_is_trusted_as_said_until_tested() {
        let mut trust = Trust::new();
        trust.introduce(1, 0.95);
        trust.introduce(2, 0.2);
        trust.introduce(3, f32::NAN);
        assert_eq!((trust.of(&T, 1), trust.of(&T, 2), trust.of(&T, 3)), (0.95, 0.2, TRUST));
        trust.refuted(&T, 1);
        let tested = trust.of(&T, 1);
        assert!(tested < 0.95);
        // Tested, what it made of it stands.
        trust.introduce(1, 1.0);
        assert_eq!(trust.of(&T, 1), tested);
    }

    #[test]
    fn it_keeps_the_best_word_on_each_thing_its_own_eyes_first() {
        let trust = Trust::new();
        let mut heard = Heard::new();
        let told = Word::seen(Saw::Player, at(50.0), 10.0, 1).passed_on(&T, 1);
        assert!(heard.hear(&T, &trust, told, 10.0));
        // Seeing them itself, a moment later: its own eyes.
        let own = Word::seen(Saw::Player, at(20.0), 11.0, 0);
        assert!(heard.hear(&T, &trust, own, 11.0));
        assert!(heard.of(Saw::Player).unwrap().firsthand());
        // Told again of where they were, no fresher: it keeps what it saw.
        assert!(!heard.hear(&T, &trust, Word::seen(Saw::Player, at(60.0), 11.0, 2).passed_on(&T, 2), 11.5));
        // Told of somewhere they were long after it last saw them: that.
        let fresh = Word::seen(Saw::Player, at(70.0), 60.0, 2).passed_on(&T, 2);
        assert!(heard.hear(&T, &trust, fresh, 60.0));
        assert_eq!(heard.of(Saw::Player).unwrap().at, at(70.0));
        // Word of something else is kept alongside.
        assert!(heard.hear(&T, &trust, Word::seen(Saw::Body, at(5.0), 60.0, 3), 60.0));
        assert_eq!(heard.words().len(), 2);
        // In time it forgets what is no longer worth believing.
        heard.forget(&T, &trust, 60.0 + STALE_HALF_LIFE * 6.0, 0.05);
        assert!(heard.words().is_empty());
    }

    #[test]
    fn word_with_no_sense_in_it_is_let_go() {
        let trust = Trust::new();
        let mut heard = Heard::<Saw>::new();
        assert!(!heard.hear(&T, &trust, Word::seen(Saw::Player, Vector3::new(f32::NAN, 0.0, 0.0), 0.0, 1), 0.0));
        assert!(!heard.hear(&T, &trust, Word::seen(Saw::Player, at(0.0), f32::INFINITY, 1), 0.0));
        assert!(!heard.hear(&T, &trust, Word::seen(Saw::Player, at(0.0), 0.0, 1).confidence(f32::NAN), 0.0));
        assert!(heard.words().is_empty());
        assert_eq!(Word::seen(Saw::Player, at(0.0), 0.0, 1).belief(&T, &trust, f32::NAN), 0.0);
        // Word from the future is no older than fresh.
        assert_eq!(Word::seen(Saw::Player, at(0.0), 50.0, 1).belief(&T, &trust, 0.0), 1.0);
    }
}
