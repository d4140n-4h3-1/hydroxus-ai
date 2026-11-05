//! Social: NPCs among each other - walking about together, meeting and talking, being talked to,
//! telling a sentry of trouble and sounding the alarm, what they and their factions think of the
//! player and the rumours about them they pass on, what becomes of their own, the trouble they
//! keep away from, how they take the player passing by, and a city's crowd.
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
//! - [`faction`]: what a whole kind of NPC thinks of the player, outlasting any one of them; a
//!   deed counting by who it was done to.
//! - [`ties`]: one of a pair or group shot down before the rest - mourning it, running, or going
//!   after whoever did it.
//! - [`trouble`]: one of them found lying, an alarm, gunfire - told on, and kept away from.
//! - [`passing`]: greeting the player, tipping them off, nodding, keeping away, or glaring, as
//!   they go by.

pub mod chat;
pub mod crowd;
pub mod faction;
pub mod group;
pub mod opinion;
pub mod passing;
pub mod report;
pub mod rumour;
pub mod talk;
pub mod ties;
pub mod trouble;
