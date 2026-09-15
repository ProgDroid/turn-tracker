//! End-to-end tests over a real HTTP+WS server bound to an ephemeral port.

use std::sync::Arc;

use actix_web::{App, HttpServer, web};
use futures_util::{SinkExt as _, StreamExt as _};

use turn_tracker::gate::CreateToken;
use turn_tracker::ratelimit;
use turn_tracker::registry::Registry;
use turn_tracker::server;

/// Spawn a server with the room-creation gate OFF.
async fn spawn_server() -> String {
    spawn_server_gated(None).await
}

/// Spawn a server with the room-creation gate set to `create_token`.
///
/// The gate is injected rather than read from `TT_CREATE_TOKEN`, so an enabled
/// gate is testable without `std::env::set_var` (`unsafe` under edition 2024,
/// and racy across concurrently running tests).
#[allow(clippy::unused_async)] // no internal awaits, but callers use .await for consistency
async fn spawn_server_gated(create_token: Option<&str>) -> String {
    let registry = Arc::new(Registry::new());
    let governor = ratelimit::room_create_config();
    let create_token = CreateToken::new(create_token.map(str::to_owned));
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let srv = HttpServer::new(move || {
        let registry = registry.clone();
        let governor = governor.clone();
        let create_token = create_token.clone();
        App::new().configure(move |cfg: &mut web::ServiceConfig| {
            server::config(cfg, registry.clone(), &governor, create_token.clone());
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

    // An UNKNOWN token presented to a locked room is `not_found`, not
    // `room_locked`. The lock check never runs: an unrecognised token means
    // "your identity is gone", which is what drives the client down the
    // clear-token-and-re-prompt path rather than showing "room is locked".
    let (_r, mut stale) = awc::Client::new().ws(&ws_url).connect().await.unwrap();
    stale.send(join_by_token("not-a-real-token")).await.unwrap();
    let err = next_json(&mut stale).await;
    assert_eq!(err["type"], "error");
    assert_eq!(
        err["code"], "not_found",
        "a stale token must send the client to re-prompt, not to the lock message; got {err}"
    );
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
    assert_eq!(
        msg["type"], "error",
        "unknown token must not mint a player; got {msg}"
    );
    assert_eq!(
        msg["code"], "not_found",
        "must use the code the frontend's stale-token recovery listens for"
    );
}

#[actix_web::test]
async fn test_create_room_is_open_when_no_token_configured() {
    // The env var is not set in the test process, so the gate is off and the
    // existing E2E suite keeps working unchanged.
    let addr = spawn_server().await;
    let resp = awc::Client::new()
        .post(format!("http://{addr}/api/rooms"))
        .send_json(&serde_json::json!({ "host_name": "Host" }))
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
}

#[actix_web::test]
async fn test_stray_create_token_header_does_not_break_an_ungated_server() {
    let addr = spawn_server().await;
    let resp = awc::Client::new()
        .post(format!("http://{addr}/api/rooms"))
        .insert_header(("X-Create-Token", "irrelevant"))
        .send_json(&serde_json::json!({ "host_name": "Host" }))
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        200,
        "with TT_CREATE_TOKEN unset, a stray header must be ignored"
    );
}

/// A `join` frame carrying an existing player's secret token.
fn join_by_token(token: &str) -> awc::ws::Message {
    awc::ws::Message::Text(
        serde_json::json!({ "type": "join", "player_token": token })
            .to_string()
            .into(),
    )
}

#[actix_web::test]
async fn test_stale_connection_cleanup_does_not_disconnect_a_reattached_player() {
    let addr = spawn_server().await;
    let client = awc::Client::new();
    let mut resp = client
        .post(format!("http://{addr}/api/rooms"))
        .send_json(&serde_json::json!({ "host_name": "Host" }))
        .await
        .unwrap();
    let body: serde_json::Value = resp.json().await.unwrap();
    let code = body["room_code"].as_str().unwrap().to_owned();
    let host_token = body["token"].as_str().unwrap().to_owned();

    // Plain-text WebSocket is intentional: loopback-only ephemeral-port test.
    let ws_url = format!("{}://{addr}/ws/{code}", "ws");

    // Connection A: the socket that will later go away (carrier NAT drop, then
    // a heartbeat timeout — indistinguishable from a close as far as cleanup
    // is concerned).
    let (_r, mut first) = awc::Client::new().ws(&ws_url).connect().await.unwrap();
    first.send(join_by_token(&host_token)).await.unwrap();
    assert_eq!(next_json(&mut first).await["type"], "welcome");

    // Connection B: the same player re-attaches by token while A is still open.
    let (_r, mut second) = awc::Client::new().ws(&ws_url).connect().await.unwrap();
    second.send(join_by_token(&host_token)).await.unwrap();
    let welcome = next_json(&mut second).await;
    assert_eq!(welcome["type"], "welcome");
    let player_id = welcome["player_id"].as_str().unwrap().to_owned();

    // A now departs. Its cleanup must not clear `connected` on a player whose
    // live connection is B.
    first.send(awc::ws::Message::Close(None)).await.unwrap();
    drop(first);
    actix_web::rt::time::sleep(std::time::Duration::from_millis(300)).await;

    // Provoke a fresh authoritative RoomState and inspect what it says about B.
    second
        .send(awc::ws::Message::Text(
            serde_json::json!({ "type": "set_locked", "locked": true })
                .to_string()
                .into(),
        ))
        .await
        .unwrap();

    loop {
        let msg = next_json(&mut second).await;
        if msg["type"] == "room_state" && msg["room"]["locked"] == true {
            let me = msg["room"]["players"]
                .as_array()
                .unwrap()
                .iter()
                .find(|p| p["id"] == player_id.as_str())
                .expect("the re-attached player must still be in the room");
            assert_eq!(
                me["connected"], true,
                "the departing connection must not mark a re-attached player disconnected; got {msg}"
            );
            break;
        }
    }
}

// --- room-creation gate, ENABLED (spec §8: "set + correct ⇒ 200, set + wrong
// ⇒ 403"). The unset case is covered above. ---

/// Post a room-creation request, optionally presenting `X-Create-Token`.
/// Returns the status code and the parsed JSON body.
#[allow(clippy::future_not_send)]
// awc's ClientResponse is not Send; these run on actix's single-threaded test runtime
async fn post_create_room(addr: &str, token: Option<&str>) -> (u16, serde_json::Value) {
    let req = awc::Client::new().post(format!("http://{addr}/api/rooms"));
    let req = match token {
        Some(t) => req.insert_header(("X-Create-Token", t)),
        None => req,
    };
    let mut resp = req
        .send_json(&serde_json::json!({ "host_name": "Host" }))
        .await
        .unwrap();
    let status = resp.status().as_u16();
    let body: serde_json::Value = resp.json().await.unwrap();
    (status, body)
}

#[actix_web::test]
async fn test_gated_create_room_accepts_the_correct_token() {
    let addr = spawn_server_gated(Some("s3cret-token")).await;
    let (status, body) = post_create_room(&addr, Some("s3cret-token")).await;
    assert_eq!(status, 200, "a matching X-Create-Token must be accepted");
    assert!(body["room_code"].is_string(), "got {body}");
}

#[actix_web::test]
async fn test_gated_create_room_rejects_a_wrong_token() {
    let addr = spawn_server_gated(Some("s3cret-token")).await;
    let (status, body) = post_create_room(&addr, Some("wrong-token")).await;
    assert_eq!(status, 403, "a wrong X-Create-Token must be rejected");
    assert!(
        body["error"].is_string(),
        "403 carries the standard HTTP error envelope; got {body}"
    );
}

#[actix_web::test]
async fn test_gated_create_room_rejects_a_missing_token() {
    let addr = spawn_server_gated(Some("s3cret-token")).await;
    let (status, _body) = post_create_room(&addr, None).await;
    assert_eq!(
        status, 403,
        "an absent X-Create-Token must be rejected when the gate is on"
    );
}

/// Send a `join` with no token at all — the path that mints a brand-new player.
fn join_anonymous() -> awc::ws::Message {
    awc::ws::Message::Text(serde_json::json!({ "type": "join" }).to_string().into())
}

#[actix_web::test]
async fn test_second_join_on_one_connection_is_rejected() {
    use futures_util::SinkExt as _;

    let addr = spawn_server().await;
    let (_status, body) = post_create_room(&addr, None).await;
    let code = body["room_code"].as_str().unwrap().to_owned();

    // Plain-text WebSocket is intentional: loopback-only ephemeral-port test.
    let ws_url = format!("{}://{addr}/ws/{code}", "ws");
    let (_resp, mut conn) = awc::Client::new().ws(ws_url).connect().await.unwrap();

    // First join succeeds and mints a player.
    conn.send(join_anonymous()).await.unwrap();
    let welcome = next_json(&mut conn).await;
    assert_eq!(welcome["type"], "welcome", "first join must succeed");

    // Second join on the SAME connection. Without a guard this mints a second
    // player and strands the first at `connected: true` with no owner, because
    // the connection's cleanup can only ever detach whichever player it holds
    // last. Repeat it enough times and the room fills with phantoms.
    conn.send(join_anonymous()).await.unwrap();
    let msg = loop {
        let m = next_json(&mut conn).await;
        // Skip the room_state broadcast the first join triggered.
        if m["type"] != "room_state" {
            break m;
        }
    };
    assert_eq!(
        msg["type"], "error",
        "a second join on a joined connection must be refused; got {msg}"
    );
    assert_eq!(
        msg["code"], "wrong_state",
        "must use a code the SPA already maps to a real string; got {msg}"
    );
}

#[actix_web::test]
async fn test_a_refused_join_still_allows_a_later_successful_join() {
    use futures_util::SinkExt as _;

    let addr = spawn_server().await;
    let (_status, body) = post_create_room(&addr, None).await;
    let code = body["room_code"].as_str().unwrap().to_owned();

    // Plain-text WebSocket is intentional: loopback-only ephemeral-port test.
    let ws_url = format!("{}://{addr}/ws/{code}", "ws");
    let (_resp, mut conn) = awc::Client::new().ws(ws_url).connect().await.unwrap();

    // A join carrying an unknown token is refused and leaves the connection
    // un-joined, so the guard added for the duplicate-join case must NOT latch:
    // a genuine retry has to keep working.
    conn.send(join_by_token("not-a-real-token")).await.unwrap();
    let refused = next_json(&mut conn).await;
    assert_eq!(refused["code"], "not_found", "got {refused}");

    conn.send(join_anonymous()).await.unwrap();
    let welcome = next_json(&mut conn).await;
    assert_eq!(
        welcome["type"], "welcome",
        "a retry after a refused join must still succeed; got {welcome}"
    );
}
