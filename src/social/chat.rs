//! What NPCs talk about among themselves: news of the player one has and another has not -
//! told on, so that it spreads - or else of trouble about town, or else, feeling strongly about
//! the player, what it thinks of them, or else nothing much. The game says it, in its own words (see [`Topic`]).

use crate::social::{
    opinion::Attitude,
    rumour::{Minds, Rumour},
    trouble::Trouble,
};

/// How strongly an NPC has to feel about the player, either way, for it to talk about them with
/// no news.
pub const OPINIONATED: f32 = 0.3;

tuning!(
    /// What NPCs talk about among themselves.
    {
        /// How strongly one has to feel about the player, either way, to talk of them.
        opinionated: f32 = OPINIONATED,
    }
);

/// What a conversation among NPCs is about.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Topic {
    /// The `teller` tells the others what it knows of the player: `rumour`.
    News { teller: usize, rumour: Rumour },
    /// The `teller` tells the others of `trouble` about town.
    Trouble { teller: usize, trouble: Trouble },
    /// The `speaker` says what it thinks of the player, as its `attitude` has it.
    Opinion { speaker: usize, attitude: Attitude },
    /// Nothing much: the game's small talk.
    SmallTalk,
}

/// What `members` - two that meet, or a group - talk about: the strongest news of the player one
/// of them has for another, or else the strongest of trouble, or else what the most opinionated of
/// them thinks of the player, if it feels strongly about them, or else small talk.
pub fn topic(tuning: &Tuning, minds: &Minds, members: &[usize]) -> Topic {
    let news = members
        .iter()
        .flat_map(|&teller| members.iter().filter(move |&&h| h != teller).map(move |&hearer| (teller, hearer)))
        .filter_map(|(teller, hearer)| minds.news(teller, hearer).map(|rumour| (teller, rumour)))
        .max_by(|a, b| a.1.strength.total_cmp(&b.1.strength));
    if let Some((teller, rumour)) = news {
        return Topic::News { teller, rumour };
    }
    let trouble = members
        .iter()
        .flat_map(|&teller| members.iter().filter(move |&&h| h != teller).map(move |&hearer| (teller, hearer)))
        .filter_map(|(teller, hearer)| minds.trouble_news(teller, hearer).map(|trouble| (teller, trouble)))
        .max_by(|a, b| a.1.strength.total_cmp(&b.1.strength));
    if let Some((teller, trouble)) = trouble {
        return Topic::Trouble { teller, trouble };
    }
    members
        .iter()
        .copied()
        .filter(|&n| minds.regard(n).abs() >= tuning.opinionated)
        .max_by(|&a, &b| minds.regard(a).abs().total_cmp(&minds.regard(b).abs()))
        .map_or(Topic::SmallTalk, |speaker| Topic::Opinion { speaker, attitude: minds.attitude(speaker) })
}

/// Has what was talked about of `topic` among `members` sink in: news, or trouble, told to all the
/// rest; an opinion swaying all the rest.
pub fn heard(minds: &mut Minds, members: &[usize], topic: Topic) {
    match topic {
        Topic::News { teller, rumour } => {
            for &n in members.iter().filter(|&&n| n != teller) {
                minds.tell(n, rumour);
            }
        }
        Topic::Trouble { teller, trouble } => {
            for &n in members.iter().filter(|&&n| n != teller) {
                minds.tell_trouble(n, trouble);
            }
        }
        Topic::Opinion { speaker, .. } => {
            for &n in members.iter().filter(|&&n| n != speaker) {
                minds.share_regard(speaker, n);
            }
        }
        Topic::SmallTalk => (),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::social::{opinion::Deed, trouble::Kind};
    use nalgebra::Vector3;

    #[test]
    fn they_talk_of_news_first_then_of_how_they_feel_and_else_of_nothing_much() {
        let mut minds = Minds::default();
        for _ in 0..4 {
            minds.add();
        }
        assert_eq!(topic(&Tuning::DEFAULT, &minds, &[0, 1]), Topic::SmallTalk);
        minds.saw(&[1], Deed::Shooting, Vector3::zeros());
        let Topic::News { teller, rumour } = topic(&Tuning::DEFAULT, &minds, &[0, 1, 2]) else {
            panic!("news");
        };
        assert_eq!((teller, rumour.deed), (1, Deed::Shooting));
        heard(&mut minds, &[0, 1, 2], Topic::News { teller, rumour });
        assert!(minds.regard(0) < 0.0 && minds.regard(2) < 0.0);
        // All of them know it: they talk of how they feel - the one that saw it, most.
        assert_eq!(topic(&Tuning::DEFAULT, &minds, &[0, 1, 2]), Topic::Opinion { speaker: 1, attitude: Attitude::Hostile });
        // One that knows nothing and feels nothing much, with one that does not either.
        assert_eq!(topic(&Tuning::DEFAULT, &minds, &[3, 3]), Topic::SmallTalk);
    }

    #[test]
    fn trouble_comes_after_news_of_the_player_and_before_how_they_feel() {
        let mut minds = Minds::default();
        for _ in 0..3 {
            minds.add();
        }
        minds.came_across(&[0], Kind::Body, Vector3::zeros());
        let Topic::Trouble { teller, trouble } = topic(&Tuning::DEFAULT, &minds, &[0, 1]) else {
            panic!("trouble");
        };
        assert_eq!((teller, trouble.kind), (0, Kind::Body));
        heard(&mut minds, &[0, 1], Topic::Trouble { teller, trouble });
        assert_eq!(minds.mind(1).unwrap().troubles().len(), 1);
        minds.saw(&[2], Deed::Robbing, Vector3::zeros());
        assert!(matches!(topic(&Tuning::DEFAULT, &minds, &[0, 2]), Topic::News { teller: 2, .. }), "the player first");
    }
}
