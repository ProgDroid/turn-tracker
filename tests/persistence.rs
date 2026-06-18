//! Full persistence pathway: save a live registry, reload it through the
//! `FileStore`, and confirm a player can reconnect by token.

use std::time::Instant;

use turn_tracker::persist::{FileStore, Store};
use turn_tracker::registry::Registry;

fn tmp_path(tag: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("tt-e2e-{}-{}.json", std::process::id(), tag))
}

#[test]
fn room_survives_save_and_reload_and_is_reconnectable() {
    let path = tmp_path("reload");
    let _ = std::fs::remove_file(&path);
    let store = FileStore::new(&path);

    // Build state and start a game so turn pointers are non-trivial.
    let original = Registry::new();
    let now = Instant::now();
    let (code, host_id, host_token) = original.create_room("Host".into(), now).unwrap();
    original
        .with_room_mut(&code.0, |room| {
            room.add_player("Bob".into(), now);
            room.start_game(&host_id, now).unwrap();
            ((), vec![], true)
        })
        .unwrap();

    // Persist, then reload into a brand-new registry.
    store.save(&original.snapshot()).unwrap();
    let restored = Registry::from_snapshot(store.load().unwrap());

    assert!(restored.contains(&code.0), "room must survive reload");
    restored
        .with_room_mut(&code.0, |room| {
            assert_eq!(room.players.len(), 2, "both players persisted");
            assert_eq!(
                room.current_player_id,
                Some(host_id.clone()),
                "turn pointer persisted"
            );
            let host = room
                .player_by_token(&host_token)
                .expect("host reconnectable by token");
            assert!(!host.connected, "players start disconnected after reload");
            ((), vec![], false)
        })
        .unwrap();

    let _ = std::fs::remove_file(&path);
}
