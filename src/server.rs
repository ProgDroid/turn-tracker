//! HTTP layer: room creation, static file serving, and the App factory.

use std::sync::Arc;
use std::time::Instant;

use actix_files::{Files, NamedFile};
use actix_web::dev::{ServiceRequest, ServiceResponse, fn_service};
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

/// Static-file service for the SPA with `index.html` fallback for deep links.
///
/// Serves files from `static_dir`. Any unmatched path (e.g. `/room/ABC123`)
/// falls back to `index.html` so client-side routes survive a hard refresh.
/// Explicit `/api/*` and `/ws/{code}` routes are registered before this
/// service, so only genuine client routes reach the fallback.
#[must_use]
pub fn spa_files(static_dir: &str) -> Files {
    let index = std::path::Path::new(static_dir).join("index.html");
    Files::new("/", static_dir)
        .index_file("index.html")
        .default_handler(fn_service(move |req: ServiceRequest| {
            let index = index.clone();
            async move {
                let (req, _) = req.into_parts();
                let file = NamedFile::open_async(&index)
                    .await
                    .map_err(actix_web::error::ErrorInternalServerError)?;
                let res = file.into_response(&req);
                Ok::<ServiceResponse, actix_web::Error>(ServiceResponse::new(req, res))
            }
        }))
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
            .service(spa_files(&static_dir))
    })
    .bind(bind)?
    .run()
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{App, test};
    use std::io::Write;

    fn tmp_static_with_index() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("tt-static-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut f = std::fs::File::create(dir.join("index.html")).unwrap();
        f.write_all(b"<!doctype html><title>SPA</title>").unwrap();
        dir
    }

    #[actix_web::test]
    async fn deep_link_falls_back_to_index_html() {
        let dir = tmp_static_with_index();
        let static_dir = dir.to_string_lossy().to_string();
        let app = test::init_service(App::new().service(spa_files(&static_dir))).await;

        let req = test::TestRequest::get().uri("/room/ABC123").to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());
        let body = test::read_body(resp).await;
        assert!(
            String::from_utf8_lossy(&body).contains("SPA"),
            "fallback did not return index.html"
        );
    }
}
