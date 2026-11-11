//! A routine: an NPC's day, by the hour. A [`Routine`] is what it does from when - home till eight,
//! work till five, the shops, home again - each block lasting till the next one starts, the last
//! running on past midnight into the first. Hours are the game's day as 0 up to [`DAY`], however
//! long its days are in seconds.
//!
//! Each NPC keeps its own routine a little early or late ([`Routine::shifted`], [`offset`]): a
//! street of them leaving for work does not all set off on the stroke of the hour. The offset
//! comes from the NPC's own number, so it is the same every day and every run.
//!
//! What it does is the game's own kind, `A`: an activity, a place, or both.

/// Hours in a day.
pub const DAY: f32 = 24.0;
/// How early or late, in hours, an NPC keeps its routine, at most either way, by default: a
/// quarter of an hour.
pub const LEEWAY: f32 = 0.25;

tuning!(
    /// How routines are kept (see the module's constants, its defaults).
    {
        /// How early or late an NPC keeps its routine, at most either way, in hours.
        leeway: f32 = LEEWAY,
    }
);

/// What an NPC does from an hour of the day.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Block<A> {
    /// When it starts, in hours from midnight: 0 up to [`DAY`].
    pub from: f32,
    pub doing: A,
}

/// An NPC's day: what it does from when, in order of the hour, at least one thing.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Routine<A> {
    blocks: Vec<Block<A>>,
}

/// `hour` brought round into 0 up to [`DAY`].
pub fn wrap(hour: f32) -> f32 {
    if hour.is_finite() {
        hour.rem_euclid(DAY)
    } else {
        0.0
    }
}

/// How early (less than 0) or late an NPC numbered `seed` keeps its routine, in hours: up to the
/// tuning's `leeway` either way, the same for it every time.
pub fn offset(tuning: &Tuning, seed: u32) -> f32 {
    let mut x = seed.wrapping_mul(0x9E37_79B9) ^ 0x5EED_1EAF;
    x ^= x >> 15;
    x = x.wrapping_mul(0x2C1B_3C6D);
    x ^= x >> 12;
    let unit = (x >> 8) as f32 / (1u32 << 24) as f32;
    (unit * 2.0 - 1.0) * tuning.leeway.max(0.0)
}

impl<A: Clone> Routine<A> {
    /// A day of `blocks`, each from its hour - put in order, each hour brought round into the day;
    /// any with no sense in it left out. None if that leaves nothing to do.
    pub fn new(blocks: impl IntoIterator<Item = Block<A>>) -> Option<Self> {
        let mut blocks: Vec<Block<A>> =
            blocks.into_iter().filter(|b| b.from.is_finite()).map(|b| Block { from: wrap(b.from), ..b }).collect();
        if blocks.is_empty() {
            return None;
        }
        blocks.sort_by(|a, b| a.from.total_cmp(&b.from));
        Some(Self { blocks })
    }

    /// The same day, every block `hours` later - earlier, less than 0: one NPC's own way of keeping
    /// a routine it shares with others (see [`offset`]).
    pub fn shifted(&self, hours: f32) -> Self {
        let hours = if hours.is_finite() { hours } else { 0.0 };
        Self::new(self.blocks.iter().map(|b| Block { from: b.from + hours, doing: b.doing.clone() })).expect("as many as it had")
    }

    /// The block under way at `hour`: the last to have started - before midnight, the day's last.
    fn index_at(&self, hour: f32) -> usize {
        let hour = wrap(hour);
        match self.blocks.iter().rposition(|b| b.from <= hour) {
            Some(n) => n,
            None => self.blocks.len() - 1,
        }
    }

    /// What it does at `hour`.
    pub fn at(&self, hour: f32) -> &A {
        &self.blocks[self.index_at(hour)].doing
    }

    /// What it does next after `hour`, and how many hours till then - with only one thing to do,
    /// that again, a day on.
    pub fn next(&self, hour: f32) -> (f32, &A) {
        let hour = wrap(hour);
        let next = &self.blocks[(self.index_at(hour) + 1) % self.blocks.len()];
        let mut till = next.from - hour;
        if till <= 0.0 {
            till += DAY;
        }
        (till, &next.doing)
    }

    /// Its blocks, in order of the hour.
    pub fn blocks(&self) -> &[Block<A>] {
        &self.blocks
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, Copy, PartialEq)]
    enum Doing {
        Home,
        Work,
        Shops,
    }

    fn day() -> Routine<Doing> {
        Routine::new([
            Block { from: 17.0, doing: Doing::Shops },
            Block { from: 8.0, doing: Doing::Work },
            Block { from: 18.5, doing: Doing::Home },
        ])
        .unwrap()
    }

    #[test]
    fn each_block_lasts_till_the_next_and_the_last_runs_on_past_midnight() {
        let d = day();
        assert_eq!(d.blocks()[0].from, 8.0, "put in order");
        assert_eq!((*d.at(9.0), *d.at(17.0), *d.at(18.0), *d.at(23.0)), (Doing::Work, Doing::Shops, Doing::Shops, Doing::Home));
        assert_eq!(*d.at(3.0), Doing::Home, "before the first, the day's last still");
        assert_eq!(*d.at(24.0 + 9.0), Doing::Work, "hours brought round into the day");
        assert_eq!(*d.at(-1.0), Doing::Home);
        assert_eq!(d.next(16.0), (1.0, &Doing::Shops));
        let (till, then) = d.next(20.0);
        assert!((till - 12.0).abs() < 1.0e-4 && *then == Doing::Work, "{till}");
        let alone = Routine::new([Block { from: 6.0, doing: Doing::Home }]).unwrap();
        assert_eq!(alone.next(6.0), (DAY, &Doing::Home));
    }

    #[test]
    fn each_npc_keeps_it_a_little_early_or_late_the_same_every_time() {
        let t = Tuning::DEFAULT;
        let offsets: Vec<f32> = (0..500).map(|seed| offset(&t, seed)).collect();
        assert!(offsets.iter().all(|o| o.abs() <= LEEWAY));
        assert!(offsets.iter().any(|&o| o < -0.2) && offsets.iter().any(|&o| o > 0.2), "spread both ways");
        assert_eq!(offset(&t, 42), offset(&t, 42));
        // Shifted by its offset, one leaves for work a few minutes off another.
        let (a, b) = (day().shifted(offsets[1]), day().shifted(offsets[2]));
        assert!((a.blocks()[0].from - b.blocks()[0].from).abs() > 0.0);
        // Shifted past midnight, the day still holds together.
        let late = day().shifted(7.0);
        assert_eq!(*late.at(0.5), Doing::Shops, "the shops from midnight, its 17:00 seven hours on");
        assert_eq!(*late.at(2.0), Doing::Home, "home from 01:30, its 18:30 seven hours on");
        assert_eq!(*late.at(16.0), Doing::Work);
        assert_eq!(offset(&Tuning { leeway: 0.0 }, 7), 0.0);
    }

    #[test]
    fn nonsense_is_left_out() {
        assert!(Routine::<Doing>::new([]).is_none());
        assert!(Routine::new([Block { from: f32::NAN, doing: Doing::Home }]).is_none());
        let d = Routine::new([Block { from: f32::INFINITY, doing: Doing::Work }, Block { from: 8.0, doing: Doing::Home }]).unwrap();
        assert_eq!(d.blocks().len(), 1);
        assert_eq!(*d.at(f32::NAN), Doing::Home);
        assert_eq!(*day().shifted(f32::NAN).at(9.0), Doing::Work);
    }
}
