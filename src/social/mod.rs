//! Social: NPCs among each other - walking about together, meeting and talking, being talked to,
//! telling a sentry of trouble and sounding the alarm, what they think of the player and the
//! rumours about them they pass on, and a city's crowd.
//!
//! - [`group`]: pairs and groups - who is in which, where each keeps about the one leading it,
//!   who speaks for it, who leads it on once the one that did is gone, and waiting for those
//!   kept or fallen behind.
//! - [`talk`]: who the player can talk to; two that meet and stop to talk; a group talking among
//!   itself; how far off what is said is heard.
//! - [`report`]: a witness going to tell a sentry, or hiding with none about; who hears an alarm.
//! - [`opinion`]: what an NPC thinks of the player, from what it saw them do or heard they did.
//! - [`rumour`]: what each NPC knows of the player, told on, weaker each time, and fading.
//! - [`chat`]: what NPCs talk about among themselves: news, their opinion, or nothing much.
//! - [`crowd`]: a city's crowd, always about the player, turning up again out of sight.

pub mod chat;
pub mod crowd;
pub mod group;
pub mod opinion;
pub mod report;
pub mod rumour;
pub mod talk;
