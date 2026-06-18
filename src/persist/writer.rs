//! Background task that periodically snapshots the registry when it is dirty.

use std::sync::Arc;
use std::time::Duration;

use crate::registry::Registry;

use super::Store;

/// Write a snapshot iff the registry changed since the last write. Save errors
/// are logged and swallowed so a transient disk fault never crashes the server.
pub async fn snapshot_once(registry: &Registry, store: &Arc<dyn Store>) {
    if !registry.take_dirty() {
        return;
    }
    let snap = registry.snapshot();
    let store = store.clone();
    match tokio::task::spawn_blocking(move || store.save(&snap)).await {
        Ok(Ok(())) => {}
        Ok(Err(e)) => log::error!("snapshot save failed: {e}"),
        Err(e) => log::error!("snapshot task join failed: {e}"),
    }
}

/// Spawn the periodic snapshotter. Returns the task handle so the caller can
/// abort it on shutdown before taking the final snapshot.
#[must_use]
pub fn spawn_snapshotter(
    registry: Arc<Registry>,
    store: Arc<dyn Store>,
    interval: Duration,
) -> actix_web::rt::task::JoinHandle<()> {
    actix_web::rt::spawn(async move {
        let mut ticker = tokio::time::interval(interval);
        loop {
            ticker.tick().await;
            snapshot_once(&registry, &store).await;
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persist::{RegistrySnapshot, Store, StoreError};
    use crate::registry::Registry;
    use std::sync::{Arc, Mutex};
    use std::time::Instant;

    /// In-memory store that records every save.
    struct MockStore {
        saves: Mutex<Vec<RegistrySnapshot>>,
    }

    impl Store for MockStore {
        fn save(&self, snap: &RegistrySnapshot) -> Result<(), StoreError> {
            self.saves.lock().unwrap().push(snap.clone());
            Ok(())
        }
        fn load(&self) -> Result<RegistrySnapshot, StoreError> {
            Ok(RegistrySnapshot::default())
        }
    }

    #[actix_web::test]
    async fn snapshot_once_writes_only_when_dirty() {
        let registry = Arc::new(Registry::new());
        let mock = Arc::new(MockStore {
            saves: Mutex::new(vec![]),
        });
        let store: Arc<dyn Store> = mock.clone();

        // Clean registry → no save.
        snapshot_once(&registry, &store).await;
        assert_eq!(mock.saves.lock().unwrap().len(), 0);

        // Mutate → dirty → one save capturing the room.
        let (code, _h, _t) = registry.create_room("Host".into(), Instant::now()).unwrap();
        snapshot_once(&registry, &store).await;
        {
            let saves = mock.saves.lock().unwrap();
            assert_eq!(saves.len(), 1);
            assert_eq!(saves[0].rooms.len(), 1);
            assert_eq!(saves[0].rooms[0].code, code.0);
            drop(saves);
        }

        // Flag cleared → no further save.
        snapshot_once(&registry, &store).await;
        assert_eq!(mock.saves.lock().unwrap().len(), 1);
    }
}
