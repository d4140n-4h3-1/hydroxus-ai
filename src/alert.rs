//! Metal Gear's alert phases, which a hostile NPC goes through as it hunts the player:
//!
//! - **Alert**: it can see them, and is after them.
//! - **Evasion**: it has lost them, and searches, for [`EVASION`] seconds.
//! - **Caution**: it has given up searching, and goes about as before, but watching for them, for
//!   [`CAUTION`] seconds; then it is calm again.
//!
//! Seeing them again in any phase, it is back on Alert.
//!
//! How long Evasion and Caution last is not fixed: an NPC that has been through something stays
//! on edge longer, as someone who has found a body would. Its [`Wariness`] remembers what it has
//! been through - noticing something, hearing gunfire, seeing the player, losing them, being shot
//! at, being hurt, finding a body, an alarm - the worse, the more, and fades over minutes. The
//! warier it is, the longer it searches and stays wary ([`Wariness::lasts`]), and the more
//! quickly it becomes aware of the player again ([`Wariness::keenness`]).

/// How long an NPC searches for the player once it has lost them, in seconds, and how long it
/// stays wary after that.
pub const EVASION: f32 = 30.0;
pub const CAUTION: f32 = 60.0;

/// How much each [`Incident`] makes an NPC warier: of how far it has to go to being as wary as
/// it can be.
pub const NOTICED: f32 = 0.1;
pub const HEARD: f32 = 0.2;
pub const GUNFIRE: f32 = 0.3;
pub const SPOTTED: f32 = 0.4;
pub const LOST: f32 = 0.2;
pub const SHOT_AT: f32 = 0.5;
pub const HURT: f32 = 0.6;
pub const FOUND_BODY: f32 = 0.8;
pub const ALARM: f32 = 0.4;
/// How long wariness takes to fade halfway, in seconds; how much longer, as wary as can be, the
/// phases last, as a share of their length (2: three times as long); and how much more keenly it
/// looks for the player (1: twice as keenly).
pub const WARY_HALF_LIFE: f32 = 180.0;
pub const LONGER: f32 = 2.0;
pub const KEENER: f32 = 1.0;

tuning!(
    /// How long the phases last, in seconds, once the player is out of sight; and how what an NPC
    /// has been through draws them out (see [`Wariness`]).
    {
        /// Searching, once it has lost them.
        evasion: f32 = EVASION,
        /// Wary, once it has given up searching.
        caution: f32 = CAUTION,
        /// How much warier each incident makes it.
        noticed: f32 = NOTICED,
        heard: f32 = HEARD,
        gunfire: f32 = GUNFIRE,
        spotted: f32 = SPOTTED,
        lost: f32 = LOST,
        shot_at: f32 = SHOT_AT,
        hurt: f32 = HURT,
        found_body: f32 = FOUND_BODY,
        alarm: f32 = ALARM,
        /// How long wariness takes to fade halfway, in seconds.
        wary_half_life: f32 = WARY_HALF_LIFE,
        /// How much longer the phases last, as wary as can be, of their length; and how much more
        /// keenly it looks.
        longer: f32 = LONGER,
        keener: f32 = KEENER,
    }
);

/// Something an NPC goes through that leaves it warier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Incident {
    /// It noticed something: a glimpse, a footfall.
    Noticed,
    /// It heard enough to be sure someone was there.
    Heard,
    /// It heard gunfire.
    Gunfire,
    /// It saw the player.
    Spotted,
    /// It lost them.
    Lost,
    /// A shot went by it.
    ShotAt,
    /// It was hit.
    Hurt,
    /// It found one of its own, down.
    FoundBody,
    /// It was told of trouble: an alarm, a witness.
    Alarm,
}

impl Incident {
    /// How much warier it makes an NPC, as `tuning` has it.
    pub fn weight(self, tuning: &Tuning) -> f32 {
        match self {
            Incident::Noticed => tuning.noticed,
            Incident::Heard => tuning.heard,
            Incident::Gunfire => tuning.gunfire,
            Incident::Spotted => tuning.spotted,
            Incident::Lost => tuning.lost,
            Incident::ShotAt => tuning.shot_at,
            Incident::Hurt => tuning.hurt,
            Incident::FoundBody => tuning.found_body,
            Incident::Alarm => tuning.alarm,
        }
    }
}

/// How wary an NPC is from what it has been through, from 0 to 1.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Wariness {
    pub level: f32,
}

impl Wariness {
    /// It went through `incident`: warier by its weight of how far it has to go to as wary as it
    /// can be - so that one thing after another adds up, but never past it.
    pub fn note(&mut self, tuning: &Tuning, incident: Incident) {
        let weight = incident.weight(tuning).clamp(0.0, 1.0);
        self.level = 1.0 - (1.0 - self.level) * (1.0 - weight);
    }

    /// Moves it on `dt`: fading, halfway in the tuning's half-life.
    pub fn step(&mut self, tuning: &Tuning, dt: f32) {
        self.level *= 0.5f32.powf(dt / tuning.wary_half_life.max(1.0e-3));
    }

    /// How long `alert` lasts for it, in seconds, once the player is out of sight: as long as
    /// [`Alert::lasts`] says, and longer the warier it is. None for Alert.
    pub fn lasts(&self, tuning: &Tuning, alert: Alert) -> Option<f32> {
        alert.lasts(tuning).map(|lasts| lasts * (1.0 + tuning.longer * self.level))
    }

    /// How many times as keenly it looks and listens for the player as one that has been through
    /// nothing (see [`crate::perception::detection`]'s eagerness).
    pub fn keenness(&self, tuning: &Tuning) -> f32 {
        1.0 + tuning.keener * self.level
    }
}

/// How a hostile NPC is going about the player, least urgent first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Alert {
    /// It has given up searching for them, and goes about, watching for them.
    Caution,
    /// It has lost them, and is searching.
    Evasion,
    /// It can see them, and is after them.
    Alert,
}

impl Alert {
    /// The phase that follows this one, seeing the player or not, with `left` seconds left of
    /// searching or of being wary: none, once it is calm again.
    pub fn next(self, sees: bool, left: f32) -> Option<Alert> {
        Some(match self {
            _ if sees => Alert::Alert,
            Alert::Alert => Alert::Evasion,
            Alert::Evasion if left <= 0.0 => Alert::Caution,
            Alert::Caution if left <= 0.0 => return None,
            other => other,
        })
    }

    /// How long this phase lasts, in seconds, once the player is out of sight: none for Alert,
    /// which lasts as long as they are in sight.
    pub fn lasts(self, tuning: &Tuning) -> Option<f32> {
        match self {
            Alert::Alert => None,
            Alert::Evasion => Some(tuning.evasion),
            Alert::Caution => Some(tuning.caution),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_it_has_been_through_draws_out_its_search_and_fades() {
        let t = Tuning::DEFAULT;
        let mut calm = Wariness::default();
        assert_eq!(calm.lasts(&t, Alert::Caution), Some(CAUTION), "nothing happened: as long as ever");
        assert_eq!(calm.keenness(&t), 1.0);
        // A body found: it stays wary much longer, and looks the keener.
        let mut shaken = Wariness::default();
        shaken.note(&t, Incident::FoundBody);
        assert!(shaken.lasts(&t, Alert::Caution).unwrap() > 2.5 * CAUTION);
        assert!(shaken.lasts(&t, Alert::Evasion).unwrap() > 2.5 * EVASION);
        assert!(shaken.keenness(&t) > 1.7);
        assert_eq!(shaken.lasts(&t, Alert::Alert), None);
        // A glimpse barely matters; one thing after another adds up, but never past all of it.
        calm.note(&t, Incident::Noticed);
        assert!(calm.lasts(&t, Alert::Caution).unwrap() < 1.3 * CAUTION);
        for _ in 0..50 {
            calm.note(&t, Incident::Hurt);
        }
        assert!(calm.level <= 1.0 && calm.level > 0.99);
        // Over minutes it fades: halfway in its half-life.
        let was = shaken.level;
        shaken.step(&t, WARY_HALF_LIFE);
        assert!((shaken.level - was / 2.0).abs() < 1e-4);
    }

    #[test]
    fn it_searches_once_it_loses_them_gives_up_and_calms_down_in_the_end() {
        assert_eq!(Alert::Alert.next(true, 0.0), Some(Alert::Alert));
        assert_eq!(Alert::Alert.next(false, 0.0), Some(Alert::Evasion));
        assert_eq!(Alert::Evasion.next(false, 5.0), Some(Alert::Evasion));
        assert_eq!(Alert::Evasion.next(false, 0.0), Some(Alert::Caution));
        assert_eq!(Alert::Evasion.next(true, 5.0), Some(Alert::Alert));
        assert_eq!(Alert::Caution.next(false, 5.0), Some(Alert::Caution));
        assert_eq!(Alert::Caution.next(true, 5.0), Some(Alert::Alert));
        assert_eq!(Alert::Caution.next(false, 0.0), None, "calm again");
    }

    #[test]
    fn the_most_urgent_phase_is_the_greatest() {
        assert!(Alert::Alert > Alert::Evasion && Alert::Evasion > Alert::Caution);
    }
}
