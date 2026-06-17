//! HTTP layer: room creation, static file serving, and the App factory.

use std::sync::Arc;
use std::time::Instant;

use actix_files::{Files, NamedFile};
use actix_web::dev::{ServiceRequest, ServiceResponse, fn_service};
use actix_web::{App, HttpResponse, HttpServer, Responder, web};
use serde::Deserialize;

use crate::domain::player::{self, NameError};
use crate::error::AppError;
use crate::registry::Registry;

/// GET /health — liveness probe. Always returns 200 with `{"status":"ok"}`.
#[allow(clippy::unused_async)] // required by actix-web handler signature
pub async fn health() -> impl Responder {
    HttpResponse::Ok().json(serde_json::json!({ "status": "ok" }))
}

#[derive(Deserialize)]
pub struct CreateRoomRequest {
    pub host_name: String,
}

/// POST /api/rooms — create a room, returning the code + host credentials.
///
/// # Errors
/// `AppError::InvalidRequest` for an empty host name; `AppError::NameTooLong`
/// if the name exceeds the length bound; `AppError::CodeExhausted` if no unique
/// code is available; `AppError::RoomCapacityReached` if the server is full.
#[allow(clippy::unused_async)] // required by actix-web handler signature
pub async fn create_room(
    registry: web::Data<Arc<Registry>>,
    body: web::Json<CreateRoomRequest>,
) -> Result<impl Responder, AppError> {
    let name = match player::validate_name(&body.host_name) {
        Ok(name) => name,
        Err(NameError::Empty) => return Err(AppError::InvalidRequest),
        Err(NameError::TooLong) => return Err(AppError::NameTooLong),
    };
    let (code, host_id, token) = registry.create_room(name, Instant::now())?;
    Ok(HttpResponse::Ok().json(serde_json::json!({
        "room_code": code.0,
        "player_id": host_id.0,
        "token": token,
    })))
}

/// Build the Actix `App`. Registry is injected as shared state.
pub fn config(cfg: &mut web::ServiceConfig, registry: Arc<Registry>) {
    cfg.app_data(web::Data::new(registry))
        .route("/health", web::get().to(health))
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

    fn test_app_config() -> impl Fn(&mut web::ServiceConfig) + Clone + Send + 'static {
        move |cfg: &mut web::ServiceConfig| config(cfg, Arc::new(Registry::new()))
    }

    #[actix_web::test]
    async fn health_returns_ok_status_json() {
        let app = test::init_service(App::new().configure(test_app_config())).await;
        let req = test::TestRequest::get().uri("/health").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
        let body = test::read_body(resp).await;
        assert_eq!(&body[..], br#"{"status":"ok"}"#);
    }

    #[actix_web::test]
    async fn create_room_accepts_name_at_max_len() {
        let app = test::init_service(App::new().configure(test_app_config())).await;
        let name = "a".repeat(crate::domain::player::MAX_NAME_LEN);
        let req = test::TestRequest::post()
            .uri("/api/rooms")
            .set_json(serde_json::json!({ "host_name": name }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200, "40-char name should be accepted");
    }

    #[actix_web::test]
    async fn create_room_rejects_name_one_over_max_len() {
        let app = test::init_service(App::new().configure(test_app_config())).await;
        let name = "a".repeat(crate::domain::player::MAX_NAME_LEN + 1);
        let req = test::TestRequest::post()
            .uri("/api/rooms")
            .set_json(serde_json::json!({ "host_name": name }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 400, "41-char name should be rejected");
    }

    #[actix_web::test]
    async fn create_room_rejects_empty_name() {
        let app = test::init_service(App::new().configure(test_app_config())).await;
        let req = test::TestRequest::post()
            .uri("/api/rooms")
            .set_json(serde_json::json!({ "host_name": "   " }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 400);
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
