//! HTTP layer: room creation, static file serving, and the App factory.

use std::sync::Arc;
use std::time::Instant;

use actix_web::{App, HttpResponse, HttpServer, Responder, web};
use serde::Deserialize;

use crate::error::AppError;
use crate::registry::Registry;

#[derive(Deserialize)]
pub struct CreateRoomRequest {
    pub host_name: String,
}

/// POST /api/rooms — create a room, returning the code + host credentials.
///
/// # Errors
/// `AppError::InvalidRequest` for an empty host name; `AppError::CodeExhausted`
/// if no unique code is available.
#[allow(clippy::unused_async)] // required by actix-web handler signature
pub async fn create_room(
    registry: web::Data<Arc<Registry>>,
    body: web::Json<CreateRoomRequest>,
) -> Result<impl Responder, AppError> {
    let name = body.host_name.trim();
    if name.is_empty() {
        return Err(AppError::InvalidRequest);
    }
    let (code, host_id, token) = registry.create_room(name.to_owned(), Instant::now())?;
    Ok(HttpResponse::Ok().json(serde_json::json!({
        "room_code": code.0,
        "player_id": host_id.0,
        "token": token,
    })))
}

/// Build the Actix `App`. Registry is injected as shared state.
pub fn config(cfg: &mut web::ServiceConfig, registry: Arc<Registry>) {
    cfg.app_data(web::Data::new(registry))
        .route("/api/rooms", web::post().to(create_room))
        .route("/ws/{code}", web::get().to(crate::ws::connection::ws_route));
}

/// Run the HTTP server, serving the SPA build from `static_dir`.
///
/// # Errors
/// Propagates bind/IO errors from Actix.
#[allow(clippy::future_not_send)] // actix-web App uses Rc internally; HttpServer runs each worker on its own thread
pub async fn run(registry: Arc<Registry>, bind: &str, static_dir: String) -> std::io::Result<()> {
    HttpServer::new(move || {
        let registry = registry.clone();
        App::new()
            .configure(|cfg| config(cfg, registry.clone()))
            .service(actix_files::Files::new("/", &static_dir).index_file("index.html"))
    })
    .bind(bind)?
    .run()
    .await
}
