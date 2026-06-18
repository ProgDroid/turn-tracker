//! Durable room-state persistence: a `Store` trait and a file-backed snapshot.

pub mod file;
pub mod snapshot;

pub use file::FileStore;
pub use snapshot::{PlayerSnapshot, RegistrySnapshot, RoomSnapshot, SnapshotState};

use thiserror::Error;

/// Errors raised while saving or loading a snapshot.
#[derive(Debug, Error)]
pub enum StoreError {
    #[error("snapshot io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("snapshot serialization error: {0}")]
    Serde(#[from] serde_json::Error),
}

/// A backend that can persist and restore the full room registry.
pub trait Store: Send + Sync {
    /// Persist a snapshot.
    ///
    /// # Errors
    /// `StoreError::Io` on a filesystem failure; `StoreError::Serde` if the
    /// snapshot cannot be serialized.
    fn save(&self, snap: &RegistrySnapshot) -> Result<(), StoreError>;

    /// Load the most recent snapshot, or an empty one if none exists.
    ///
    /// # Errors
    /// `StoreError::Io` on an unreadable file. A corrupt/unparseable file is
    /// NOT an error: it is backed up and an empty snapshot is returned.
    fn load(&self) -> Result<RegistrySnapshot, StoreError>;
}
