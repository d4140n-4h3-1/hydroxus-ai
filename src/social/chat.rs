//! What NPCs talk about among themselves: news of the player one has and another has not -
//! told on, so that it spreads - or else, feeling strongly about them, what it thinks of them,
//! or else nothing much. The game says it, in its own words (see [`Topic`]).

use crate::social::{
    opinion::Attitude,
    rumour::{Minds, Rumour},
};

/// How strongly an NPC has to feel about the player, either way, for it to talk about them with
/// no news.
pub const OPINIONATED: f32 = 0.3;

/// What a conversation among NPCs is about.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Topic {
    /// The `teller` tells the others what it knows of the player: `rumour`.
    News { teller: usize, rumour: Rumour },
    /// The `speaker` says what it thinks of the player, as its `attitude` has it.
    Opinion { speaker: usize, attitude: Attitude },
    /// Nothing much: the game's small talk.
    SmallTalk,
}

/// What `members` - two that meet, or a group - talk about: the strongest news one of them has
/// for another, or else what the most opinionated of them thinks of the player, if it feels
/// strongly about them, or else small talk.
pub fn topic(minds: &Minds, members: &[usize]) -> Topic {
    let news = members
        .iter()
        .flat_map(|&teller| members.iter().filter(move |&&h| h != teller).map(move |&hearer| (teller, hearer)))
        .filter_map(|(teller, hearer)| minds.news(teller, hearer).map(|rumour| (teller, rumour)))
        .max_by(|a, b| a.1.strength.total_cmp(&b.1.strength));
    if let Some((teller, rumour)) = news {
        return Topic::News { teller, rumour };
    }
    members
        .iter()
        .copied()
        .filter(|&n| minds.regard(n).abs() >= OPINIONATED)
        .max_by(|&a, &b| minds.regard(a).abs().total_cmp(&minds.regard(b).abs()))
        .map_or(Topic::SmallTalk, |speaker| Topic::Opinion { speaker, attitude: minds.attitude(speaker) })
}

/// Has what was talked about of `topic` among `members` sink in: news told to all the rest; an
/// opinion swaying all the rest.
pub fn heard(minds: &mut Minds, members: &[usize], topic: Topic) {
    match topic {
        Topic::News { teller, rumour } => {
            for &n in members.iter().filter(|&&n| n != teller) {
                minds.tell(n, rumour);
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
    use crate::social::opinion::Deed;
    use nalgebra::Vector3;

    #[test]
    fn they_talk_of_news_first_then_of_how_they_feel_and_else_of_nothing_much() {
        let mut minds = Minds::default();
        for _ in 0..4 {
            minds.add();
        }
        assert_eq!(topic(&minds, &[0, 1]), Topic::SmallTalk);
        minds.saw(&[1], Deed::Shooting, Vector3::zeros());
        let Topic::News { teller, rumour } = topic(&minds, &[0, 1, 2]) else {
            panic!("news");
        };
        assert_eq!((teller, rumour.deed), (1, Deed::Shooting));
        heard(&mut minds, &[0, 1, 2], Topic::News { teller, rumour });
        assert!(minds.regard(0) < 0.0 && minds.regard(2) < 0.0);
        // All of them know it: they talk of how they feel - the one that saw it, most.
        assert_eq!(topic(&minds, &[0, 1, 2]), Topic::Opinion { speaker: 1, attitude: Attitude::Hostile });
        // One that knows nothing and feels nothing much, with one that does not either.
        assert_eq!(topic(&minds, &[3, 3]), Topic::SmallTalk);
    }
}
