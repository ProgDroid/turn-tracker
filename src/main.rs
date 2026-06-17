use std::sync::Arc;

use turn_tracker::{cleanup, registry::Registry, server};

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::init_from_env(env_logger::Env::default().default_filter_or("info"));
    let registry = Arc::new(Registry::new());
    cleanup::spawn(registry.clone());
    let bind = std::env::var("BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:8080".into());
    let static_dir = std::env::var("STATIC_DIR").unwrap_or_else(|_| "./static".into());
    if turn_tracker::ws::origin::allowed_origins_from_env().is_empty() {
        log::warn!(
            "ALLOWED_ORIGINS is unset/empty: WebSocket Origin checking is DISABLED (all origins allowed)"
        );
    }
    log::info!("turn-tracker listening on {bind}");
    server::run(registry, &bind, static_dir).await
}
