mod cleanup;
mod domain;
mod error;
mod registry;
mod server;
mod wire;
mod ws;

use std::sync::Arc;

use registry::Registry;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::init_from_env(env_logger::Env::default().default_filter_or("info"));
    let registry = Arc::new(Registry::new());
    cleanup::spawn(registry.clone());
    let bind = std::env::var("BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:8080".into());
    let static_dir = std::env::var("STATIC_DIR").unwrap_or_else(|_| "./static".into());
    log::info!("turn-tracker listening on {bind}");
    server::run(registry, &bind, static_dir).await
}
