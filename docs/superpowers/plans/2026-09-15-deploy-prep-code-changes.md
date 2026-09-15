# Deploy-Prep Code Changes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Land the three code changes the public deploy depends on — reject unknown player tokens on join, add a server-initiated WebSocket heartbeat, and gate room creation behind a shared token.

**Architecture:** All three follow patterns already established in this codebase. The creation gate mirrors `ALLOWED_ORIGINS` exactly: an environment variable read per request, degrading to "off" when unset so local development and the Playwright E2E suite are unaffected, with the decision itself in a pure, unit-tested function. The heartbeat's timing decision is extracted into a small struct so it can be tested without sleeping for 90 seconds. The token fix reuses an error code the frontend already handles.

**Tech Stack:** Rust 2024 edition on stable toolchain, actix-web 4, actix-ws 0.4, tokio 1.52 (`time` feature already enabled); Vue 3 + TypeScript, Vitest, vue-i18n.

**Spec:** `docs/superpowers/specs/2026-09-15-deploy-and-private-beta-design.md` (§5.6, §5.7, §5.8)

## Global Constraints

- **Toolchain is stable Rust (1.94.1 in this container).** `std` has had native file locking since 1.89; do **not** add `fs4` or `fs2`.
- **Clippy runs with `all` + `pedantic` + `nursery` at warn, and CI uses `-D warnings`.** Every new public item needs a doc comment; every `async fn` with no internal await needs `#[allow(clippy::unused_async)]` if actix's signature forces it. Follow the existing `#[allow(...)]` comments in neighbouring code for the house style.
- **`cargo fmt --all -- --check` must pass.**
- **Frontend: vitest passing is NOT sufficient.** Run `npm run build` (which runs `vue-tsc`) before every frontend commit — the type check catches what vitest does not.
- **Never regenerate `frontend/package-lock.json`.** It is deliberately Linux-generated; a local `npm install` on Windows reintroduces drift that breaks CI's `npm ci`. No task here changes frontend dependencies.
- **A Semgrep PostToolUse hook blocks the literal string `ws://`.** Existing integration tests dodge it with `format!("{}://{addr}/ws/{code}", "ws")`. Use that same form in any new test.
- **All user-visible frontend strings go through `$t()` / `t()`.** No bare literals in templates.
- **Error codes sent to the client must already exist in `errorKey()`'s `known` list** (`frontend/src/i18n/index.ts:13`) or be added to it, otherwise they silently fall back to `errors.generic`.

---

## File Structure

| File | Responsibility | Task |
|---|---|---|
| `src/ws/connection.rs` | Modified: new `JoinOutcome::UnknownToken`; heartbeat wired into the select loop | 1, 2 |
| `src/ws/heartbeat.rs` | **Created**: pure liveness-tracking struct, unit tested | 2 |
| `src/ws/mod.rs` | Modified: declare `heartbeat` | 2 |
| `src/gate.rs` | **Created**: `create_token_from_env()` + pure `create_allowed()` | 3 |
| `src/lib.rs` | Modified: declare `gate` | 3 |
| `src/error.rs` | Modified: new `AppError::CreateForbidden` → 403 | 3 |
| `src/server.rs` | Modified: `create_room` checks the token header | 3 |
| `tests/ws_integration.rs` | Modified: integration coverage for tasks 1 and 3 | 1, 3 |
| `frontend/src/services/createToken.ts` | **Created**: capture `?k=`, persist, read back | 4 |
| `frontend/src/services/__tests__/createToken.spec.ts` | **Created**: unit tests | 4 |
| `frontend/src/main.ts` | Modified: capture the token before mount | 4 |
| `frontend/src/views/LandingView.vue` | Modified: send the header, handle 403 | 4 |
| `frontend/src/i18n/locales/en.json` | Modified: `errors.createForbidden` | 4 |

---

### Task 1: Reject unknown player tokens on join

A `Join` carrying a non-empty `player_token` that matches no player in the room currently falls through to `room.add_player("")`, creating an empty-named ghost player. It should be rejected with `not_found`, which the frontend already handles at `frontend/src/stores/room.ts:156` by clearing the saved token and re-prompting for a name.

**Files:**
- Modify: `src/ws/connection.rs` (the `JoinOutcome` enum at :306, the outcome resolution at :231-245, the broadcast match at :247-252, the result match at :259-302)
- Test: `tests/ws_integration.rs`

**Interfaces:**
- Consumes: nothing from earlier tasks.
- Produces: `JoinOutcome::UnknownToken` — a new variant on the private enum in `connection.rs`. Nothing outside that file consumes it.

- [ ] **Step 1: Write the failing test**

Append to `tests/ws_integration.rs`:

```rust
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
```

- [ ] **Step 2: Run the test and confirm it fails**

Run: `cargo test --test ws_integration test_join_with_unknown_token_is_rejected`
Expected: FAIL. The current code sends a `welcome` frame (having created an empty-named player), so the assertion on `msg["type"] == "error"` fails.

- [ ] **Step 3: Add the enum variant**

In `src/ws/connection.rs`, extend the enum at :306:

```rust
/// Result of resolving a join inside the room lock.
enum JoinOutcome {
    /// `(player_id, token, was_rejoin)`
    Joined(PlayerId, String, bool),
    RoomFull,
    RoomLocked,
    /// A non-empty token was supplied but matched no player in the room.
    UnknownToken,
}
```

- [ ] **Step 4: Reject the unknown token during resolution**

In `handle_join`, the `if let Some((id, token)) = existing` chain (currently :231-245) gains a branch. `is_rejoin` is a `bool` computed at :202, so it is `Copy` and usable inside the closure:

```rust
        let outcome: JoinOutcome = if let Some((id, token)) = existing {
            if let Some(p) = room.players.iter_mut().find(|p| p.id == id) {
                p.connected = true;
            }
            // Always send Welcome on (re-)join so the client can set `me`.
            // Reconnection by token bypasses the lock — a dropped player returns.
            JoinOutcome::Joined(id, token, true)
        } else if is_rejoin {
            // A token was supplied but matched nothing. Minting a player here
            // would create an empty-named ghost (the name is blank for rejoins),
            // so reject and let the client clear its stale token.
            JoinOutcome::UnknownToken
        } else if room.locked {
            JoinOutcome::RoomLocked
        } else if room.is_full() {
            JoinOutcome::RoomFull
        } else {
            let (id, token) = room.add_player(new_name, now);
            JoinOutcome::Joined(id, token, false)
        };
```

- [ ] **Step 5: Add the variant to the broadcast match**

Still inside the closure (currently :247-252):

```rust
        let broadcast = match &outcome {
            JoinOutcome::Joined(..) => vec![Outbound::All(ServerMessage::RoomState {
                room: PublicRoom::from(&*room),
            })],
            JoinOutcome::RoomFull | JoinOutcome::RoomLocked | JoinOutcome::UnknownToken => {
                Vec::new()
            }
        };
```

The `dirty` line below it needs no change: `matches!(outcome, JoinOutcome::Joined(_, _, false))` is already false for the new variant.

- [ ] **Step 6: Send the error**

Add an arm to the `match resolved` block, after the `RoomFull` arm:

```rust
        Ok(JoinOutcome::UnknownToken) => {
            log::info!("ws join rejected: room={code} unknown player token");
            let _ = forward(
                session,
                &ServerMessage::Error {
                    code: "not_found".into(),
                    message: "Room not found".into(),
                },
            )
            .await;
            Ok(())
        }
```

- [ ] **Step 7: Run the test and confirm it passes**

Run: `cargo test --test ws_integration test_join_with_unknown_token_is_rejected`
Expected: PASS

- [ ] **Step 8: Run the full suite and lints**

Run: `cargo test --workspace && cargo clippy --all-targets -- -D warnings && cargo fmt --all -- --check`
Expected: all pass. Pay attention to any existing test that relied on a tokenless or unknown-token join succeeding.

- [ ] **Step 9: Commit**

```bash
git add src/ws/connection.rs tests/ws_integration.rs
git commit -m "fix(ws): reject unknown player tokens instead of minting a ghost

A Join with a non-empty but unrecognised token fell through to
add_player with an empty name, creating a blank-named player. It now
returns not_found, which the client's existing stale-token recovery
handles by clearing the token and re-prompting."
```

---

### Task 2: Server-initiated WebSocket heartbeat

Nothing currently sends a periodic ping — `connection.rs:69` only *replies* to them. An idle connection survives only as long as every intermediary allows, and a friends-and-family test spends most of its time in the lobby, where sockets are genuinely idle.

The browser WebSocket API cannot send pings, so this must be server-initiated; browsers reply to Ping frames automatically at protocol level.

**Files:**
- Create: `src/ws/heartbeat.rs`
- Modify: `src/ws/mod.rs`
- Modify: `src/ws/connection.rs` (the spawned task at :50-127)

**Interfaces:**
- Consumes: nothing from earlier tasks.
- Produces:
  - `turn_tracker::ws::heartbeat::HEARTBEAT_INTERVAL: Duration` (30s)
  - `turn_tracker::ws::heartbeat::CLIENT_TIMEOUT: Duration` (90s)
  - `turn_tracker::ws::heartbeat::Heartbeat::new(timeout: Duration, now: Instant) -> Heartbeat`
  - `Heartbeat::record_pong(&mut self, now: Instant)`
  - `Heartbeat::is_stale(&self, now: Instant) -> bool`

**A note on test scope:** the timing decision is deliberately extracted into `Heartbeat` so it can be unit-tested with millisecond durations. Do **not** write an integration test that waits 30 or 90 seconds — and note that `awc` auto-replies to pings, so a client that never pongs cannot easily be simulated end-to-end. End-to-end validation is the manual "background a phone for 5 minutes" check in the spec's §8.

- [ ] **Step 1: Write the failing test**

Create `src/ws/heartbeat.rs` containing only the tests:

```rust
//! Liveness tracking for a WebSocket connection.

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn fresh_connection_is_not_stale() {
        let now = Instant::now();
        let hb = Heartbeat::new(Duration::from_millis(50), now);
        assert!(!hb.is_stale(now));
    }

    #[test]
    fn connection_is_stale_once_the_timeout_elapses() {
        let now = Instant::now();
        let hb = Heartbeat::new(Duration::from_millis(50), now);
        assert!(
            hb.is_stale(now + Duration::from_millis(51)),
            "must be stale strictly after the timeout"
        );
    }

    #[test]
    fn connection_is_not_stale_exactly_at_the_timeout() {
        let now = Instant::now();
        let hb = Heartbeat::new(Duration::from_millis(50), now);
        assert!(!hb.is_stale(now + Duration::from_millis(50)));
    }

    #[test]
    fn recording_a_pong_resets_staleness() {
        let now = Instant::now();
        let mut hb = Heartbeat::new(Duration::from_millis(50), now);
        let later = now + Duration::from_millis(40);
        hb.record_pong(later);
        assert!(!hb.is_stale(later + Duration::from_millis(40)));
        assert!(hb.is_stale(later + Duration::from_millis(51)));
    }

    #[test]
    fn timeout_is_a_multiple_of_the_interval_so_a_single_lost_pong_is_tolerated() {
        assert!(
            CLIENT_TIMEOUT >= HEARTBEAT_INTERVAL * 2,
            "one dropped pong must not close a healthy connection"
        );
    }
}
```

Add the module declaration to `src/ws/mod.rs`:

```rust
pub mod connection;
pub mod dispatch;
pub mod heartbeat;
pub mod origin;
```

- [ ] **Step 2: Run the test and confirm it fails**

Run: `cargo test heartbeat`
Expected: FAIL to compile — `cannot find type Heartbeat in this scope`, plus unresolved `CLIENT_TIMEOUT` / `HEARTBEAT_INTERVAL`.

- [ ] **Step 3: Write the implementation**

Insert above the `#[cfg(test)]` block in `src/ws/heartbeat.rs`:

```rust
use std::time::{Duration, Instant};

/// How often the server sends a Ping frame. Chosen to sit well under the
/// shortest idle timeout we expect to meet — carrier NAT, and Cloudflare's
/// proxy if it is ever enabled in front of the app.
pub const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);

/// How long a connection may go without a Pong before it is closed. Two
/// intervals plus slack, so one dropped Pong does not kill a live connection.
pub const CLIENT_TIMEOUT: Duration = Duration::from_secs(90);

/// Tracks when a connection last proved it was alive.
///
/// Browsers answer Ping frames automatically at protocol level (the JS
/// WebSocket API cannot send pings), so the server pings and the client's
/// Pong is the liveness signal.
#[derive(Debug)]
pub struct Heartbeat {
    timeout: Duration,
    last_pong: Instant,
}

impl Heartbeat {
    /// Start tracking, treating `now` as the last known-good moment.
    #[must_use]
    pub const fn new(timeout: Duration, now: Instant) -> Self {
        Self {
            timeout,
            last_pong: now,
        }
    }

    /// Record that the peer answered a Ping.
    pub const fn record_pong(&mut self, now: Instant) {
        self.last_pong = now;
    }

    /// Whether the peer has been silent for longer than the timeout.
    #[must_use]
    pub fn is_stale(&self, now: Instant) -> bool {
        now.duration_since(self.last_pong) > self.timeout
    }
}
```

- [ ] **Step 4: Run the test and confirm it passes**

Run: `cargo test heartbeat`
Expected: PASS (5 tests).

If clippy objects to `const fn` on either method, drop `const` — it is a nicety, not a requirement.

- [ ] **Step 5: Wire it into the connection loop**

In `src/ws/connection.rs`, add the import alongside the existing `use crate::ws::origin::...` line:

```rust
use crate::ws::heartbeat::{CLIENT_TIMEOUT, HEARTBEAT_INTERVAL, Heartbeat};
```

Inside the spawned task, just after `let mut me: Option<PlayerId> = None;` (currently :56):

```rust
        let mut heartbeat = Heartbeat::new(CLIENT_TIMEOUT, Instant::now());
        let mut ticker = tokio::time::interval(HEARTBEAT_INTERVAL);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        // `interval` yields immediately on its first tick; consume it so the
        // first real ping happens one full interval from now.
        ticker.tick().await;
```

- [ ] **Step 6: Record pongs from the client**

In the `incoming = msg_stream.next()` arm, add this **before** the catch-all `_ => {}` (currently :72), since the catch-all would otherwise swallow it:

```rust
                        Some(Ok(actix_ws::Message::Pong(_))) => {
                            heartbeat.record_pong(Instant::now());
                        }
```

- [ ] **Step 7: Add the ticker branch to the select**

Add a third branch to the `tokio::select!`, after the `broadcast = rx.recv()` arm:

```rust
                _ = ticker.tick() => {
                    if heartbeat.is_stale(Instant::now()) {
                        log::info!("ws heartbeat timeout: room={code}");
                        break;
                    }
                    if session.ping(b"").await.is_err() {
                        break;
                    }
                }
```

- [ ] **Step 8: Run the full suite and lints**

Run: `cargo test --workspace && cargo clippy --all-targets -- -D warnings && cargo fmt --all -- --check`
Expected: all pass. The existing `ws_integration` tests must still pass — `awc` answers pings automatically, so they are unaffected.

- [ ] **Step 9: Commit**

```bash
git add src/ws/heartbeat.rs src/ws/mod.rs src/ws/connection.rs
git commit -m "feat(ws): add server-initiated heartbeat

Pings every 30s and closes a connection after 90s without a Pong, so
idle sockets survive intermediary timeouts and half-open connections
get cleaned up. Server-initiated because the browser WebSocket API
cannot send pings; browsers reply to Ping frames automatically."
```

---

### Task 3: Gate room creation behind `TT_CREATE_TOKEN`

During the friends-and-family window only invited hosts should be able to create rooms. Joining stays completely open — the QR deep-link flow is what the test exists to validate.

This mirrors `ALLOWED_ORIGINS` deliberately: read per request, "off" when unset (so local dev and the Playwright E2E suite need no changes), with the decision in a pure function that is unit tested.

**Files:**
- Create: `src/gate.rs`
- Modify: `src/lib.rs`
- Modify: `src/error.rs`
- Modify: `src/server.rs` (`create_room` at :37-52)
- Test: `tests/ws_integration.rs`

**Interfaces:**
- Consumes: nothing from earlier tasks.
- Produces:
  - `turn_tracker::gate::create_token_from_env() -> Option<String>`
  - `turn_tracker::gate::create_allowed(provided: Option<&str>, expected: Option<&str>) -> bool`
  - `turn_tracker::error::AppError::CreateForbidden` → HTTP 403
  - Request header name: `X-Create-Token`

- [ ] **Step 1: Write the failing unit tests**

Create `src/gate.rs`:

```rust
//! Optional shared-secret gate on room creation, used during a closed beta.
//!
//! Mirrors the `ALLOWED_ORIGINS` pattern: unset means the gate is OFF, so local
//! development and the Playwright E2E suite need no configuration. When set,
//! `POST /api/rooms` requires a matching `X-Create-Token` header.
//!
//! Joining a room is deliberately NOT gated — guests arrive by QR deep link and
//! must see no extra friction.
//!
//! TRUST BOUNDARY: this is a shared secret distributed to a handful of hosts,
//! not an authentication system. It is transmitted over TLS and guarded by the
//! per-IP rate limiter on the same endpoint, which runs first. A constant-time
//! comparison is not warranted at this scale.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gate_is_open_when_no_token_is_configured() {
        assert!(create_allowed(None, None));
        assert!(
            create_allowed(Some("anything"), None),
            "a stray header must not break an ungated server"
        );
    }

    #[test]
    fn matching_token_is_allowed() {
        assert!(create_allowed(Some("s3cret"), Some("s3cret")));
    }

    #[test]
    fn missing_token_is_rejected_when_gated() {
        assert!(!create_allowed(None, Some("s3cret")));
    }

    #[test]
    fn wrong_token_is_rejected() {
        assert!(!create_allowed(Some("nope"), Some("s3cret")));
    }

    #[test]
    fn empty_configured_token_means_the_gate_is_off() {
        // Treat `TT_CREATE_TOKEN=` the same as unset, matching how
        // ALLOWED_ORIGINS handles an empty value.
        assert!(create_allowed(None, Some("")));
    }
}
```

Add to `src/lib.rs` alongside the existing module declarations:

```rust
pub mod gate;
```

- [ ] **Step 2: Run the tests and confirm they fail**

Run: `cargo test gate`
Expected: FAIL to compile — `cannot find function create_allowed in this scope`.

- [ ] **Step 3: Write the implementation**

Insert above the `#[cfg(test)]` block in `src/gate.rs`:

```rust
/// Read the configured creation token, treating unset or empty as "no gate".
#[must_use]
pub fn create_token_from_env() -> Option<String> {
    std::env::var("TT_CREATE_TOKEN")
        .ok()
        .filter(|t| !t.is_empty())
}

/// Whether a room-creation request may proceed.
///
/// `expected` of `None` or `Some("")` means the gate is disabled and everything
/// is allowed.
#[must_use]
pub fn create_allowed(provided: Option<&str>, expected: Option<&str>) -> bool {
    match expected {
        None | Some("") => true,
        Some(want) => provided == Some(want),
    }
}
```

- [ ] **Step 4: Run the tests and confirm they pass**

Run: `cargo test gate`
Expected: PASS (5 tests).

- [ ] **Step 5: Add the error variant**

In `src/error.rs`, add to the enum:

```rust
    #[error("Hosting is invite-only during the beta")]
    CreateForbidden,
```

and to `status_code`:

```rust
            Self::CreateForbidden => StatusCode::FORBIDDEN,
```

- [ ] **Step 6: Write the failing integration test**

Append to `tests/ws_integration.rs`:

```rust
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
```

- [ ] **Step 7: Run the integration tests and confirm they pass**

Run: `cargo test --test ws_integration`
Expected: the whole file PASSes, before and after the handler change in Step 8. (Cargo takes a single `TESTNAME` positional, so run the file rather than naming both tests.)

**This is deliberately not a red-green step, and that is the point.** These two
tests pin the *gate-off* behaviour — that an unconfigured server keeps creating
rooms and ignores a stray header — which is the regression that would break the
existing Playwright E2E suite. They must pass now and still pass after Step 8.

The rejection path is covered by the unit tests in Step 1 rather than here,
because `TT_CREATE_TOKEN` is process-global: setting it inside one
`#[actix_web::test]` would leak into every other test running in the same
process and make the suite order-dependent. Do not try to work around this by
setting the variable in an integration test.

- [ ] **Step 8: Enforce the gate in the handler**

In `src/server.rs`, add the import:

```rust
use crate::gate;
```

and change `create_room` to take the request and check the header first:

```rust
/// POST /api/rooms — create a room, returning the code + host credentials.
///
/// # Errors
/// `AppError::CreateForbidden` if `TT_CREATE_TOKEN` is set and the
/// `X-Create-Token` header does not match; `AppError::InvalidRequest` for an
/// empty host name; `AppError::NameTooLong` if the name exceeds the length
/// bound; `AppError::CodeExhausted` if no unique code is available;
/// `AppError::RoomCapacityReached` if the server is full.
#[allow(clippy::unused_async)] // required by actix-web handler signature
pub async fn create_room(
    req: actix_web::HttpRequest,
    registry: web::Data<Arc<Registry>>,
    body: web::Json<CreateRoomRequest>,
) -> Result<impl Responder, AppError> {
    // Runs after the rate limiter (wrapped on the resource), so guessing the
    // token is itself throttled.
    let provided = req
        .headers()
        .get("X-Create-Token")
        .and_then(|v| v.to_str().ok());
    let expected = gate::create_token_from_env();
    if !gate::create_allowed(provided, expected.as_deref()) {
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
```

- [ ] **Step 9: Warn at startup when the gate is off**

In `src/main.rs`, next to the existing `ALLOWED_ORIGINS` warning:

```rust
    if turn_tracker::gate::create_token_from_env().is_none() {
        log::info!("TT_CREATE_TOKEN is unset: room creation is OPEN to anyone");
    }
```

- [ ] **Step 10: Run the full suite and lints**

Run: `cargo test --workspace && cargo clippy --all-targets -- -D warnings && cargo fmt --all -- --check`
Expected: all pass.

- [ ] **Step 11: Commit**

```bash
git add src/gate.rs src/lib.rs src/error.rs src/server.rs src/main.rs tests/ws_integration.rs
git commit -m "feat(api): optional TT_CREATE_TOKEN gate on room creation

Room creation requires a matching X-Create-Token header when
TT_CREATE_TOKEN is set; unset means open, mirroring ALLOWED_ORIGINS so
local dev and the E2E suite are unaffected. Joining is deliberately not
gated — guests arrive by QR deep link and must see no extra friction."
```

---

### Task 4: Carry the creation token in the SPA

The SPA is baked into the image at build time, so the token cannot be a build-time variable. Hosts receive `https://whosego.app/?k=<token>`; the SPA captures it, stores it, strips it from the URL, and sends it as a header on create.

**Files:**
- Create: `frontend/src/services/createToken.ts`
- Create: `frontend/src/services/__tests__/createToken.spec.ts`
- Modify: `frontend/src/main.ts`
- Modify: `frontend/src/views/LandingView.vue`
- Modify: `frontend/src/i18n/locales/en.json`
- Test: `frontend/src/views/__tests__/LandingView.spec.ts`

**Interfaces:**
- Consumes: `X-Create-Token` header name and the 403 status from Task 3.
- Produces:
  - `captureCreateTokenFromUrl(): void` — reads `?k=`, persists it, strips it from the URL
  - `loadCreateToken(): string | null`
  - `localStorage` key: `tt:createToken`
  - i18n key: `errors.createForbidden`

- [ ] **Step 1: Write the failing unit tests**

Create `frontend/src/services/__tests__/createToken.spec.ts`:

```typescript
import { describe, it, expect, beforeEach } from 'vitest'
import { captureCreateTokenFromUrl, loadCreateToken } from '@/services/createToken'

function setUrl(url: string) {
  window.history.replaceState({}, '', url)
}

describe('createToken', () => {
  beforeEach(() => {
    localStorage.clear()
    setUrl('/')
  })

  it('returns null when nothing has been captured', () => {
    expect(loadCreateToken()).toBeNull()
  })

  it('captures ?k= and persists it', () => {
    setUrl('/?k=abc123')
    captureCreateTokenFromUrl()
    expect(loadCreateToken()).toBe('abc123')
  })

  it('strips the token from the URL so it is not shared or logged', () => {
    setUrl('/?k=abc123')
    captureCreateTokenFromUrl()
    expect(window.location.search).not.toContain('abc123')
  })

  it('preserves other query parameters while stripping k', () => {
    setUrl('/?k=abc123&debug=1')
    captureCreateTokenFromUrl()
    expect(window.location.search).toContain('debug=1')
    expect(window.location.search).not.toContain('abc123')
  })

  it('keeps a previously stored token when the URL has none', () => {
    setUrl('/?k=abc123')
    captureCreateTokenFromUrl()
    setUrl('/')
    captureCreateTokenFromUrl()
    expect(loadCreateToken()).toBe('abc123')
  })

  it('overwrites a stored token when a new one arrives', () => {
    setUrl('/?k=old')
    captureCreateTokenFromUrl()
    setUrl('/?k=new')
    captureCreateTokenFromUrl()
    expect(loadCreateToken()).toBe('new')
  })
})
```

- [ ] **Step 2: Run the tests and confirm they fail**

Run: `cd frontend && npm run test -- createToken`
Expected: FAIL — cannot resolve `@/services/createToken`.

- [ ] **Step 3: Write the implementation**

Create `frontend/src/services/createToken.ts`:

```typescript
/**
 * Closed-beta host token. Hosts are given a link of the form
 * `https://whosego.app/?k=<token>`; the token is captured once, persisted per
 * device, and sent as a header when creating a room. Joining is never gated.
 */
const KEY = 'tt:createToken'
const PARAM = 'k'

/**
 * Capture `?k=` from the current URL, persist it, and strip it from the
 * address bar so it is not shared along with a copied link.
 */
export function captureCreateTokenFromUrl(): void {
  const url = new URL(window.location.href)
  const token = url.searchParams.get(PARAM)
  if (!token) return
  try {
    localStorage.setItem(KEY, token)
  } catch {
    // Private mode or blocked storage: the header is simply not sent.
  }
  url.searchParams.delete(PARAM)
  window.history.replaceState({}, '', `${url.pathname}${url.search}${url.hash}`)
}

/** The stored host token, if this device has one. */
export function loadCreateToken(): string | null {
  try {
    return localStorage.getItem(KEY)
  } catch {
    return null
  }
}
```

- [ ] **Step 4: Run the tests and confirm they pass**

Run: `cd frontend && npm run test -- createToken`
Expected: PASS (6 tests).

- [ ] **Step 5: Capture the token before mount**

Modify `frontend/src/main.ts` so the token is captured whatever route the host lands on:

```typescript
import './styles/tokens.css'
import './styles/base.css'
import { createApp } from 'vue'
import { createPinia } from 'pinia'
import App from './App.vue'
import { router } from './router'
import { i18n } from './i18n'
import { captureCreateTokenFromUrl } from './services/createToken'

captureCreateTokenFromUrl()

createApp(App).use(createPinia()).use(router).use(i18n).mount('#app')
```

- [ ] **Step 6: Add the error string**

In `frontend/src/i18n/locales/en.json`, insert this line into the `errors`
object **immediately after the `"generic"` line**, so the trailing comma stays
valid (`"reconnecting"` remains the last key):

```json
    "createForbidden": "Hosting is invite-only during the beta",
```

- [ ] **Step 7: Write the failing view tests**

Append to `frontend/src/views/__tests__/LandingView.spec.ts`, inside the existing `describe('LandingView', ...)` block:

```typescript
  it('sends the create token header when one is stored', async () => {
    localStorage.setItem('tt:createToken', 'host-token')
    const fetchMock = vi.fn().mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => ({ room_code: 'GR7K9P', player_id: 'p1', token: 'tok' }),
    })
    vi.stubGlobal('fetch', fetchMock)
    const { wrapper, router } = mountView()
    await router.isReady()
    await wrapper.get('[data-test=name]').setValue('Sam')
    await wrapper.get('[data-test=create]').trigger('click')
    await flushPromises()
    const headers = fetchMock.mock.calls[0][1].headers as Record<string, string>
    expect(headers['X-Create-Token']).toBe('host-token')
  })

  it('omits the header entirely when no token is stored', async () => {
    const fetchMock = vi.fn().mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => ({ room_code: 'GR7K9P', player_id: 'p1', token: 'tok' }),
    })
    vi.stubGlobal('fetch', fetchMock)
    const { wrapper, router } = mountView()
    await router.isReady()
    await wrapper.get('[data-test=name]').setValue('Sam')
    await wrapper.get('[data-test=create]').trigger('click')
    await flushPromises()
    const headers = fetchMock.mock.calls[0][1].headers as Record<string, string>
    expect(headers['X-Create-Token']).toBeUndefined()
  })

  it('shows the invite-only message on 403', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue({ ok: false, status: 403 }))
    const { wrapper, router } = mountView()
    await router.isReady()
    await wrapper.get('[data-test=name]').setValue('Sam')
    await wrapper.get('[data-test=create]').trigger('click')
    await flushPromises()
    expect(wrapper.text()).toContain('invite-only')
  })
```

- [ ] **Step 8: Run the tests and confirm they fail**

Run: `cd frontend && npm run test -- LandingView`
Expected: FAIL — no `X-Create-Token` header is sent, and a 403 currently renders the generic error.

- [ ] **Step 9: Update the view**

In `frontend/src/views/LandingView.vue`, add the import next to the existing `saveToken` import:

```typescript
import { loadCreateToken } from '@/services/createToken'
```

and replace `create()`:

```typescript
async function create() {
  error.value = ''
  const headers: Record<string, string> = { 'content-type': 'application/json' }
  const createToken = loadCreateToken()
  if (createToken) headers['X-Create-Token'] = createToken
  const res = await fetch('/api/rooms', {
    method: 'POST',
    headers,
    body: JSON.stringify({ host_name: name.value.trim() }),
  })
  if (!res.ok) {
    error.value = res.status === 403 ? t('errors.createForbidden') : t('errors.generic')
    return
  }
  const data = (await res.json()) as { room_code: string; player_id: string; token: string }
  saveToken(data.room_code, data.token)
  router.push({ path: `/room/${data.room_code}` })
}
```

- [ ] **Step 10: Run the tests and confirm they pass**

Run: `cd frontend && npm run test -- LandingView`
Expected: PASS

- [ ] **Step 11: Run the whole frontend gate**

Run: `cd frontend && npm run test && npm run build`
Expected: both pass. The `build` step runs `vue-tsc`; vitest passing alone is not sufficient and has hidden type errors on this project before.

- [ ] **Step 12: Commit**

```bash
git add frontend/src/services/createToken.ts \
        frontend/src/services/__tests__/createToken.spec.ts \
        frontend/src/main.ts \
        frontend/src/views/LandingView.vue \
        frontend/src/views/__tests__/LandingView.spec.ts \
        frontend/src/i18n/locales/en.json
git commit -m "feat(frontend): carry the closed-beta host token

Captures ?k= once, persists it per device, strips it from the URL, and
sends it as X-Create-Token when creating a room. A 403 renders an
invite-only message. The join path is untouched."
```

---

### Task 5: Update the operator runbook

`DEPLOY.md` is the document the deploy is executed from. It must describe the two new environment-facing behaviours.

**Files:**
- Modify: `DEPLOY.md`

**Interfaces:**
- Consumes: `TT_CREATE_TOKEN` from Task 3; heartbeat constants from Task 2.
- Produces: nothing consumed by code.

- [ ] **Step 1: Add the variable to the environment table**

Add a row to the table in `DEPLOY.md`, after `TT_SNAPSHOT_INTERVAL_SECS`:

```markdown
| `TT_CREATE_TOKEN` | _(unset)_ | Shared secret required in the `X-Create-Token` header on `POST /api/rooms`. Unset = room creation is open to anyone. |
```

- [ ] **Step 2: Document the closed-beta gate**

Add a new subsection under "Operational requirements":

```markdown
### 5. Closed beta: `TT_CREATE_TOKEN`

While the app is in friends-and-family testing, set `TT_CREATE_TOKEN` to a
random 32-character string. Room **creation** then requires a matching
`X-Create-Token` header; **joining is never gated**, so guests arriving by QR
deep link see no difference.

Give hosts `https://whosego.app/?k=<token>` once — the SPA captures the token,
stores it on the device, and strips it from the URL. To open the app to
everyone, remove the variable and recreate the container; the gate code is
inert when unset.

The token is a shared secret, not authentication. It is protected by TLS and by
the per-IP rate limiter on the same endpoint, which runs first.
```

- [ ] **Step 3: Document the heartbeat**

Add under "Operational requirements":

```markdown
### 6. WebSocket heartbeat

The server pings each WebSocket every 30s and closes a connection after 90s
without a pong. Any reverse proxy in front of the app must therefore allow idle
WebSocket connections to live at least 90s. Caddy imposes no such timeout by
default. This is also why the app is served with Cloudflare's proxy disabled
(grey cloud) — see the deploy spec.
```

- [ ] **Step 4: Commit**

```bash
git add DEPLOY.md
git commit -m "docs: document TT_CREATE_TOKEN and the WebSocket heartbeat"
```

---

## Done criteria

- `cargo test --workspace` passes (backend suite grows from 95 by roughly 11).
- `cargo clippy --all-targets -- -D warnings` and `cargo fmt --all -- --check` clean.
- `cd frontend && npm run test && npm run build` both pass (frontend suite grows from 80 to 89).
- CI green on the branch.
- `DEPLOY.md` documents `TT_CREATE_TOKEN` and the heartbeat.

Infrastructure and CD are a separate plan:
`docs/superpowers/plans/2026-09-15-deploy-infrastructure.md`.
