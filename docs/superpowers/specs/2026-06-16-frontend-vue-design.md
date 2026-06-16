# Turn Tracker — Vue Frontend Design

**Date:** 2026-06-16
**Status:** Approved (brainstorm)
**Scope:** The `frontend/` Vue SPA that drives the Turn Tracker backend, plus one small backend change (SPA fallback route).

---

## 1. Goal & context

Turn Tracker is a no-install, pass-the-phone turn coordinator for in-person board games. The Rust/Actix backend (room model, WebSocket protocol, idle-room cleanup) is merged to `main`. This spec covers the Vue frontend that implements the approved design system and the 9 screen mockups in `design-export/Turn Tracker Design System.dc.html`.

The visual design is **settled** (tokens + 9 screens + component states + motion/a11y notes). This spec is about **architecture and build**, not aesthetics.

### Source of truth: the backend contract

The frontend binds to the existing wire contract (`src/wire.rs`). It does not invent state.

**Connect flow:**
- **Host:** `POST /api/rooms {host_name}` → `{room_code, player_id, token}`; then open WS `GET /ws/{code}` and send `join {player_token}`.
- **Joiner:** open WS `GET /ws/{code}` → send `join {player_name}` → server replies `welcome {player_id, token}`.
- `token` is the **reconnect credential**; persist it (localStorage, keyed by room code) and re-`join` with it after a drop.

**Single source of truth:** after connect, the whole UI is driven by `room_state {room}` broadcasts, plus `nudged` and `error {code, message}`.

**`PublicRoom` shape:** `{ code, state: "lobby"|"active", players: [{id, name, is_host, connected}], current_player_id }`. Every screen detail ("you're 2 away", "up next", host badge, connection dots) is **derived** from this one object.

**Client actions (ClientMessage, snake_case `type` tag):** `join`, `start_game`, `set_order {player_ids}`, `end_turn`, `claim_turn`, `undo_turn`, `skip_player {player_id}`, `remove_player {player_id}`, `nudge`.

**Server error codes:** `not_authorized`, `not_found`, `wrong_state`, `not_your_turn`, `nudge_cooldown` (plus join-validation errors).

**Known gap — nudge cooldown:** the `nudge_cooldown` error carries **no remaining seconds**. The design's "Wait 6s" countdown is therefore a **client-side 10s timer** the frontend runs itself on nudge / on receiving the cooldown error.

---

## 2. Stack & project layout

**Location:** `frontend/` in this repo. Vite builds to `frontend/dist/`, served by Actix as the production `static_dir`.

**Stack:** Vue 3 (`<script setup>` + TypeScript) · Vite · Pinia · Vue Router (history mode) · `vue-i18n` · Vitest + Vue Test Utils · Playwright · `qrcode` (canvas QR, no framework deps).

```
frontend/
  index.html
  vite.config.ts            # dev proxy: /api + /ws -> 127.0.0.1:8080
  playwright.config.ts       # webServer: boot Rust backend + vite preview
  src/
    main.ts
    App.vue                  # router-view + global ToastHost
    router/index.ts          # 2 routes: / and /room/:code
    i18n/
      index.ts               # vue-i18n setup (en only for v1)
      locales/en.json        # all user-visible copy
    styles/
      tokens.css             # the :root block, copied verbatim from the design
      base.css               # resets, @font-face (self-hosted fonts)
    assets/fonts/            # Hanken Grotesk + JetBrains Mono (self-hosted)
    types/wire.ts            # TS mirror of ServerMessage/ClientMessage/PublicRoom
    services/
      socket.ts              # raw WS: connect, JSON frame, auto-reconnect
      sound.ts               # Web Audio chime (no asset file)
      haptics.ts             # navigator.vibrate wrapper (no-op fallback)
      wakeLock.ts            # acquire/release + visibility re-acquire
    stores/
      room.ts                # Pinia: single source of truth
    composables/
      useReducedMotion.ts
    views/
      LandingView.vue        # screens 01 Create / 02 Join
      RoomView.vue           # switches Lobby/Active subviews
    components/
      lobby/   active/   player/   overlays/   ui/
  tests/                     # *.spec.ts unit alongside src; e2e/ for Playwright
```

**Design tokens:** the `:root` block (lines 236–279 of the export) becomes `tokens.css` **verbatim** — `--tt-*` custom properties consumed via plain CSS.

**Fonts:** Hanken Grotesk + JetBrains Mono are **self-hosted** (vendored + `@font-face`), not CDN-loaded — the app is self-hosted on Hetzner and we avoid third-party runtime fetches.

---

## 3. State & data flow

**Layering:** `socket.ts` owns the wire; `stores/room.ts` owns the truth; components are thin.

### `services/socket.ts`
- `connect(code)` opens `GET /ws/{code}`. The URL is built from `window.location` — the secure WebSocket scheme when the page is served over HTTPS, otherwise the plaintext scheme for local dev. **Never a hardcoded plaintext-scheme literal in source** (the semgrep PostToolUse hook blocks those; deriving the scheme from `location.protocol` is also the correct production behavior, since prod is HTTPS and therefore always secure-WS).
- Parses each inbound frame as a `ServerMessage` and hands it to a callback. Exposes `send(msg: ClientMessage)` with JSON framing.
- **Auto-reconnect:** on unexpected close, exponential backoff (≈0.5s→8s cap); on reopen, re-send `join {player_token}` from the persisted token. Surfaces `connecting | open | reconnecting` status the store reflects (drives the "Reconnecting…" banner).

### `stores/room.ts` (Pinia) — single source of truth
- **State:** `me: {playerId, token} | null` · `room: PublicRoom | null` · `connStatus` · `lastError: {code, message} | null` · `nudgeCooldownEndsAt: number | null`.
- **Inbound handlers:**
  - `welcome` → store `playerId` + `token`; persist token to localStorage keyed by room code.
  - `room_state` → replace `room`.
  - `nudged` → trigger toast + haptics + sound.
  - `error` → set `lastError` (toast); if code is `nudge_cooldown`, start the **client-side 10s timer**.
- **Actions** (thin `send` wrappers): `startGame`, `setOrder(ids)`, `endTurn`, `claimTurn`, `undoTurn`, `skipPlayer(id)`, `removePlayer(id)`, `nudge()`.
- **Derived getters** (where screen logic lives; the focus of unit tests):
  - `isHost`, `currentPlayer`, `isMyTurn` (`current_player_id === me.playerId`)
  - `myPosition` / `playersAway` (index distance from current → "you're 2 away")
  - `upNext` (next N after current → the "up next" strip)
  - `amINext` (drives the Claim-turn screen)
  - `phase`: `'landing' | 'lobby' | 'active'` and the active sub-state → which subview `RoomView` renders.

### Entry / persistence
- On `/room/:code` load, if a token exists in localStorage for that code, auto-`join` with it (refresh/reconnect restores the player); otherwise show the Join form to collect a name.
- A host arriving from create already holds `{playerId, token}` in memory (and persisted).

### Stale-token / room-gone recovery
If auto-`join` with a saved token is rejected because the room no longer exists (server restart or idle cleanup → `not_found`), the frontend **clears the stale token for that code, shows the "Room not found" toast, and routes back to `/`**. No dead-room or infinite-spinner states.

### Token storage note
Tokens are reconnect credentials; localStorage (per room code) is acceptable for this app. They are already kept out of broadcasts server-side (`PublicRoom` omits them) and are never logged.

---

## 4. Screens, components & device features

### Routes → the 9 designed screens

| Route / container | Design screens | Shown when |
|---|---|---|
| `/` `LandingView` | 01 Create · 02 Join | no room joined |
| `/room/:code` `RoomView` → `LobbySubview` | 03 Lobby (host) · 04 Lobby (waiting) | `room.state==='lobby'`; host vs non-host by `isHost` |
| `/room/:code` `RoomView` → `ActiveSubview` | 05 Your Turn · 06 Not-your-turn · 07 Claim-turn | `room.state==='active'`, branched by `isMyTurn` / `amINext` |

Approach: **few routes, state-driven screens.** Turn changes are server-pushed *state*, not navigations — the URL stays stable across turns (correct for share/reconnect).

### Overlays (layered, not routes)
- `NudgeToast` (screen 08, triggered by `nudged`)
- `HostSheet` (screen 09, bottom-sheet: skip / undo / reorder / remove)
- global `ToastHost` for the banner/toast set (reconnecting, not-your-turn, host-only, nudge-cooldown, room-not-found, turn-passed+undo).

### Component inventory (design Section 04 — every state pre-specified)
- `player/PlayerRow.vue` — host / connected / disconnected / skipped (color **+ label + icon**, never color alone)
- `lobby/ShareCodeBlock.vue` — code, copy-link (Clipboard/Web-Share), Show-QR, "copied" confirm
- `active/TurnButton.vue` — the big DONE: default / pressed / disabled
- `active/NudgeButton.vue` — fires `nudge()`, runs the client-side 10s cooldown bar + "Wait Ns" label
- `active/TurnEmblem.vue` — pulsing "!" emblem (respects reduced-motion)
- `ui/` — `AppButton`, `Toast`, `Banner`, `CodeInput` (6-char join), `Avatar`
- Lobby reorder uses native HTML5 drag (design shows a drag handle) → emits `set_order`.

### Device features (all v1; graceful no-ops where unsupported)
- **Becoming your turn** (`isMyTurn` false→true): `haptics.vibrate([0,80,40,80])` + `sound.chime()` + `wakeLock.acquire()`; release wake lock when no longer your turn.
- **Nudge received** (`nudged`): toast drop + shake (reduced-motion: no shake), double-buzz, emblem pulse speeds ~3s.
- **`useReducedMotion`** gates all wipe / pulse / shake / sheen; vibration + sound still fire (not motion). `aria-live="assertive"` announces "It's your turn" and nudges.
- **QR:** `qrcode` renders the join URL (`https://<host>/room/<code>`) to a canvas in the lobby.

### Accessibility (carried from the design)
≥56px touch targets, documented AA/AAA contrast ratios, status conveyed by label/icon not hue alone.

### i18n
`vue-i18n`, **English-only** locale for v1, but **every user-visible string goes through `$t('key')`** — no hardcoded template text. Server `error.code` maps to an i18n key, so the frontend owns user-facing wording (translatable later). The `vue-i18n-auditor` agent enforces compliance before release.

---

## 5. Backend change (Rust)

Add an SPA fallback so client deep links survive a hard refresh. Currently `actix_files::Files::new("/", &static_dir).index_file("index.html")` has no fallback, so `GET /room/ABC123` would 404 on refresh.

- In `server.rs`, give the `Files` service a `default_handler` that returns `index.html` for unmatched paths.
- `/api/*` and `/ws/{code}` are registered explicitly before the catch-all, so only genuine client routes (`/room/:code`) fall through.
- Test: `GET /room/ABC123` returns the SPA `index.html`; `GET /api/rooms` and `/ws/...` are unaffected.
- Canonical clippy lints table already in place — keep CI green.

---

## 6. Testing

Clean test pyramid: Vitest owns state **derivation**; Playwright owns the real distributed **round-trip**. They don't overlap.

### Vitest + Vue Test Utils (the bulk)
- Store getters (`isMyTurn`, `playersAway`, `upNext`, `amINext`, `phase`) fed crafted `room_state` objects.
- Inbound-message handling: `welcome` persists token; `nudge_cooldown` starts the timer; reconnect re-joins with token; stale-token → room-gone recovery clears token + routes home.
- Component tests: `PlayerRow` states, `TurnButton` states, `NudgeButton` cooldown, `CodeInput`.

### Playwright E2E (`frontend/tests/e2e/`)
- `webServer` boots the Rust backend (`cargo run`) + `vite preview`.
- **Multi-context happy path:** host creates → 2 joiners join (separate `browser.newContext()`) → host starts → each page asserts the correct turn screen → Done advances the turn → propagation asserted across all three pages.
- Error toasts: `not_your_turn`, `nudge_cooldown`, host-only action.
- One **reconnect** test: close the WS, assert token-based rejoin restores state.
- Device APIs stubbed via `addInitScript` and **spied** (vibrate / wakeLock called; QR canvas present) — not deeply asserted (can't truly verify a buzz headless).

### Verification gate (before "done")
`npm run build`, `vitest run`, `playwright test`, backend `cargo test`, and `cargo clippy --all-targets -- -D warnings` all green.

---

## 7. Build & dev workflow

- **Dev:** `cargo run` (backend :8080) + `npm run dev` (Vite :5173). Vite proxies `/api` and `/ws` (`ws: true`) to :8080 — hot reload on the frontend, real backend behind it.
- **Prod:** `npm run build` → `frontend/dist/` → Actix serves it as `static_dir`.
- **Docker:** add a frontend build stage (node → build → copy `dist` into the final image alongside the Rust binary).
- **CI:** add a frontend job (install, lint, typecheck, unit test, build). The existing Docker-build validation job covers the combined image. E2E may run in CI behind the booted backend (or be a documented local/pre-release gate if CI runtime is a concern).

---

## 8. Future / out of scope (v1)

- **Server-side durability (likely needed once hosted).** The backend keeps all room state **in memory** (`Registry` + idle-room cleanup), so **any server restart — including a routine release/deploy — wipes all live sessions**, and reconnect tokens cannot restore a room that no longer exists server-side. v1 ships ephemeral to validate the application; once it's hosted online, durable room storage (e.g. a database or persistent store, plus reconnect/resume across restarts) will most likely be required. Tracked as the first post-validation backend workstream.
- **PWA / offline / installable** — service worker, manifest, add-to-home-screen. Deferred.
- **Additional locales** — i18n scaffolding is in place (en only); add locales later.
- **Spectator mode, game history, scores** — not in v1.
