//! Background task that periodically removes idle rooms.

use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::registry::Registry;

pub const ROOM_TTL: Duration = Duration::from_hours(24);
const SWEEP_INTERVAL: Duration = Duration::from_mins(10);

/// Spawn the periodic sweep. Runs until the process exits.
pub fn spawn(registry: Arc<Registry>) {
    actix_web::rt::spawn(async move {
        let mut ticker = tokio::time::interval(SWEEP_INTERVAL);
        loop {
            ticker.tick().await;
            let removed = registry.sweep_expired(Instant::now(), ROOM_TTL);
            if removed > 0 {
                log::info!("cleanup: removed {removed} idle room(s)");
            }
        }
    });
}
