//! Durable room-state persistence: a `Store` trait and a file-backed snapshot.

pub mod snapshot;

pub use snapshot::{PlayerSnapshot, RegistrySnapshot, RoomSnapshot, SnapshotState};
