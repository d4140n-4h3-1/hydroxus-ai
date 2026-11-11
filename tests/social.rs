//! Social awareness through the public API: a town of factions, where one NPC comes to suspect the
//! player of a shooting it didn't see, tells those it gets on with and not those it doesn't, and
//! each that hears believes it as far as it trusts the teller - until word that the player was
//! somewhere else at the time clears them for those who hear it, and only for them.

use hydroxus_ai::{
    nalgebra::Vector3,
    social::{
        faction::{Faction, Factions},
        opinion::{attitude, Attitude},
        relation::{self, Relations, Who},
        suspicion::Suspicions,
        trouble::Kind,
        word::{Trust, Word},
    },
    Tuning,
};

const AI: Tuning = Tuning::DEFAULT;
const SECURITY: Faction = 0;
const CITIZENS: Faction = 1;
const GANG: Faction = 2;
const RIVALS: Faction = 3;
const SHOOTING: u32 = 1;

fn at(x: f32) -> Vector3<f32> {
    Vector3::new(x, 0.0, 0.0)
}

fn town() -> Factions {
    let mut f = Factions::new(4, Default::default());
    f.relate(GANG, Some(RIVALS), -1.0);
    f.relate(RIVALS, Some(GANG), -1.0);
    f.relate(GANG, Some(SECURITY), -0.3);
    f
}

struct Npc {
    who: Who,
    suspicions: Suspicions,
    trust: Trust,
    regard: f32,
}

impl Npc {
    fn new(id: u32, faction: Faction) -> Self {
        Self { who: (id, Some(faction)), suspicions: Suspicions::new(), trust: Trust::new(), regard: 0.1 }
    }

    fn attitude(&self, now: f32) -> Attitude {
        attitude(&AI.opinion, self.suspicions.regard(&AI.suspicion, self.regard, now))
    }
}

/// `teller` tells `hearer` what it suspects of `id`, if it gets on with it well enough; the
/// hearer, trusting a teller it hasn't tested as far as it gets on with it, believes that much of
/// it. Whether it was told.
fn tell(town: &Factions, relations: &Relations, teller: &Npc, hearer: &mut Npc, id: u32, now: f32) -> bool {
    let r = &AI.relation;
    if !relation::shares_with(r, relations.affinity(r, town, teller.who, hearer.who, now)) {
        return false;
    }
    let Some(told) = teller.suspicions.tell(&AI.suspicion, id, now) else { return false };
    let affinity = relations.affinity(r, town, hearer.who, teller.who, now);
    hearer.trust.introduce(teller.who.0, relation::trust(r, &AI.word, affinity));
    let trust = hearer.trust.of(&AI.word, teller.who.0);
    hearer.suspicions.hear(&AI.suspicion, told, trust, now);
    true
}

#[test]
fn suspicion_spreads_along_who_gets_on_and_an_alibi_clears_it_only_for_those_who_hear_it() {
    let town = town();
    let mut relations = Relations::new();
    let mut baker = Npc::new(10, CITIZENS);
    let mut neighbour = Npc::new(11, CITIZENS);
    let mut guard = Npc::new(20, SECURITY);
    let mut camera = Npc::new(21, SECURITY);
    let mut lookout = Npc::new(30, GANG);
    let mut rival = Npc::new(40, RIVALS);

    // The baker and the lookout pass the time of day for a while: they get on.
    relations.together(&AI.relation, 10, 30, 300.0, 50.0);

    // The baker sees the player hurry past its shop; a few seconds later, shots round the corner;
    // a minute on, it hears one of them was found lying there.
    baker.suspicions.saw_player(&AI.suspicion, at(3.0), 103.0);
    let suspects = baker.suspicions.trouble(&AI.suspicion, SHOOTING, at(0.0), 104.0, Kind::Body.strength(), 160.0);
    assert!(suspects > 0.5, "{suspects}");
    assert_eq!(baker.attitude(160.0), Attitude::Wary, "it thinks the worse of them for what it suspects");
    assert_eq!(attitude(&AI.opinion, baker.regard), Attitude::Neutral, "though its regard itself is unmoved");

    // It tells its neighbour, the guard and the lookout.
    for hearer in [&mut neighbour, &mut guard, &mut lookout] {
        assert!(tell(&town, &relations, &baker, hearer, SHOOTING, 170.0));
    }
    let (n, g, l) = (
        neighbour.suspicions.of(&AI.suspicion, SHOOTING, 170.0),
        guard.suspicions.of(&AI.suspicion, SHOOTING, 170.0),
        lookout.suspicions.of(&AI.suspicion, SHOOTING, 170.0),
    );
    assert!(n > 0.0 && g > 0.0 && l > 0.0);
    assert!(n < suspects && g < suspects, "secondhand is weaker than the baker's own");
    // Its neighbour, of its own kind, believes it the sooner than the guard, a stranger.
    assert!(n > g, "neighbour {n}, guard {g}");
    // The lookout, who got to know it, believes it sooner than a stranger would.
    assert!(l > g, "lookout {l}, guard {g}");

    // The lookout won't pass it on to a rival gang's man; the rival knows nothing.
    assert!(!tell(&town, &relations, &lookout, &mut rival, SHOOTING, 175.0));
    assert_eq!(rival.suspicions.of(&AI.suspicion, SHOOTING, 175.0), 0.0);

    // A security camera had the player two hundred meters off when the shots went: it radios the
    // guard, who believes its own as far as it trusts it.
    camera.suspicions.saw_player(&AI.suspicion, at(200.0), 104.5);
    let word = Word::seen((), at(200.0), 104.5, 21).passed_on(&AI.word, 21);
    // Word that old says little of where they are now, but as much as ever of where they were
    // then: for an alibi, how far it is believed is how far the source is trusted.
    let believed = word.credence(&AI.word, &guard.trust);
    assert!(word.belief(&AI.word, &guard.trust, 180.0) < believed * 0.1 && believed > 0.5);
    let cleared = guard.suspicions.alibi(&AI.suspicion, word.at, word.seen, believed, 180.0);
    assert_eq!(cleared.len(), 1);
    assert!(guard.suspicions.of(&AI.suspicion, SHOOTING, 180.0) < g * 0.6);

    // The neighbour never heard it, and suspects them still: they are not copies of each other.
    assert!((neighbour.suspicions.of(&AI.suspicion, SHOOTING, 180.0) - n).abs() < 0.01);
    assert_eq!(neighbour.attitude(180.0), Attitude::Neutral, "told it, it suspects less than the baker");

    // Then the baker learns who really did it: it no longer suspects; nor is it warier for it.
    baker.suspicions.settle(SHOOTING);
    assert_eq!(baker.attitude(200.0), Attitude::Neutral);
}

#[test]
fn harm_turns_a_friend_into_one_kept_in_the_dark() {
    let town = town();
    let mut relations = Relations::new();
    let (mut a, mut b) = (Npc::new(1, CITIZENS), Npc::new(2, CITIZENS));
    a.suspicions.saw_player(&AI.suspicion, at(0.0), 0.0);
    a.suspicions.trouble(&AI.suspicion, SHOOTING, at(0.0), 0.0, 1.0, 0.0);
    // `b` wronged `a` - twice: `a` keeps what it knows from it, for a good while.
    relations.harmed(&AI.relation, 1, 2, 0.0);
    relations.harmed(&AI.relation, 1, 2, 0.0);
    assert!(!tell(&town, &relations, &a, &mut b, SHOOTING, 1.0));
    assert!(!tell(&town, &relations, &a, &mut b, SHOOTING, 1800.0));
    // In time it gets over it.
    let later = 4.0 * 3600.0;
    assert!(relation::shares_with(&AI.relation, relations.affinity(&AI.relation, &town, a.who, b.who, later)));
}

#[cfg(feature = "serde")]
#[test]
fn social_state_saved_and_loaded_carries_on_the_same() {
    let mut relations = Relations::new();
    relations.together(&AI.relation, 1, 2, 120.0, 10.0);
    relations.harmed(&AI.relation, 2, 3, 20.0);
    let mut s = Suspicions::new();
    s.saw_player(&AI.suspicion, at(3.0), 30.0);
    s.trouble(&AI.suspicion, 5, at(0.0), 31.0, 0.8, 40.0);
    let mut trust = Trust::new();
    trust.introduce(4, 0.9);
    trust.refuted(&AI.word, 4);

    let relations2: Relations = serde_json::from_str(&serde_json::to_string(&relations).unwrap()).unwrap();
    let mut s2: Suspicions = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
    let trust2: Trust = serde_json::from_str(&serde_json::to_string(&trust).unwrap()).unwrap();
    assert_eq!((&relations2, &s2, &trust2), (&relations, &s, &trust));

    // And carries on as the one never saved does.
    let town = town();
    for now in [50.0, 500.0, 5000.0] {
        assert_eq!(
            relations.affinity(&AI.relation, &town, (2, Some(CITIZENS)), (3, Some(GANG)), now),
            relations2.affinity(&AI.relation, &town, (2, Some(CITIZENS)), (3, Some(GANG)), now)
        );
    }
    s.alibi(&AI.suspicion, at(150.0), 31.0, 0.7, 60.0);
    s2.alibi(&AI.suspicion, at(150.0), 31.0, 0.7, 60.0);
    assert_eq!(s, s2);
}
