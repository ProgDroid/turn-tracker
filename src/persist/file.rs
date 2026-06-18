//! File-backed `Store`: atomic JSON snapshot with corrupt-file recovery.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use super::snapshot::RegistrySnapshot;
use super::{Store, StoreError};

/// Snapshot persisted as a single JSON file. Writes go to a temp file that is
/// fsync'd then renamed over the target (atomic on Linux) to avoid torn files.
#[derive(Debug, Clone)]
pub struct FileStore {
    path: PathBuf,
}

/// Append a suffix to a path's filename (e.g. `rooms.json` -> `rooms.json.tmp`).
fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut s = path.to_path_buf().into_os_string();
    s.push(suffix);
    PathBuf::from(s)
}

#[cfg(unix)]
fn restrict_permissions(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
}

#[cfg(not(unix))]
#[allow(clippy::unnecessary_wraps, clippy::missing_const_for_fn)]
fn restrict_permissions(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

impl FileStore {
    /// Create a store backed by the file at `path`.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
}

impl Store for FileStore {
    fn save(&self, snap: &RegistrySnapshot) -> Result<(), StoreError> {
        if let Some(parent) = self.path.parent()
            && !parent.as_os_str().is_empty()
        {
            fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_vec_pretty(snap)?;
        let tmp = with_suffix(&self.path, ".tmp");
        {
            let mut f = fs::File::create(&tmp)?;
            f.write_all(&json)?;
            f.sync_all()?;
        }
        restrict_permissions(&tmp)?;
        // Windows rename fails if the destination exists; remove first there.
        #[cfg(windows)]
        if self.path.exists() {
            fs::remove_file(&self.path)?;
        }
        fs::rename(&tmp, &self.path)?;
        Ok(())
    }

    fn load(&self) -> Result<RegistrySnapshot, StoreError> {
        let bytes = match fs::read(&self.path) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(RegistrySnapshot::default());
            }
            Err(e) => return Err(e.into()),
        };
        match serde_json::from_slice::<RegistrySnapshot>(&bytes) {
            Ok(snap) => Ok(snap),
            Err(e) => {
                log::error!(
                    "snapshot parse failed ({e}); backing up to .corrupt and starting fresh"
                );
                let corrupt = with_suffix(&self.path, ".corrupt");
                if let Err(e) = fs::rename(&self.path, &corrupt) {
                    log::warn!(
                        "could not back up corrupt snapshot to {}: {e}",
                        corrupt.display()
                    );
                }
                Ok(RegistrySnapshot::default())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persist::Store;
    use crate::persist::snapshot::{RoomSnapshot, SnapshotState};

    fn tmp_path(tag: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("tt-persist-{}-{}.json", std::process::id(), tag))
    }

    fn sample() -> RegistrySnapshot {
        RegistrySnapshot {
            rooms: vec![RoomSnapshot {
                code: "ABC123".into(),
                state: SnapshotState::Lobby,
                players: vec![],
                current_player_id: None,
                previous_player_id: None,
            }],
        }
    }

    #[test]
    fn save_then_load_round_trips() {
        let path = tmp_path("roundtrip");
        let _ = std::fs::remove_file(&path);
        let store = FileStore::new(&path);
        store.save(&sample()).unwrap();
        let loaded = store.load().unwrap();
        assert_eq!(loaded.rooms.len(), 1);
        assert_eq!(loaded.rooms[0].code, "ABC123");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn load_missing_file_returns_empty() {
        let path = tmp_path("missing");
        let _ = std::fs::remove_file(&path);
        let store = FileStore::new(&path);
        let loaded = store.load().unwrap();
        assert!(loaded.rooms.is_empty());
    }

    #[test]
    fn load_corrupt_file_backs_up_and_returns_empty() {
        let path = tmp_path("corrupt");
        std::fs::write(&path, b"{ this is not json").unwrap();
        let store = FileStore::new(&path);
        let loaded = store.load().unwrap();
        assert!(loaded.rooms.is_empty());

        let mut corrupt = path.clone().into_os_string();
        corrupt.push(".corrupt");
        assert!(
            std::path::Path::new(&corrupt).exists(),
            "corrupt backup not created"
        );

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(std::path::PathBuf::from(corrupt));
    }
}
