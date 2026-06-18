//! Single-instance guard: an exclusive OS advisory lock on the data directory.
//!
//! Two processes writing the same snapshot file race on the temp-file rename and
//! silently clobber each other's rooms. At startup we take an exclusive lock on a
//! `.lock` file beside the snapshot; a second instance then fails fast instead of
//! corrupting state. The OS releases the lock when the holding process exits —
//! including on a crash — so there is no stale lock to clean up by hand.
//!
//! Uses `std::fs::File::{try_lock}` (file locking stabilized in Rust 1.89), so no
//! external locking crate is needed.

use std::fs::{self, File, TryLockError};
use std::path::{Path, PathBuf};

/// Path of the lockfile guarding the data directory that holds `snapshot_path`.
fn lock_path_for(snapshot_path: &Path) -> PathBuf {
    snapshot_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map_or_else(|| PathBuf::from(".lock"), |dir| dir.join(".lock"))
}

/// Acquire the exclusive instance lock for the data directory holding
/// `snapshot_path`, creating the directory if needed.
///
/// The returned [`File`] MUST be kept alive for the whole process lifetime:
/// dropping it (or closing the process) releases the lock.
///
/// # Errors
/// `io::ErrorKind::WouldBlock` if another instance already holds the lock; other
/// `io::Error`s if the data directory or lockfile can't be created/opened.
pub fn acquire_data_lock(snapshot_path: &Path) -> std::io::Result<File> {
    if let Some(dir) = snapshot_path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(dir)?;
    }
    let file = File::create(lock_path_for(snapshot_path))?;
    // Non-blocking exclusive lock: WouldBlock => someone else owns the data dir.
    match file.try_lock() {
        Ok(()) => Ok(file),
        Err(TryLockError::WouldBlock) => Err(std::io::ErrorKind::WouldBlock.into()),
        Err(TryLockError::Error(e)) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_snapshot(tag: &str) -> PathBuf {
        std::env::temp_dir()
            .join(format!("tt-lock-{}-{}", std::process::id(), tag))
            .join("rooms.json")
    }

    #[test]
    fn second_acquire_is_rejected_while_first_is_held() {
        let path = tmp_snapshot("contend");
        let _ = fs::remove_dir_all(path.parent().unwrap());

        let first = acquire_data_lock(&path).expect("first lock acquired");
        let second = acquire_data_lock(&path);
        assert!(
            matches!(&second, Err(e) if e.kind() == std::io::ErrorKind::WouldBlock),
            "second instance must be rejected with WouldBlock, got {second:?}"
        );

        // Releasing the first lets a later instance acquire it.
        drop(first);
        let third = acquire_data_lock(&path).expect("lock acquirable after release");
        drop(third);
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn lock_path_sits_beside_the_snapshot() {
        let p = Path::new("/var/data/rooms.json");
        assert_eq!(lock_path_for(p), Path::new("/var/data/.lock"));
        // A bare filename (no parent dir) locks in the current directory.
        assert_eq!(lock_path_for(Path::new("rooms.json")), Path::new(".lock"));
    }
}
