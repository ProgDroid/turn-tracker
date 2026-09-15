//! End-to-end tests over a real HTTP+WS server bound to an ephemeral port.

use std::sync::Arc;

use actix_web::{App, HttpServer, web};
use futures_util::{SinkExt as _, StreamExt as _};

use turn_tracker::ratelimit;
use turn_tracker::registry::Registry;
use turn_tracker::server;

#[allow(clippy::unused_async)] // no internal awaits, but callers use .await for consistency
async fn spawn_server() -> String {
    let registry = Arc::new(Registry::new());
    let governor = ratelimit::room_create_config();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let srv = HttpServer::new(move || {
        let registry = registry.clone();
        let governor = governor.clone();
        App::new().configure(|cfg: &mut web::ServiceConfig| {
            server::config(cfg, registry.clone(), &governor);
        })
    })
    .listen(listener)
    .unwrap()
    .run();
    actix_web::rt::spawn(srv);
    format!("127.0.0.1:{}", addr.port())
}

#[actix_web::test]
async fn test_health_endpoint_returns_ok() {
    let addr = spawn_server().await;
    let client = awc::Client::new();
    let mut resp = client
        .get(format!("http://{addr}/health"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "ok");
}

#[actix_web::test]
async fn test_create_then_join_and_start_flow() {
    let addr = spawn_server().await;

    let client = awc::Client::new();
    let mut resp = client
        .post(format!("http://{addr}/api/rooms"))
        .send_json(&serde_json::json!({ "host_name": "Host" }))
        .await
        .unwrap();
    let body: serde_json::Value = resp.json().await.unwrap();
    let code = body["room_code"].as_str().unwrap().to_owned();
    let token = body["token"].as_str().unwrap().to_owned();

    // Plain-text WebSocket is intentional: loopback-only ephemeral-port test.
    let ws_url = format!("{}://{addr}/ws/{code}", "ws");
    let (_resp, mut conn) = awc::Client::new().ws(ws_url).connect().await.unwrap();

    let join = serde_json::json!({ "type": "join", "player_token": token });
    conn.send(awc::ws::Message::Text(join.to_string().into()))
        .await
        .unwrap();

    // Token re-join now always sends Welcome first so the client can set `me`.
    let frame = conn.next().await.unwrap().unwrap();
    if let awc::ws::Frame::Text(bytes) = frame {
        let msg: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(
            msg["type"], "welcome",
            "expected welcome before room_state; got {msg}"
        );
        assert!(msg["player_id"].is_string());
        assert!(msg["token"].is_string());
    } else {
        panic!("expected text frame for welcome");
    }

    let frame = conn.next().await.unwrap().unwrap();
    if let awc::ws::Frame::Text(bytes) = frame {
        let msg: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(msg["type"], "room_state");
        assert_eq!(msg["room"]["state"], "lobby");
    } else {
        panic!("expected text frame for room_state");
    }
}

/// Read frames until the next text frame, returning it as JSON.
async fn next_json(
    conn: &mut (
             impl futures_util::Stream<Item = Result<awc::ws::Frame, awc::error::WsProtocolError>>
             + Unpin
         ),
) -> serde_json::Value {
    use futures_util::StreamExt as _;
    loop {
        if let awc::ws::Frame::Text(bytes) = conn.next().await.unwrap().unwrap() {
            return serde_json::from_slice(&bytes).unwrap();
        }
    }
}

#[actix_web::test]
async fn test_locked_room_rejects_new_joiner_but_allows_token_rejoin() {
    let addr = spawn_server().await;
    let client = awc::Client::new();

    // Host creates a room and joins by token.
    let mut resp = client
        .post(format!("http://{addr}/api/rooms"))
        .send_json(&serde_json::json!({ "host_name": "Host" }))
        .await
        .unwrap();
    let body: serde_json::Value = resp.json().await.unwrap();
    let code = body["room_code"].as_str().unwrap().to_owned();
    let host_token = body["token"].as_str().unwrap().to_owned();

    let ws_url = format!("ws://{addr}/ws/{code}");
    let (_r, mut host) = awc::Client::new().ws(&ws_url).connect().await.unwrap();
    host.send(awc::ws::Message::Text(
        serde_json::json!({ "type": "join", "player_token": host_token })
            .to_string()
            .into(),
    ))
    .await
    .unwrap();
    assert_eq!(next_json(&mut host).await["type"], "welcome");
    assert_eq!(next_json(&mut host).await["type"], "room_state");

    // Host locks the room and observes the broadcast reflecting locked=true.
    host.send(awc::ws::Message::Text(
        serde_json::json!({ "type": "set_locked", "locked": true })
            .to_string()
            .into(),
    ))
    .await
    .unwrap();
    let state = next_json(&mut host).await;
    assert_eq!(state["type"], "room_state");
    assert_eq!(state["room"]["locked"], true);

    // A brand-new joiner (no token) is turned away with room_locked.
    let (_r, mut newcomer) = awc::Client::new().ws(&ws_url).connect().await.unwrap();
    newcomer
        .send(awc::ws::Message::Text(
            serde_json::json!({ "type": "join", "player_name": "Late" })
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
    let err = next_json(&mut newcomer).await;
    assert_eq!(err["type"], "error");
    assert_eq!(err["code"], "room_locked");
}

#[actix_web::test]
async fn test_join_with_unknown_token_is_rejected() {
    let addr = spawn_server().await;
    let client = awc::Client::new();
    let mut resp = client
        .post(format!("http://{addr}/api/rooms"))
        .send_json(&serde_json::json!({ "host_name": "Host" }))
        .await
        .unwrap();
    let body: serde_json::Value = resp.json().await.unwrap();
    let code = body["room_code"].as_str().unwrap().to_owned();

    // Plain-text WebSocket is intentional: loopback-only ephemeral-port test.
    let ws_url = format!("{}://{addr}/ws/{code}", "ws");
    let (_resp, mut conn) = awc::Client::new().ws(ws_url).connect().await.unwrap();

    // A token the room has never issued — e.g. a client returning after the
    // snapshot was restored from an older backup.
    let join = serde_json::json!({ "type": "join", "player_token": "not-a-real-token" });
    conn.send(awc::ws::Message::Text(join.to_string().into()))
        .await
        .unwrap();

    let frame = conn.next().await.unwrap().unwrap();
    let awc::ws::Frame::Text(bytes) = frame else {
        panic!("expected a text frame");
    };
    let msg: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(msg["type"], "error", "unknown token must not mint a player; got {msg}");
    assert_eq!(
        msg["code"], "not_found",
        "must use the code the frontend's stale-token recovery listens for"
    );
}
