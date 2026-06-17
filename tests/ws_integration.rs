//! End-to-end tests over a real HTTP+WS server bound to an ephemeral port.

use std::sync::Arc;

use actix_web::{App, HttpServer, web};
use futures_util::{SinkExt as _, StreamExt as _};

use turn_tracker::registry::Registry;
use turn_tracker::server;

#[allow(clippy::unused_async)] // no internal awaits, but callers use .await for consistency
async fn spawn_server() -> String {
    let registry = Arc::new(Registry::new());
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let srv = HttpServer::new(move || {
        let registry = registry.clone();
        App::new().configure(|cfg: &mut web::ServiceConfig| server::config(cfg, registry.clone()))
    })
    .listen(listener)
    .unwrap()
    .run();
    actix_web::rt::spawn(srv);
    format!("127.0.0.1:{}", addr.port())
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
