//! Domain module — pure business types. Dead-code allowed: items are pub API
//! consumed by later tasks (wire layer, room state machine) but not yet wired
//! into main.rs.
#![allow(dead_code)]

pub mod ids;
pub mod player;
pub mod room;
