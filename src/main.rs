use std::sync::Arc;
use std::time::Duration;

use turn_tracker::persist::{FileStore, RegistrySnapshot, Store, writer};
use turn_tracker::{cleanup, registry::Registry, server};

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::init_from_env(env_logger::Env::default().default_filter_or("info"));

    let snapshot_path =
        std::env::var("TT_SNAPSHOT_PATH").unwrap_or_else(|_| "./data/rooms.json".into());
    let interval_secs = std::env::var("TT_SNAPSHOT_INTERVAL_SECS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(60);

    let store: Arc<dyn Store> = Arc::new(FileStore::new(snapshot_path));
    let snap = store.load().unwrap_or_else(|e| {
        log::error!("snapshot load failed: {e}; starting with no rooms");
        RegistrySnapshot::default()
    });
    let restored_rooms = snap.rooms.len();
    let registry = Arc::new(Registry::from_snapshot(snap));
    log::info!("restored {restored_rooms} room(s) from snapshot");

    cleanup::spawn(registry.clone());
    let (snapshotter, shutdown) = writer::spawn_snapshotter(
        registry.clone(),
        store.clone(),
        Duration::from_secs(interval_secs),
    );

    let bind = std::env::var("BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:8080".into());
    let static_dir = std::env::var("STATIC_DIR").unwrap_or_else(|_| "./static".into());
    if turn_tracker::ws::origin::allowed_origins_from_env().is_empty() {
        log::warn!(
            "ALLOWED_ORIGINS is unset/empty: WebSocket Origin checking is DISABLED (all origins allowed)"
        );
    }
    log::info!("turn-tracker listening on {bind}");

    // Runs until SIGTERM/SIGINT; Actix drains connections before returning.
    let result = server::run(registry.clone(), &bind, static_dir).await;

    // Graceful shutdown: stop the periodic writer and wait for any in-flight save
    // to finish, THEN take the final snapshot exclusively (no concurrent write).
    shutdown.notify_one();
    let _ = snapshotter.await;
    if let Err(e) = store.save(&registry.snapshot()) {
        log::error!("final snapshot save failed: {e}");
    } else {
        log::info!("final snapshot written on shutdown");
    }

    result
}
