# Turn Tracker — MVP Design

**Status:** Approved (2026-06-16)
**Supersedes:** the original handoff spec (planning conversation). Where this document and the handoff spec disagree, this document wins.

---

## Purpose

Board game groups lose track of whose turn it is during tangents, breaks, and phone
distractions. Turn Tracker is a lightweight, **no-install** companion: each player joins on
their own phone via a shared code/link, sees clearly when it is their turn, and can nudge
whoever is taking too long.

The unmet niche — and the entire reason to build this rather than use an existing app — is the
combination of **no install + each player on their own phone + real-time turn coordination for a
physical, co-located board game.**

---

## Key decisions from brainstorming

These decisions resolve the original spec's open questions and override its suggestions.

1. **Web, Jackbox-style — not native.** Zero-install, share-a-link join is the product's
   structural advantage. Going native (Flutter/etc.) would trade away the near-zero adoption
   friction that makes spontaneous, in-person group use viable — and would not even cleanly solve
   the notification problem it would be chosen for (iOS push remains gated). Decision: **web SPA.**

2. **No background push for MVP.** The core value is answered by *foreground* signalling, because
   the activity is co-located and talkative. Background Web Push (waking a locked/pocketed phone)
   is the most expensive piece to build (service worker + VAPID + subscription storage + encrypted
   payloads + an iOS "Add to Home Screen" onboarding tail) and serves only the "player wandered
   off" case — which the table can cover socially via the nudge. MVP relies on:
   - full-screen turn state,
   - sound + Vibration API on becoming active,
   - Screen Wake Lock on the active player's phone,
   - **nudge** as the social backstop for stragglers.
   Background Web Push is a clearly-scoped Phase 2, justified by real usage if people report
   missing turns while away.

3. **Manual turn passing, model "B" (push + pull).** No auto-detection — every automatic method
   (mic, camera, accelerometer, timer auto-advance) is privacy-hostile, unreliable, or adds more
   setup friction than it removes. The frictionless win is that **only one phone needs to act at a
   time** (the active player's), every other phone is passive, and the one action is a big,
   forgiving, full-screen tap.
   - The **current player** taps "Done" to advance, **or**
   - the **next player** can "claim" the turn (pull) — mirrors a real table and removes the
     active-player-forgot-to-tap stall,
   - the **host** can override (skip, reorder, jump), and
   - **undo** reverts an accidental advance.

4. **Host is a player.** The host enters a name at room creation and becomes a `Player` with an
   `is_host` flag, included in the turn rotation. No separate coordinator role in MVP.

5. **Serve the Vue build from Actix.** One binary, one deploy, no CORS, no separate CDN config.
   (Resolves original open question 5.) Cloudflare Pages split remains possible later.

---

## MVP Scope

### In scope
- Host creates a room and receives a shareable code/link.
- Players join by code/link — **no account, no install.**
- Host sets turn order via drag-to-reorder in the lobby and can update it mid-game.
- Turn passing (model B): active player taps **Done**; next player may **claim**; host may
  override; **undo** reverts.
- Active player's screen clearly shows it is their turn (full-screen state change) with sound +
  vibration + screen wake lock.
- Any non-current player can **nudge** the current player (10s cooldown per sender,
  server-enforced).
- Players can join mid-game, be **skipped this turn**, or be **removed**.
- Rooms expire after 24h of inactivity.
- Reconnection: clients re-attach to their existing player via a stored token.

### Explicitly out of scope for MVP
- Background push notifications (Phase 2).
- Location/Bluetooth turn ordering.
- Score/round tracking, game templates, persistent/saved rooms.
- Per-turn timers.
- Accounts/authentication.
- `Paused` room state (YAGNI — nothing auto-advances, so a break simply leaves the turn where it
  is).
- Host-passing / away toggle (see Deferred).

---

## Architecture

Single Rust binary. Actix-web serves both the API/WebSocket and the static Vue build.

```
Browser (Vue 3 SPA, served as static files by Actix)
   │  HTTP  POST /api/rooms        → create room, get code + token
   │  WS    /ws/{room_code}        → all realtime play
   ▼
Actix-web server
   ├── http/      room creation endpoint, static file serving
   ├── ws/        connection actor: deserialize client msgs, auth, dispatch, broadcast
   ├── domain/    Room/Player structs + pure turn-state logic (NO Actix/WS types)
   ├── registry   DashMap<RoomCode, Room> + room lookup/insert/remove
   └── cleanup    background tokio task: sweep rooms idle > 24h
```

**Critical boundary:** `domain/` is pure Rust. `Room`, `Player`, and every turn transition
(`add_player`, `remove_player`, `end_turn`, `claim_turn`, `undo_turn`, `skip_player`,
`set_order`, next-player computation, code generation) are plain methods/functions with **no
WebSocket or Actix dependency**. The `ws/` layer does I/O only: deserialize → call domain → handle
result → serialize broadcast. This makes the entire turn engine unit-testable without a server,
which is where the logic risk concentrates.

---

## Data model

```rust
type PlayerId = String;   // server-generated, opaque
type RoomCode = String;   // 6 chars, unambiguous alphabet

enum RoomState { Lobby, Active }

struct Room {
    code: RoomCode,
    created_at: Instant,
    last_active: Instant,             // bumped on any activity; drives 24h cleanup
    state: RoomState,
    players: Vec<Player>,             // ORDER is significant — this IS the turn order
    current_player_id: Option<PlayerId>,   // tracked by ID, never by array index
    previous_player_id: Option<PlayerId>,  // single-level undo target
    last_nudge_at: HashMap<PlayerId, Instant>,  // per-sender nudge cooldown
}

struct Player {
    id: PlayerId,
    name: String,
    token: String,        // secret session credential; NEVER broadcast
    is_host: bool,
    connected: bool,
    skip_next: bool,      // one-turn skip flag, cleared when skipped
}
```

### Turn tracking by ID, not index
The original spec's `current_turn_index: usize` is fragile: removing or reordering a player
silently makes the index point at the wrong person. Instead, store `current_player_id` and compute
"next" by walking the ordered `players` list from the current player, skipping players that are
disconnected or flagged `skip_next` (clearing the flag as they are skipped). Reorder, remove, and
skip then all behave correctly without index bookkeeping.

### Public projection (token-leak fix)
The original `{ type: "room_state", room }` would serialize every player's secret `token` to every
client. Broadcasts instead use a derived view with tokens stripped:

```rust
struct PublicRoom  { code, state, players: Vec<PublicPlayer>, current_player_id }
struct PublicPlayer { id, name, is_host, connected }   // no token
// impl From<&Room> for PublicRoom
```

---

## Protocol

### HTTP
`POST /api/rooms { host_name }` → `{ room_code, player_id, token }`

Room creation is a one-shot request/response, kept separate from the realtime channel so the
shareable code/link exists before any socket opens. The host then connects the WS and `join`s with
the returned token.

### Client → Server (WebSocket)
| msg | authorized sender | effect |
|---|---|---|
| `join { player_token?, player_name? }` | anyone | with a valid token → re-attach to existing player (`connected = true`); else create a new player |
| `start_game` | host | `Lobby` → `Active`; sets `current_player_id` to the first player in `players` order |
| `set_order { player_ids }` | host | reorder the rotation |
| `end_turn` | current player **or** host | advance to next eligible player |
| `claim_turn` | the **next** eligible player | pull the turn forward (model B) |
| `undo_turn` | host | single-level undo: move `current_player_id` back to the immediately previous holder (server stores `previous_player_id`; one step only, not a full history) |
| `skip_player { player_id }` | host | set `skip_next`; if they are current, advance |
| `remove_player { player_id }` | host | remove from room (and rotation) |
| `nudge` | any non-current player | alert the current player; rejected if sender within 10s cooldown |

### Server → Client (WebSocket)
- `welcome { player_id, token }` — sent on a fresh join so the client can persist the token in
  localStorage for later reconnect.
- `room_state { room: PublicRoom }` — full state, sent on join/reconnect **and after every
  mutation**.
- `nudged` — targeted to the current player.
- `error { code, message }` — rejected/unauthorized actions.

### Full-state broadcast (intentional MVP simplification)
MVP broadcasts the entire `room_state` on every change rather than granular deltas
(`turn_changed`, `player_joined`, …). One code path, impossible to desync, and the payload is tiny
(a handful of players). Granular events are a Phase-2 optimization only if needed.

---

## Reconnection

Phones sleep and backgrounded tabs drop the socket. Strategy:
- Client persists `{ room_code, player_token }` in localStorage.
- On load/wake, the client opens the WS and sends `join { player_token }`. The server re-attaches
  to the existing player, sets `connected = true`, and replies with full `room_state`.
- Unknown/absent token → treated as a new player.
- The socket auto-reconnects with capped exponential backoff (≈1 → 2 → 5 → 10s).

A `useRoomSocket` Vue composable owns the socket lifecycle, backoff, token persistence, and exposes
the reactive `room` state derived from `room_state` messages.

---

## Security (MVP threat model: friends around a table)
- Player/host tokens are opaque 256-bit random values, stored in localStorage.
- All traffic over **HTTPS/TLS** via a reverse proxy on the VPS — tokens are never sent in clear.
- Tokens are never broadcast (see `PublicRoom`).
- The host flag is a soft permission, not hard auth; acceptable for the threat model.
- No further hardening in MVP.

---

## Frontend (Vue 3 Composition API + Vite)
- **Create** view: host enters name → `POST /api/rooms` → routed to Lobby.
- **Lobby** view: shareable code/link, drag-to-reorder player list (host), Start button (host),
  live roster.
- **Active** view:
  - Active player: huge full-screen "DONE" button; Screen Wake Lock held; sound + vibrate on
    becoming active.
  - Non-active players: passive "Alice's turn — you're N away" + a Nudge button.
- Styling: minimal, functional, mobile-first. No component library for MVP.
- A `useRoomSocket` composable centralizes realtime state and reconnection.

---

## Infrastructure / deploy
- Single Dockerized binary.
- Hetzner CAX11 (ARM) VPS — ARM Rust target; may share a box with the anime-calendar project.
- TLS-terminating reverse proxy in front of Actix.
- CI/CD: GitHub Actions + Docker, matching existing project patterns.

---

## Testing strategy (TDD)
- **Domain layer (primary):** unit-test every transition — end/claim/undo turn; skip; remove the
  current player; reorder mid-game; next-player computation skipping disconnected/`skip_next`
  players; room-code generation/collision. Pure functions → fast, exhaustive tests. Logic risk
  lives here.
- **Integration:** a few WS tests — join → start_game → `end_turn` broadcast; reconnect re-attaches
  via token; nudge cooldown rejection; unauthorized action returns `error`.
- **Frontend:** component tests later; not gating for MVP.

---

## Build order
Each step is independently testable and leaves the app runnable.

1. **Domain core + tests** — `Room`/`Player`, turn engine, code generation. No server (pure TDD).
2. **Registry + cleanup task** — `DashMap` store; 24h idle sweep via tokio interval.
3. **HTTP create + static serving** — `POST /api/rooms`; serve the Vue build.
4. **WS layer** — connection actor, join/auth, dispatch to domain, broadcast `room_state`.
5. **Frontend Create + Lobby** — create room, share code/link, drag-reorder, start.
6. **Frontend Active play** — turn screen, Done/claim/undo, nudge, wake-lock + vibrate.
7. **Reconnection polish** — backoff, token re-attach, resync UX.
8. **Deploy** — Dockerfile, TLS reverse proxy, GitHub Actions → Hetzner.

---

## Deferred (post-MVP backlog)
Captured during brainstorming for context; explicitly **not** in MVP.

- **Background Web Push** — wake locked/pocketed phones (service worker + VAPID + subscription
  storage; iOS requires PWA install). The natural Phase 2 if usage shows players miss turns while
  away.
- **Host-passing** — let the host transfer the host role to another player when they intentionally
  leave.
- **Away / inactive toggle** — mark a player as away so they're auto-skipped every round until
  cleared (vs. the MVP one-turn `skip_player`).
- **NFC "turn token"** (novelty) — phone-to-phone tap to pass the turn. No hardware to sell; iOS
  NFC is restrictive — a "maybe, if it's fun" idea, not a roadmap commitment.
- From the original spec's future directions: Bluetooth clockwise/anticlockwise ordering, score &
  round tracking, game templates, persistent/saved room configurations, long-distance async play,
  turn timers.
