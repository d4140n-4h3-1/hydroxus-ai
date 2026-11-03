//! Social: NPCs among each other - walking about together, meeting and talking, being talked to,
//! telling a sentry of trouble and sounding the alarm, and a city's crowd.
//!
//! - [`group`]: pairs and groups - who is in which, where each keeps about the one leading it,
//!   who speaks for it, who leads it on once the one that did is gone, and waiting for those
//!   kept or fallen behind.
//! - [`talk`]: who the player can talk to; two that meet and stop to talk; a group talking among
//!   itself; how far off what is said is heard.
//! - [`report`]: a witness going to tell a sentry, or hiding with none about; who hears an alarm.
//! - [`crowd`]: a city's crowd, always about the player, turning up again out of sight.

pub mod crowd;
pub mod group;
pub mod report;
pub mod talk;
