//! HTTP layer: room creation, static file serving, and the App factory.

use std::sync::Arc;
use std::time::Instant;

use actix_files::{Files, NamedFile};
use actix_governor::Governor;
use actix_web::dev::{ServiceRequest, ServiceResponse, fn_service};
use actix_web::middleware::DefaultHeaders;
use actix_web::{App, HttpResponse, HttpServer, Responder, web};
use serde::Deserialize;

use crate::domain::player::{self, NameError};
use crate::error::AppError;
use crate::gate;
use crate::ratelimit::{self, RealIpKeyExtractor};
use crate::registry::Registry;
use actix_governor::GovernorConfig;
use actix_governor::governor::middleware::NoOpMiddleware;

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
/// `AppError::CreateForbidden` if the injected [`gate::CreateToken`] is set and
/// the `X-Create-Token` header does not match; `AppError::InvalidRequest` for an
/// empty host name; `AppError::NameTooLong` if the name exceeds the length
/// bound; `AppError::CodeExhausted` if no unique code is available;
/// `AppError::RoomCapacityReached` if the server is full.
#[allow(clippy::unused_async)] // required by actix-web handler signature
#[allow(clippy::future_not_send)]
// actix-web's HttpRequest is not Send; handler runs on actix's single-threaded runtime
pub async fn create_room(
    req: actix_web::HttpRequest,
    registry: web::Data<Arc<Registry>>,
    create_token: web::Data<gate::CreateToken>,
    body: web::Json<CreateRoomRequest>,
) -> Result<impl Responder, AppError> {
    // Runs after the rate limiter (wrapped on the resource), so guessing the
    // token is itself throttled.
    let provided = req
        .headers()
        .get("X-Create-Token")
        .and_then(|v| v.to_str().ok());
    if !gate::create_allowed(provided, create_token.expected()) {
        log::warn!("room creation rejected: missing or wrong create token");
        return Err(AppError::CreateForbidden);
    }

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

/// Build the Actix `App`.
///
/// Registry is injected as shared state. `governor` is the shared per-client
/// rate-limit config applied to room creation; pass the SAME config into every
/// worker so they share one set of buckets. `create_token` is the resolved
/// creation gate (see [`gate::CreateToken`]), passed in rather than read from
/// the environment per request.
pub fn config(
    cfg: &mut web::ServiceConfig,
    registry: Arc<Registry>,
    governor: &GovernorConfig<RealIpKeyExtractor, NoOpMiddleware>,
    create_token: gate::CreateToken,
) {
    cfg.app_data(web::Data::new(registry))
        .app_data(web::Data::new(create_token))
        .route("/health", web::get().to(health))
        .service(
            web::resource("/api/rooms")
                .wrap(Governor::new(governor))
                .route(web::post().to(create_room)),
        )
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

/// Content-Security-Policy for the self-contained SPA. The Vite build emits only
/// external (same-origin) scripts/styles, so `script-src 'self'` is safe; Vue
/// injects dynamic inline styles at runtime, so `style-src` allows `unsafe-inline`.
/// `connect-src 'self'` covers the same-origin WebSocket.
const CONTENT_SECURITY_POLICY: &str = "default-src 'self'; script-src 'self'; \
     style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src 'self'; \
     font-src 'self'; object-src 'none'; base-uri 'self'; frame-ancestors 'none'";

/// Baseline security response headers applied to every response.
#[must_use]
pub fn security_headers() -> DefaultHeaders {
    DefaultHeaders::new()
        .add(("X-Content-Type-Options", "nosniff"))
        .add(("X-Frame-Options", "DENY"))
        .add(("Referrer-Policy", "no-referrer"))
        .add(("Content-Security-Policy", CONTENT_SECURITY_POLICY))
}

/// Run the HTTP server, serving the SPA build from `static_dir`.
///
/// # Errors
/// Propagates bind/IO errors from Actix.
#[allow(clippy::future_not_send)] // actix-web App uses Rc internally; HttpServer runs each worker on its own thread
pub async fn run(registry: Arc<Registry>, bind: &str, static_dir: String) -> std::io::Result<()> {
    // Build the rate-limit config ONCE so every worker shares the same buckets.
    let governor = ratelimit::room_create_config();
    // Resolve the creation gate ONCE, at startup, instead of on every request.
    let create_token = gate::CreateToken::from_env();
    HttpServer::new(move || {
        let registry = registry.clone();
        let governor = governor.clone();
        let create_token = create_token.clone();
        App::new()
            .wrap(security_headers())
            .configure(|cfg| config(cfg, registry.clone(), &governor, create_token.clone()))
            .service(spa_files(&static_dir))
    })
    // Actix force-stops workers only after this timeout. It defaults to 30s,
    // and the heartbeat keeps WebSockets healthy right through shutdown, so a
    // single connected phone would hold `run()` open past Docker Compose's
    // 10s default `stop_grace_period` — the container is SIGKILLed and the
    // final snapshot in `main` never runs. Five seconds drains comfortably and
    // leaves the rest of the grace period for the snapshot write.
    .shutdown_timeout(5)
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
        move |cfg: &mut web::ServiceConfig| {
            config(
                cfg,
                Arc::new(Registry::new()),
                &ratelimit::room_create_config(),
                gate::CreateToken::default(),
            );
        }
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
    async fn responses_carry_security_headers() {
        let app = test::init_service(
            App::new()
                .wrap(security_headers())
                .configure(test_app_config()),
        )
        .await;
        let req = test::TestRequest::get().uri("/health").to_request();
        let resp = test::call_service(&app, req).await;
        let headers = resp.headers();
        assert_eq!(headers.get("X-Content-Type-Options").unwrap(), "nosniff");
        assert_eq!(headers.get("X-Frame-Options").unwrap(), "DENY");
        assert!(
            headers.get("Content-Security-Policy").is_some(),
            "CSP header must be present"
        );
    }

    #[actix_web::test]
    async fn create_room_is_rate_limited_after_burst() {
        // One app, one shared limiter; all requests share the "unknown" bucket
        // (TestRequest has no peer/XFF). Burst is ROOM_CREATE_BURST (10).
        let app = test::init_service(App::new().configure(test_app_config())).await;
        let make = || {
            test::TestRequest::post()
                .uri("/api/rooms")
                .set_json(serde_json::json!({ "host_name": "Host" }))
                .to_request()
        };
        // Burst requests succeed.
        for i in 0..10 {
            let resp = test::call_service(&app, make()).await;
            assert_eq!(resp.status(), 200, "request {i} within burst should pass");
        }
        // The next one is throttled (429).
        let resp = test::call_service(&app, make()).await;
        assert_eq!(
            resp.status(),
            429,
            "request past the burst should be rate-limited"
        );
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
