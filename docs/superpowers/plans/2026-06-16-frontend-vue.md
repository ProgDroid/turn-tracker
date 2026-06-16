# Turn Tracker Vue Frontend Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the `frontend/` Vue 3 SPA that implements the approved design system and 9 screens, driven entirely by the existing Turn Tracker WebSocket backend, plus one small Actix change so deep links survive refresh.

**Architecture:** A thin `socket.ts` owns the raw WebSocket; a Pinia `room` store is the single source of truth (consumes `room_state` broadcasts, exposes derived getters); components are thin and read getters / call store actions. Two routes (`/`, `/room/:code`); the 9 screens are state-driven subviews + overlays, not separate routes.

**Tech Stack:** Vue 3 (`<script setup>` + TypeScript), Vite, Pinia, Vue Router (history), vue-i18n, Vitest + Vue Test Utils, Playwright, `qrcode`. Backend: Rust/Actix (one fallback-route change).

**Spec:** `docs/superpowers/specs/2026-06-16-frontend-vue-design.md`

---

## Conventions for every task

- **WebSocket scheme rule:** never write a plaintext-scheme literal (`w` + `s` + `:` + `//`) in source. Build the scheme from `location.protocol` (`location.protocol === 'https:' ? 'wss' : 'ws'`). The repo's semgrep PostToolUse hook blocks the literal. Tests that need a URL string should build it the same way or use `wss://` (secure) literals only.
- **Commit via the Bash tool, not PowerShell** (PowerShell prepends a BOM to commit subjects in this environment).
- **i18n:** no hardcoded user-visible strings in templates — every one goes through `t('key')` with a matching entry in `src/i18n/locales/en.json`.
- **Run frontend commands from `frontend/`** unless a path says otherwise.

---

## File structure

```
frontend/
  index.html
  package.json
  tsconfig.json  tsconfig.node.json
  vite.config.ts
  playwright.config.ts
  env.d.ts
  src/
    main.ts
    App.vue
    router/index.ts
    i18n/index.ts
    i18n/locales/en.json
    styles/tokens.css
    styles/base.css
    assets/fonts/...            (vendored font files)
    types/wire.ts
    services/socket.ts
    services/sound.ts
    services/haptics.ts
    services/wakeLock.ts
    composables/useReducedMotion.ts
    stores/room.ts
    views/LandingView.vue
    views/RoomView.vue
    components/ui/AppButton.vue
    components/ui/Toast.vue
    components/ui/CodeInput.vue
    components/ui/Avatar.vue
    components/player/PlayerRow.vue
    components/lobby/ShareCodeBlock.vue
    components/lobby/LobbySubview.vue
    components/active/TurnButton.vue
    components/active/NudgeButton.vue
    components/active/TurnEmblem.vue
    components/active/ActiveSubview.vue
    components/overlays/ToastHost.vue
    components/overlays/NudgeToast.vue
    components/overlays/HostSheet.vue
    src/**/__tests__/*.spec.ts   (unit tests beside source)
  tests/e2e/*.spec.ts            (Playwright)
src/server.rs                    (backend: SPA fallback)
```

---

## Task 1: Backend SPA fallback route

**Files:**
- Modify: `src/server.rs`

The current static service (`src/server.rs:56`) is `actix_files::Files::new("/", &static_dir).index_file("index.html")` with no fallback, so `GET /room/ABC123` 404s on refresh. Add a `default_handler` that serves `index.html` for unmatched paths, extracted into a testable helper. `/api/*` and `/ws/{code}` are registered first (`config()`), so only genuine client routes fall through.

- [ ] **Step 1: Write the failing test**

Add to the bottom of `src/server.rs`:

```rust
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
        let app = test::init_service(
            App::new().service(spa_files(&static_dir)),
        )
        .await;

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
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib server::tests::deep_link_falls_back_to_index_html`
Expected: FAIL — `cannot find function spa_files in this scope`.

- [ ] **Step 3: Add the `spa_files` helper and use it in `run`**

In `src/server.rs`, add imports at the top:

```rust
use actix_files::{Files, NamedFile};
use actix_web::dev::{ServiceRequest, ServiceResponse, fn_service};
```

Add the helper above `run`:

```rust
/// Static-file service for the SPA: serves files from `static_dir`, and falls
/// back to `index.html` for any unmatched path so client-side deep links
/// (e.g. `/room/ABC123`) survive a hard refresh. Explicit `/api/*` and
/// `/ws/{code}` routes are registered before this service, so only genuine
/// client routes reach the fallback.
pub fn spa_files(static_dir: &str) -> Files {
    let index = std::path::Path::new(static_dir).join("index.html");
    Files::new("/", static_dir)
        .index_file("index.html")
        .default_handler(fn_service(move |req: ServiceRequest| {
            let index = index.clone();
            async move {
                let (req, _) = req.into_parts();
                let file = NamedFile::open_async(&index).await?;
                let res = file.into_response(&req);
                Ok::<ServiceResponse, std::io::Error>(ServiceResponse::new(req, res))
            }
        }))
}
```

Replace the `.service(actix_files::Files::new(...))` line in `run` (`src/server.rs:56`) with:

```rust
            .service(spa_files(&static_dir))
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --lib server::tests::deep_link_falls_back_to_index_html`
Expected: PASS.

- [ ] **Step 5: Verify the whole backend is still green**

Run: `cargo test` then `cargo clippy --all-targets -- -D warnings`
Expected: all pass, no warnings.

- [ ] **Step 6: Commit**

```bash
git add src/server.rs
git commit -m "feat: serve index.html fallback for SPA deep links"
```

---

## Task 2: Scaffold the Vite + Vue + TS project

**Files:**
- Create: `frontend/package.json`, `frontend/tsconfig.json`, `frontend/tsconfig.node.json`, `frontend/vite.config.ts`, `frontend/index.html`, `frontend/env.d.ts`, `frontend/src/main.ts`, `frontend/src/App.vue`

- [ ] **Step 1: Create `frontend/package.json`**

```json
{
  "name": "turn-tracker-frontend",
  "private": true,
  "type": "module",
  "scripts": {
    "dev": "vite",
    "build": "vue-tsc --noEmit && vite build",
    "preview": "vite preview --port 4173",
    "test": "vitest run",
    "test:watch": "vitest",
    "e2e": "playwright test"
  },
  "dependencies": {
    "pinia": "^2.2.0",
    "qrcode": "^1.5.4",
    "vue": "^3.5.0",
    "vue-i18n": "^10.0.0",
    "vue-router": "^4.4.0"
  },
  "devDependencies": {
    "@playwright/test": "^1.48.0",
    "@types/qrcode": "^1.5.5",
    "@vitejs/plugin-vue": "^5.1.0",
    "@vue/test-utils": "^2.4.6",
    "jsdom": "^25.0.0",
    "typescript": "^5.6.0",
    "vite": "^5.4.0",
    "vitest": "^2.1.0",
    "vue-tsc": "^2.1.0"
  }
}
```

> Note: pin exact versions at install time with Context7/npm if any of the above have moved; the carets are floors, not guarantees.

- [ ] **Step 2: Create `frontend/tsconfig.json`**

```json
{
  "compilerOptions": {
    "target": "ES2022",
    "module": "ESNext",
    "moduleResolution": "bundler",
    "strict": true,
    "jsx": "preserve",
    "lib": ["ES2022", "DOM", "DOM.Iterable"],
    "skipLibCheck": true,
    "noUnusedLocals": true,
    "noUnusedParameters": true,
    "types": ["vitest/globals"],
    "baseUrl": ".",
    "paths": { "@/*": ["src/*"] }
  },
  "include": ["src/**/*.ts", "src/**/*.vue", "tests/**/*.ts"]
}
```

- [ ] **Step 3: Create `frontend/tsconfig.node.json`**

```json
{
  "compilerOptions": {
    "module": "ESNext",
    "moduleResolution": "bundler",
    "types": ["node"]
  },
  "include": ["vite.config.ts", "playwright.config.ts"]
}
```

- [ ] **Step 4: Create `frontend/vite.config.ts`** (dev proxy to the Rust backend; vitest config inline)

```ts
import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import { fileURLToPath, URL } from 'node:url'

export default defineConfig({
  plugins: [vue()],
  resolve: {
    alias: { '@': fileURLToPath(new URL('./src', import.meta.url)) },
  },
  server: {
    proxy: {
      '/api': 'http://127.0.0.1:8080',
      '/ws': { target: 'http://127.0.0.1:8080', ws: true },
    },
  },
  test: {
    environment: 'jsdom',
    globals: true,
  },
})
```

- [ ] **Step 5: Create `frontend/env.d.ts`**

```ts
/// <reference types="vite/client" />
declare module '*.vue' {
  import type { DefineComponent } from 'vue'
  const component: DefineComponent<{}, {}, any>
  export default component
}
```

- [ ] **Step 6: Create `frontend/index.html`**

```html
<!doctype html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover" />
    <title>Turn Tracker</title>
  </head>
  <body>
    <div id="app"></div>
    <script type="module" src="/src/main.ts"></script>
  </body>
</html>
```

- [ ] **Step 7: Create `frontend/src/App.vue`** (placeholder; expanded in Task 13)

```vue
<script setup lang="ts"></script>

<template>
  <RouterView />
</template>
```

- [ ] **Step 8: Create `frontend/src/main.ts`** (router/i18n/pinia wired in later tasks; minimal now)

```ts
import { createApp } from 'vue'
import App from './App.vue'

createApp(App).mount('#app')
```

- [ ] **Step 9: Install and verify the build**

Run (from `frontend/`): `npm install` then `npm run build`
Expected: install succeeds; `vue-tsc` + `vite build` complete with no errors, producing `frontend/dist/`.

- [ ] **Step 10: Add `frontend/.gitignore` and commit**

Create `frontend/.gitignore`:

```
node_modules
dist
playwright-report
test-results
```

```bash
git add frontend/.gitignore frontend/package.json frontend/package-lock.json frontend/tsconfig.json frontend/tsconfig.node.json frontend/vite.config.ts frontend/index.html frontend/env.d.ts frontend/src/App.vue frontend/src/main.ts
git commit -m "chore: scaffold Vue 3 + Vite + TS frontend"
```

---

## Task 3: Design tokens, base styles, fonts

**Files:**
- Create: `frontend/src/styles/tokens.css`, `frontend/src/styles/base.css`
- Create: `frontend/src/assets/fonts/` (vendored font files)

- [ ] **Step 1: Create `frontend/src/styles/tokens.css`**

Copy the `:root` block verbatim from `design-export/Turn Tracker Design System.dc.html` lines 236–279 (strip the HTML `<span>` syntax-highlight wrappers — keep only the CSS). The result:

```css
:root {
  /* surfaces */
  --tt-surface-0: #0a0a0c;
  --tt-surface-1: #141418;
  --tt-surface-2: #1c1c21;
  --tt-surface-3: #2a2a31;
  --tt-border: #2e2e36;
  /* text */
  --tt-text: #f4f4f5; --tt-text-muted: #a1a1aa; --tt-text-faint: #6b6b75;
  /* accent — "your turn" owns this color */
  --tt-accent: #34d399;
  --tt-accent-strong: #10b981;
  --tt-accent-glow: rgba(52, 211, 153, 0.55);
  --tt-on-accent: #04140d;
  /* semantic roles */
  --tt-success: #34d399; --tt-danger: #fb7185; --tt-on-danger: #2a0710;
  --tt-warning: #fbbf24; --tt-info: #38bdf8;
  /* type */
  --tt-font-sans: "Hanken Grotesk", system-ui, sans-serif;
  --tt-font-mono: "JetBrains Mono", ui-monospace, monospace;
  --tt-fs-display: 64px; --tt-fs-h1: 32px; --tt-fs-h2: 24px;
  --tt-fs-body: 16px; --tt-fs-sm: 14px; --tt-fs-xs: 12px;
  /* spacing (4px base) */
  --tt-1: 4px; --tt-2: 8px; --tt-3: 12px; --tt-4: 16px;
  --tt-5: 24px; --tt-6: 32px; --tt-7: 48px; --tt-8: 64px;
  /* radii */
  --tt-r-sm: 8px; --tt-r-md: 12px; --tt-r-lg: 18px; --tt-r-xl: 24px; --tt-r-full: 999px;
  /* elevation */
  --tt-shadow: 0 8px 24px -8px rgba(0, 0, 0, 0.7);
  --tt-shadow-lg: 0 24px 50px -16px rgba(0, 0, 0, 0.8);
  --tt-glow: 0 0 28px -2px var(--tt-accent-glow);
  /* motion */
  --tt-dur-fast: 120ms; --tt-dur: 200ms; --tt-dur-slow: 320ms; --tt-dur-hero: 560ms;
  --tt-ease: cubic-bezier(0.2, 0, 0, 1);
  --tt-ease-spring: cubic-bezier(0.34, 1.56, 0.64, 1);
}
```

- [ ] **Step 2: Vendor the fonts**

Download Hanken Grotesk (weights 400,500,600,700,800) and JetBrains Mono (400,500,700) as `.woff2` into `frontend/src/assets/fonts/`. (Use the Google Fonts files or fontsource packages; if using `@fontsource`, add them to `package.json` deps instead and import in `base.css`.) For the vendored approach, name them e.g. `hanken-grotesk-700.woff2`.

- [ ] **Step 3: Create `frontend/src/styles/base.css`**

```css
@font-face {
  font-family: "Hanken Grotesk";
  font-weight: 800;
  font-display: swap;
  src: url("@/assets/fonts/hanken-grotesk-800.woff2") format("woff2");
}
/* repeat @font-face for each vendored weight: Hanken 400/500/600/700/800,
   JetBrains Mono 400/500/700 — same pattern, correct file + font-weight. */

* { box-sizing: border-box; }
html, body, #app { height: 100%; }
body {
  margin: 0;
  background: var(--tt-surface-0);
  color: var(--tt-text);
  font-family: var(--tt-font-sans);
  -webkit-font-smoothing: antialiased;
}

@keyframes ttPulse { 0% { transform: scale(.7); opacity: .65; } 100% { transform: scale(2.3); opacity: 0; } }
@keyframes ttShake { 10%,90%{transform:translateX(-2px)} 20%,80%{transform:translateX(3px)} 30%,50%,70%{transform:translateX(-7px)} 40%,60%{transform:translateX(7px)} }
@keyframes ttBlink { 50% { opacity: .3; } }
@keyframes ttSpin { to { transform: rotate(360deg); } }
@keyframes ttSheen { 0% { transform: translateX(-120%); } 60%,100% { transform: translateX(320%); } }
```

- [ ] **Step 4: Import styles in `main.ts`**

Edit `frontend/src/main.ts` to add at the top:

```ts
import './styles/tokens.css'
import './styles/base.css'
```

- [ ] **Step 5: Verify build**

Run: `npm run build`
Expected: PASS, fonts and CSS bundled.

- [ ] **Step 6: Commit**

```bash
git add frontend/src/styles frontend/src/assets/fonts frontend/src/main.ts
git commit -m "feat: add design tokens, base styles, self-hosted fonts"
```

---

## Task 4: Wire types (TS mirror of the backend contract)

**Files:**
- Create: `frontend/src/types/wire.ts`
- Test: `frontend/src/types/__tests__/wire.spec.ts`

Mirror `src/wire.rs`. These types are the contract; getting them exact prevents whole classes of bugs.

- [ ] **Step 1: Write the failing test** (a compile + shape guard)

`frontend/src/types/__tests__/wire.spec.ts`:

```ts
import { describe, it, expect } from 'vitest'
import type { ServerMessage, ClientMessage, PublicRoom } from '@/types/wire'

describe('wire types', () => {
  it('parses a room_state message shape', () => {
    const msg = JSON.parse(
      '{"type":"room_state","room":{"code":"GR7K9P","state":"lobby","players":[{"id":"p1","name":"Sam","is_host":true,"connected":true}],"current_player_id":null}}',
    ) as ServerMessage
    expect(msg.type).toBe('room_state')
    if (msg.type === 'room_state') {
      const room: PublicRoom = msg.room
      expect(room.players[0].is_host).toBe(true)
      expect(room.current_player_id).toBeNull()
    }
  })

  it('builds a typed client message', () => {
    const m: ClientMessage = { type: 'skip_player', player_id: 'p2' }
    expect(JSON.stringify(m)).toContain('skip_player')
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

Run: `npx vitest run src/types/__tests__/wire.spec.ts`
Expected: FAIL — cannot find module `@/types/wire`.

- [ ] **Step 3: Create `frontend/src/types/wire.ts`**

```ts
export type PlayerId = string

export interface PublicPlayer {
  id: PlayerId
  name: string
  is_host: boolean
  connected: boolean
}

export type PublicState = 'lobby' | 'active'

export interface PublicRoom {
  code: string
  state: PublicState
  players: PublicPlayer[]
  current_player_id: PlayerId | null
}

export type ServerMessage =
  | { type: 'welcome'; player_id: PlayerId; token: string }
  | { type: 'room_state'; room: PublicRoom }
  | { type: 'nudged' }
  | { type: 'error'; code: string; message: string }

export type ClientMessage =
  | { type: 'join'; player_token?: string; player_name?: string }
  | { type: 'start_game' }
  | { type: 'set_order'; player_ids: PlayerId[] }
  | { type: 'end_turn' }
  | { type: 'claim_turn' }
  | { type: 'undo_turn' }
  | { type: 'skip_player'; player_id: PlayerId }
  | { type: 'remove_player'; player_id: PlayerId }
  | { type: 'nudge' }

/** Known server error codes (see src/ws/dispatch.rs). */
export type ErrorCode =
  | 'not_authorized'
  | 'not_found'
  | 'wrong_state'
  | 'not_your_turn'
  | 'nudge_cooldown'
```

- [ ] **Step 4: Run test to verify it passes**

Run: `npx vitest run src/types/__tests__/wire.spec.ts`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/types
git commit -m "feat: add TS wire types mirroring backend contract"
```

---

## Task 5: Socket service

**Files:**
- Create: `frontend/src/services/socket.ts`
- Test: `frontend/src/services/__tests__/socket.spec.ts`

A small class wrapping `WebSocket`: builds the URL from `location`, frames JSON, parses inbound to `ServerMessage`, auto-reconnects with backoff, exposes a status. Injectable `WebSocket` factory for tests.

- [ ] **Step 1: Write the failing test**

`frontend/src/services/__tests__/socket.spec.ts`:

```ts
import { describe, it, expect, vi } from 'vitest'
import { RoomSocket } from '@/services/socket'
import type { ServerMessage } from '@/types/wire'

class FakeWS {
  static OPEN = 1
  static CLOSED = 3
  readyState = 1
  onopen: (() => void) | null = null
  onmessage: ((e: { data: string }) => void) | null = null
  onclose: (() => void) | null = null
  onerror: (() => void) | null = null
  sent: string[] = []
  constructor(public url: string) {}
  send(d: string) { this.sent.push(d) }
  close() { this.readyState = 3; this.onclose?.() }
  emitOpen() { this.onopen?.() }
  emitMessage(m: ServerMessage) { this.onmessage?.({ data: JSON.stringify(m) }) }
}

function make() {
  const created: FakeWS[] = []
  const factory = (url: string) => {
    const ws = new FakeWS(url)
    created.push(ws)
    return ws as unknown as WebSocket
  }
  return { created, factory }
}

describe('RoomSocket', () => {
  it('builds a secure URL when page is https', () => {
    const { created, factory } = make()
    const s = new RoomSocket(factory, { protocol: 'https:', host: 'app.example' })
    s.connect('GR7K9P', () => {})
    expect(created[0].url).toBe('wss://app.example/ws/GR7K9P')
  })

  it('builds a plaintext URL for local http dev', () => {
    const { created, factory } = make()
    const s = new RoomSocket(factory, { protocol: 'http:', host: 'localhost:5173' })
    s.connect('GR7K9P', () => {})
    expect(created[0].url.startsWith('ws://localhost:5173/ws/')).toBe(true)
  })

  it('parses inbound messages and forwards them', () => {
    const { created, factory } = make()
    const got: ServerMessage[] = []
    const s = new RoomSocket(factory, { protocol: 'https:', host: 'h' })
    s.connect('C', (m) => got.push(m))
    created[0].emitOpen()
    created[0].emitMessage({ type: 'nudged' })
    expect(got).toEqual([{ type: 'nudged' }])
  })

  it('send() frames JSON', () => {
    const { created, factory } = make()
    const s = new RoomSocket(factory, { protocol: 'https:', host: 'h' })
    s.connect('C', () => {})
    created[0].emitOpen()
    s.send({ type: 'end_turn' })
    expect(created[0].sent).toEqual(['{"type":"end_turn"}'])
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

Run: `npx vitest run src/services/__tests__/socket.spec.ts`
Expected: FAIL — cannot find module `@/services/socket`.

- [ ] **Step 3: Create `frontend/src/services/socket.ts`**

```ts
import type { ClientMessage, ServerMessage } from '@/types/wire'

export type ConnStatus = 'idle' | 'connecting' | 'open' | 'reconnecting' | 'closed'
type Loc = Pick<Location, 'protocol' | 'host'>
type WSFactory = (url: string) => WebSocket
type OnMessage = (msg: ServerMessage) => void
type OnStatus = (status: ConnStatus) => void

const BACKOFF_BASE = 500
const BACKOFF_MAX = 8000

export class RoomSocket {
  private ws: WebSocket | null = null
  private code = ''
  private onMessage: OnMessage = () => {}
  private onStatus: OnStatus = () => {}
  private attempts = 0
  private closedByUs = false
  private timer: ReturnType<typeof setTimeout> | null = null

  constructor(
    private factory: WSFactory = (url) => new WebSocket(url),
    private loc: Loc = window.location,
  ) {}

  onStatusChange(cb: OnStatus) { this.onStatus = cb }

  private url(code: string): string {
    const scheme = this.loc.protocol === 'https:' ? 'wss' : 'ws'
    return `${scheme}://${this.loc.host}/ws/${code}`
  }

  connect(code: string, onMessage: OnMessage) {
    this.code = code
    this.onMessage = onMessage
    this.closedByUs = false
    this.open()
  }

  private open() {
    this.onStatus(this.attempts === 0 ? 'connecting' : 'reconnecting')
    const ws = this.factory(this.url(this.code))
    this.ws = ws
    ws.onopen = () => {
      this.attempts = 0
      this.onStatus('open')
    }
    ws.onmessage = (e: MessageEvent) => {
      try {
        this.onMessage(JSON.parse(String(e.data)) as ServerMessage)
      } catch {
        /* ignore malformed frame */
      }
    }
    ws.onclose = () => {
      if (this.closedByUs) {
        this.onStatus('closed')
        return
      }
      this.scheduleReconnect()
    }
    ws.onerror = () => ws.close()
  }

  private scheduleReconnect() {
    this.attempts += 1
    const delay = Math.min(BACKOFF_MAX, BACKOFF_BASE * 2 ** (this.attempts - 1))
    this.onStatus('reconnecting')
    this.timer = setTimeout(() => this.open(), delay)
  }

  send(msg: ClientMessage) {
    this.ws?.send(JSON.stringify(msg))
  }

  close() {
    this.closedByUs = true
    if (this.timer) clearTimeout(this.timer)
    this.ws?.close()
  }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `npx vitest run src/services/__tests__/socket.spec.ts`
Expected: PASS (all 4).

- [ ] **Step 5: Commit**

```bash
git add frontend/src/services/socket.ts frontend/src/services/__tests__/socket.spec.ts
git commit -m "feat: add auto-reconnecting room socket service"
```

---

## Task 6: Token persistence helper

**Files:**
- Create: `frontend/src/services/tokenStore.ts`
- Test: `frontend/src/services/__tests__/tokenStore.spec.ts`

Per-room-code reconnect token in localStorage.

- [ ] **Step 1: Write the failing test**

`frontend/src/services/__tests__/tokenStore.spec.ts`:

```ts
import { describe, it, expect, beforeEach } from 'vitest'
import { saveToken, loadToken, clearToken } from '@/services/tokenStore'

describe('tokenStore', () => {
  beforeEach(() => localStorage.clear())

  it('saves and loads a token per room code', () => {
    saveToken('GR7K9P', 'tok-1')
    expect(loadToken('GR7K9P')).toBe('tok-1')
    expect(loadToken('OTHER')).toBeNull()
  })

  it('clears a token', () => {
    saveToken('GR7K9P', 'tok-1')
    clearToken('GR7K9P')
    expect(loadToken('GR7K9P')).toBeNull()
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

Run: `npx vitest run src/services/__tests__/tokenStore.spec.ts`
Expected: FAIL — cannot find module.

- [ ] **Step 3: Create `frontend/src/services/tokenStore.ts`**

```ts
const key = (code: string) => `tt:token:${code.toUpperCase()}`

export function saveToken(code: string, token: string): void {
  localStorage.setItem(key(code), token)
}

export function loadToken(code: string): string | null {
  return localStorage.getItem(key(code))
}

export function clearToken(code: string): void {
  localStorage.removeItem(key(code))
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `npx vitest run src/services/__tests__/tokenStore.spec.ts`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/services/tokenStore.ts frontend/src/services/__tests__/tokenStore.spec.ts
git commit -m "feat: add per-room reconnect token persistence"
```

---

## Task 7: Pinia room store — state, getters, message handling

**Files:**
- Create: `frontend/src/stores/room.ts`
- Test: `frontend/src/stores/__tests__/room.spec.ts`

The single source of truth. This is the logic-heavy task — getters get full unit coverage. The store takes an injectable socket so tests don't touch the network.

- [ ] **Step 1: Write the failing tests**

`frontend/src/stores/__tests__/room.spec.ts`:

```ts
import { describe, it, expect, beforeEach, vi } from 'vitest'
import { setActivePinia, createPinia } from 'pinia'
import { useRoomStore } from '@/stores/room'
import type { ServerMessage, PublicRoom } from '@/types/wire'

function room(partial: Partial<PublicRoom> = {}): PublicRoom {
  return {
    code: 'GR7K9P',
    state: 'active',
    players: [
      { id: 'p1', name: 'Sam', is_host: true, connected: true },
      { id: 'p2', name: 'Alice', is_host: false, connected: true },
      { id: 'p3', name: 'Bob', is_host: false, connected: true },
    ],
    current_player_id: 'p2',
    ...partial,
  }
}

const fakeSocket = () => ({
  connect: vi.fn(),
  send: vi.fn(),
  close: vi.fn(),
  onStatusChange: vi.fn(),
})

describe('room store', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    localStorage.clear()
  })

  it('welcome stores identity and persists token', () => {
    const s = useRoomStore()
    s._setSocket(fakeSocket() as any)
    s.codeInView = 'GR7K9P'
    s._handle({ type: 'welcome', player_id: 'p1', token: 'tok' } as ServerMessage)
    expect(s.me).toEqual({ playerId: 'p1', token: 'tok' })
    expect(localStorage.getItem('tt:token:GR7K9P')).toBe('tok')
  })

  it('room_state replaces the room', () => {
    const s = useRoomStore()
    s._handle({ type: 'room_state', room: room() })
    expect(s.room?.current_player_id).toBe('p2')
  })

  it('isMyTurn reflects current_player_id vs me', () => {
    const s = useRoomStore()
    s.me = { playerId: 'p2', token: 't' }
    s._handle({ type: 'room_state', room: room() })
    expect(s.isMyTurn).toBe(true)
    s.me = { playerId: 'p1', token: 't' }
    expect(s.isMyTurn).toBe(false)
  })

  it('playersAway counts seats from me to current', () => {
    const s = useRoomStore()
    s.me = { playerId: 'p1', token: 't' } // current is p2 (index 1), me p1 index 0
    s._handle({ type: 'room_state', room: room() })
    // order p1,p2,p3 ; current p2 ; from p2 to p1 wraps: p2->p3->p1 = 2 away
    expect(s.playersAway).toBe(2)
  })

  it('amINext is true when I am the seat after current', () => {
    const s = useRoomStore()
    s.me = { playerId: 'p3', token: 't' } // current p2 index1, next index2 = p3
    s._handle({ type: 'room_state', room: room() })
    expect(s.amINext).toBe(true)
  })

  it('upNext lists players after current in order, wrapping', () => {
    const s = useRoomStore()
    s._handle({ type: 'room_state', room: room() })
    expect(s.upNext.map((p) => p.id)).toEqual(['p3', 'p1'])
  })

  it('phase derives from connection + room state', () => {
    const s = useRoomStore()
    expect(s.phase).toBe('landing')
    s._handle({ type: 'room_state', room: room({ state: 'lobby' }) })
    expect(s.phase).toBe('lobby')
    s._handle({ type: 'room_state', room: room({ state: 'active' }) })
    expect(s.phase).toBe('active')
  })

  it('nudge_cooldown error starts a client-side timer', () => {
    vi.useFakeTimers()
    const s = useRoomStore()
    s._handle({ type: 'error', code: 'nudge_cooldown', message: 'x' })
    expect(s.nudgeCooldownRemaining).toBeGreaterThan(0)
    vi.advanceTimersByTime(10_000)
    expect(s.nudgeCooldownRemaining).toBe(0)
    vi.useRealTimers()
  })

  it('stale token (not_found) during reconnect clears token and resets', () => {
    const s = useRoomStore()
    s.codeInView = 'GR7K9P'
    localStorage.setItem('tt:token:GR7K9P', 'stale')
    s.joiningWithToken = true
    s._handle({ type: 'error', code: 'not_found', message: 'x' })
    expect(localStorage.getItem('tt:token:GR7K9P')).toBeNull()
    expect(s.roomGone).toBe(true)
  })

  it('actions send the right client messages', () => {
    const sock = fakeSocket()
    const s = useRoomStore()
    s._setSocket(sock as any)
    s.endTurn()
    s.skipPlayer('p3')
    expect(sock.send).toHaveBeenCalledWith({ type: 'end_turn' })
    expect(sock.send).toHaveBeenCalledWith({ type: 'skip_player', player_id: 'p3' })
  })
})
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `npx vitest run src/stores/__tests__/room.spec.ts`
Expected: FAIL — cannot find module `@/stores/room`.

- [ ] **Step 3: Create `frontend/src/stores/room.ts`**

```ts
import { defineStore } from 'pinia'
import type {
  ClientMessage, PlayerId, PublicPlayer, PublicRoom, ServerMessage,
} from '@/types/wire'
import { RoomSocket, type ConnStatus } from '@/services/socket'
import { saveToken, loadToken, clearToken } from '@/services/tokenStore'

const NUDGE_COOLDOWN_MS = 10_000

interface Me { playerId: PlayerId; token: string }
interface SocketLike {
  connect: (code: string, cb: (m: ServerMessage) => void) => void
  send: (m: ClientMessage) => void
  close: () => void
  onStatusChange: (cb: (s: ConnStatus) => void) => void
}

interface State {
  socket: SocketLike | null
  codeInView: string
  me: Me | null
  room: PublicRoom | null
  connStatus: ConnStatus
  lastError: { code: string; message: string } | null
  nudgeCooldownEndsAt: number | null
  nudgeCooldownRemaining: number
  nudgeTimer: ReturnType<typeof setInterval> | null
  joiningWithToken: boolean
  roomGone: boolean
  nudgeReceivedAt: number | null
}

export const useRoomStore = defineStore('room', {
  state: (): State => ({
    socket: null,
    codeInView: '',
    me: null,
    room: null,
    connStatus: 'idle',
    lastError: null,
    nudgeCooldownEndsAt: null,
    nudgeCooldownRemaining: 0,
    nudgeTimer: null,
    joiningWithToken: false,
    roomGone: false,
    nudgeReceivedAt: null,
  }),

  getters: {
    isHost: (s): boolean =>
      !!s.me && !!s.room?.players.find((p) => p.id === s.me!.playerId)?.is_host,
    currentPlayer: (s): PublicPlayer | null =>
      s.room?.players.find((p) => p.id === s.room?.current_player_id) ?? null,
    isMyTurn: (s): boolean =>
      !!s.me && !!s.room && s.room.current_player_id === s.me.playerId,
    myIndex(s): number {
      if (!s.me || !s.room) return -1
      return s.room.players.findIndex((p) => p.id === s.me!.playerId)
    },
    currentIndex(s): number {
      if (!s.room) return -1
      return s.room.players.findIndex((p) => p.id === s.room!.current_player_id)
    },
    playersAway(): number {
      const n = this.room?.players.length ?? 0
      if (n === 0 || this.myIndex < 0 || this.currentIndex < 0) return 0
      return (this.myIndex - this.currentIndex + n) % n
    },
    amINext(): boolean {
      return this.playersAway === 1
    },
    upNext(s): PublicPlayer[] {
      const players = s.room?.players ?? []
      const n = players.length
      if (n === 0 || this.currentIndex < 0) return []
      const out: PublicPlayer[] = []
      for (let i = 1; i < n; i++) out.push(players[(this.currentIndex + i) % n])
      return out
    },
    phase(s): 'landing' | 'lobby' | 'active' {
      if (!s.room) return 'landing'
      return s.room.state === 'active' ? 'active' : 'lobby'
    },
  },

  actions: {
    _setSocket(sock: SocketLike) {
      this.socket = sock
      sock.onStatusChange((st) => { this.connStatus = st })
    },

    ensureSocket() {
      if (!this.socket) this._setSocket(new RoomSocket())
    },

    connect(code: string) {
      this.codeInView = code.toUpperCase()
      this.roomGone = false
      this.ensureSocket()
      this.socket!.connect(this.codeInView, (m) => this._handle(m))
    },

    /** After ws open, the view calls this to (re)join. */
    join(name?: string) {
      const token = loadToken(this.codeInView)
      this.joiningWithToken = !!token
      const msg: ClientMessage = token
        ? { type: 'join', player_token: token }
        : { type: 'join', player_name: name }
      this.socket!.send(msg)
    },

    _handle(m: ServerMessage) {
      switch (m.type) {
        case 'welcome':
          this.me = { playerId: m.player_id, token: m.token }
          saveToken(this.codeInView, m.token)
          this.joiningWithToken = false
          break
        case 'room_state':
          this.room = m.room
          if (!this.codeInView) this.codeInView = m.room.code
          break
        case 'nudged':
          this.nudgeReceivedAt = Date.now()
          break
        case 'error':
          this.lastError = { code: m.code, message: m.message }
          if (m.code === 'nudge_cooldown') this._startNudgeCooldown()
          if (m.code === 'not_found' && this.joiningWithToken) {
            clearToken(this.codeInView)
            this.joiningWithToken = false
            this.roomGone = true
          }
          break
      }
    },

    _startNudgeCooldown() {
      this.nudgeCooldownEndsAt = Date.now() + NUDGE_COOLDOWN_MS
      this.nudgeCooldownRemaining = NUDGE_COOLDOWN_MS / 1000
      if (this.nudgeTimer) clearInterval(this.nudgeTimer)
      this.nudgeTimer = setInterval(() => {
        const left = (this.nudgeCooldownEndsAt! - Date.now()) / 1000
        if (left <= 0) {
          clearInterval(this.nudgeTimer!)
          this.nudgeTimer = null
          this.nudgeCooldownRemaining = 0
          this.nudgeCooldownEndsAt = null
        } else {
          this.nudgeCooldownRemaining = left
        }
      }, 100)
    },

    dismissError() { this.lastError = null },

    // --- client actions ---
    startGame() { this.socket?.send({ type: 'start_game' }) },
    setOrder(ids: PlayerId[]) { this.socket?.send({ type: 'set_order', player_ids: ids }) },
    endTurn() { this.socket?.send({ type: 'end_turn' }) },
    claimTurn() { this.socket?.send({ type: 'claim_turn' }) },
    undoTurn() { this.socket?.send({ type: 'undo_turn' }) },
    skipPlayer(id: PlayerId) { this.socket?.send({ type: 'skip_player', player_id: id }) },
    removePlayer(id: PlayerId) { this.socket?.send({ type: 'remove_player', player_id: id }) },
    nudge() {
      if (this.nudgeCooldownRemaining > 0) return
      this.socket?.send({ type: 'nudge' })
      this._startNudgeCooldown()
    },
  },
})
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `npx vitest run src/stores/__tests__/room.spec.ts`
Expected: PASS (all). If `playersAway` test fails, re-check the modulo direction against the comment (me-to-current, wrapping forward).

- [ ] **Step 5: Commit**

```bash
git add frontend/src/stores
git commit -m "feat: add Pinia room store with derived getters and message handling"
```

---

## Task 8: i18n setup + English copy

**Files:**
- Create: `frontend/src/i18n/index.ts`, `frontend/src/i18n/locales/en.json`
- Test: `frontend/src/i18n/__tests__/i18n.spec.ts`

- [ ] **Step 1: Write the failing test**

`frontend/src/i18n/__tests__/i18n.spec.ts`:

```ts
import { describe, it, expect } from 'vitest'
import { i18n, errorKey } from '@/i18n'

describe('i18n', () => {
  it('resolves a known key', () => {
    expect(i18n.global.t('landing.createTitle')).toContain('game room')
  })
  it('maps server error codes to message keys', () => {
    expect(i18n.global.t(errorKey('not_your_turn'))).toMatch(/not your turn/i)
    expect(i18n.global.t(errorKey('nudge_cooldown'))).toMatch(/cooldown|seconds/i)
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

Run: `npx vitest run src/i18n/__tests__/i18n.spec.ts`
Expected: FAIL — cannot find module `@/i18n`.

- [ ] **Step 3: Create `frontend/src/i18n/locales/en.json`**

```json
{
  "appName": "Turn Tracker",
  "landing": {
    "createTitle": "Start a game room",
    "createSubtitle": "You'll get a code to share. Everyone joins on their own phone — no app, no sign-up.",
    "yourName": "Your name",
    "createCta": "Create room",
    "joinPrompt": "Joining instead? Enter a code",
    "joinTitle": "Join a room",
    "joinSubtitle": "Type the 6-character code from the host.",
    "joinCta": "Join"
  },
  "lobby": {
    "roomCode": "Room code",
    "copyLink": "Copy link",
    "showQr": "Show QR",
    "copied": "Copied to clipboard",
    "players": "Players",
    "dragToReorder": "Drag to reorder",
    "host": "Host",
    "connecting": "connecting…",
    "startGame": "Start game",
    "waitingForHost": "Waiting for {host} to start…",
    "waitingHint": "Hang tight — the game begins the moment the host taps Start. Keep this screen open.",
    "you": "(you)"
  },
  "active": {
    "yourTurnEyebrow": "Your turn",
    "yourTurnTitle": "It's your turn",
    "yourTurnHint": "Make your move, then tap Done.",
    "done": "DONE",
    "tapAnywhere": "tap anywhere here",
    "currentTurn": "Current turn",
    "turnOf": "{name}'s turn",
    "away": "{n} away",
    "upNext": "Up next",
    "nudge": "Nudge {name}",
    "nudgeWait": "Wait {n}s",
    "nudgeHint": "A gentle buzz on their phone · 10s cooldown",
    "youreUpNext": "You're up next",
    "finishingTurn": "{name} is finishing their turn",
    "claimHint": "Ready to go now? Pull your turn forward instead of waiting for the tap.",
    "claim": "Claim my turn",
    "wait": "Wait my turn",
    "screenAwake": "Screen stays awake",
    "soundBuzz": "Sound + buzz"
  },
  "host": {
    "skipTurn": "Skip this turn",
    "undoTurn": "Undo last turn",
    "reorder": "Reorder players",
    "remove": "Remove from room",
    "current": "Current"
  },
  "nudge": { "nudgedYou": "{name} nudged you!", "waiting": "Everyone's waiting on your move." },
  "status": { "connected": "connected", "disconnected": "disconnected", "skipped": "SKIPPED" },
  "errors": {
    "not_authorized": "Only the host can do that",
    "not_found": "Room not found",
    "wrong_state": "That action isn't valid right now",
    "not_your_turn": "It's not your turn yet",
    "nudge_cooldown": "Nudge on cooldown — try again in a few seconds",
    "generic": "Something went wrong",
    "reconnecting": "Reconnecting…",
    "reconnectingHint": "Hang on, we'll resync your turn."
  }
}
```

- [ ] **Step 4: Create `frontend/src/i18n/index.ts`**

```ts
import { createI18n } from 'vue-i18n'
import en from './locales/en.json'

export const i18n = createI18n({
  legacy: false,
  locale: 'en',
  fallbackLocale: 'en',
  messages: { en },
})

/** Map a server error code to an i18n key, falling back to a generic one. */
export function errorKey(code: string): string {
  const known = [
    'not_authorized', 'not_found', 'wrong_state', 'not_your_turn', 'nudge_cooldown',
  ]
  return known.includes(code) ? `errors.${code}` : 'errors.generic'
}
```

- [ ] **Step 5: Run test to verify it passes**

Run: `npx vitest run src/i18n/__tests__/i18n.spec.ts`
Expected: PASS.

- [ ] **Step 6: Wire i18n + pinia + router into `main.ts`**

Replace `frontend/src/main.ts` with:

```ts
import './styles/tokens.css'
import './styles/base.css'
import { createApp } from 'vue'
import { createPinia } from 'pinia'
import App from './App.vue'
import { router } from './router'
import { i18n } from './i18n'

createApp(App).use(createPinia()).use(router).use(i18n).mount('#app')
```

(The `./router` import resolves in Task 9.)

- [ ] **Step 7: Commit**

```bash
git add frontend/src/i18n frontend/src/main.ts
git commit -m "feat: add vue-i18n setup and English copy"
```

---

## Task 9: Router

**Files:**
- Create: `frontend/src/router/index.ts`
- Test: `frontend/src/router/__tests__/router.spec.ts`

- [ ] **Step 1: Write the failing test**

`frontend/src/router/__tests__/router.spec.ts`:

```ts
import { describe, it, expect } from 'vitest'
import { router } from '@/router'

describe('router', () => {
  it('has landing and room routes', () => {
    const names = router.getRoutes().map((r) => r.path)
    expect(names).toContain('/')
    expect(names).toContain('/room/:code')
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

Run: `npx vitest run src/router/__tests__/router.spec.ts`
Expected: FAIL — cannot find module `@/router`.

- [ ] **Step 3: Create `frontend/src/router/index.ts`**

```ts
import { createRouter, createWebHistory } from 'vue-router'
import LandingView from '@/views/LandingView.vue'
import RoomView from '@/views/RoomView.vue'

export const router = createRouter({
  history: createWebHistory(),
  routes: [
    { path: '/', name: 'landing', component: LandingView },
    { path: '/room/:code', name: 'room', component: RoomView, props: true },
  ],
})
```

- [ ] **Step 4: Run test to verify it passes** (create temporary stub views first if needed)

If the views don't exist yet, create minimal stubs so the import resolves:

`frontend/src/views/LandingView.vue` and `frontend/src/views/RoomView.vue`:

```vue
<template><div /></template>
```

Run: `npx vitest run src/router/__tests__/router.spec.ts`
Expected: PASS. (Stubs are replaced in Tasks 11–12.)

- [ ] **Step 5: Commit**

```bash
git add frontend/src/router frontend/src/views/LandingView.vue frontend/src/views/RoomView.vue
git commit -m "feat: add router with landing and room routes"
```

---

## Task 10: UI leaf components (AppButton, CodeInput, Avatar, Toast)

**Files:**
- Create: `frontend/src/components/ui/AppButton.vue`, `CodeInput.vue`, `Avatar.vue`, `Toast.vue`
- Test: `frontend/src/components/ui/__tests__/CodeInput.spec.ts`

These are presentational; `CodeInput` has logic worth testing (6-char uppercase, emits when complete).

- [ ] **Step 1: Write the failing test for CodeInput**

`frontend/src/components/ui/__tests__/CodeInput.spec.ts`:

```ts
import { describe, it, expect } from 'vitest'
import { mount } from '@vue/test-utils'
import CodeInput from '@/components/ui/CodeInput.vue'

describe('CodeInput', () => {
  it('uppercases input and emits complete on 6 chars', async () => {
    const wrapper = mount(CodeInput)
    const input = wrapper.get('input')
    await input.setValue('gr7k9p')
    expect((wrapper.emitted('update:modelValue')?.at(-1)?.[0] as string)).toBe('GR7K9P')
    expect(wrapper.emitted('complete')).toBeTruthy()
  })

  it('does not emit complete below 6 chars', async () => {
    const wrapper = mount(CodeInput)
    await wrapper.get('input').setValue('gr7')
    expect(wrapper.emitted('complete')).toBeFalsy()
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

Run: `npx vitest run src/components/ui/__tests__/CodeInput.spec.ts`
Expected: FAIL — cannot find module.

- [ ] **Step 3: Create the components**

`frontend/src/components/ui/CodeInput.vue`:

```vue
<script setup lang="ts">
import { ref, watch } from 'vue'

const props = defineProps<{ modelValue?: string }>()
const emit = defineEmits<{
  (e: 'update:modelValue', v: string): void
  (e: 'complete', v: string): void
}>()

const value = ref(props.modelValue ?? '')

function onInput(e: Event) {
  const raw = (e.target as HTMLInputElement).value.toUpperCase().replace(/[^A-Z0-9]/g, '').slice(0, 6)
  value.value = raw
  emit('update:modelValue', raw)
  if (raw.length === 6) emit('complete', raw)
}

watch(() => props.modelValue, (v) => { if (v !== undefined) value.value = v })
</script>

<template>
  <input
    class="code-input"
    inputmode="text"
    autocapitalize="characters"
    autocomplete="off"
    maxlength="6"
    :value="value"
    @input="onInput"
  />
</template>

<style scoped>
.code-input {
  width: 100%;
  font-family: var(--tt-font-mono);
  letter-spacing: 0.16em;
  text-transform: uppercase;
  background: var(--tt-surface-2);
  border: 1.5px solid var(--tt-border);
  border-radius: var(--tt-r-md);
  color: var(--tt-text);
  padding: var(--tt-4);
  font-size: 24px;
  text-align: center;
}
.code-input:focus { outline: none; border-color: var(--tt-accent); }
</style>
```

`frontend/src/components/ui/AppButton.vue`:

```vue
<script setup lang="ts">
defineProps<{ variant?: 'accent' | 'ghost'; disabled?: boolean }>()
</script>

<template>
  <button class="btn" :class="variant ?? 'accent'" :disabled="disabled">
    <slot />
  </button>
</template>

<style scoped>
.btn {
  width: 100%; min-height: 56px;
  border: none; border-radius: var(--tt-r-md);
  font-family: var(--tt-font-sans); font-weight: 700; font-size: 18px;
  cursor: pointer;
}
.accent { background: var(--tt-accent); color: var(--tt-on-accent); box-shadow: 0 0 24px -4px var(--tt-accent-glow); }
.ghost { background: transparent; color: var(--tt-text-muted); border: 1.5px solid var(--tt-border); }
.btn:disabled { background: var(--tt-surface-1); color: var(--tt-text-faint); box-shadow: none; cursor: default; }
</style>
```

`frontend/src/components/ui/Avatar.vue`:

```vue
<script setup lang="ts">
const props = defineProps<{ name: string; accent?: boolean; size?: number }>()
const initial = () => props.name.trim().charAt(0).toUpperCase() || '?'
</script>

<template>
  <span class="avatar" :class="{ accent }" :style="{ width: (size ?? 34) + 'px', height: (size ?? 34) + 'px' }">
    {{ initial() }}
  </span>
</template>

<style scoped>
.avatar {
  display: inline-flex; align-items: center; justify-content: center;
  border-radius: 50%; background: var(--tt-surface-3); color: var(--tt-text-muted);
  font-weight: 800; font-size: 15px;
}
.avatar.accent { background: var(--tt-accent); color: var(--tt-on-accent); }
</style>
```

`frontend/src/components/ui/Toast.vue`:

```vue
<script setup lang="ts">
defineProps<{ tone?: 'info' | 'danger' | 'warning' | 'success' }>()
</script>

<template>
  <div class="toast" :class="tone ?? 'info'" role="status">
    <slot />
  </div>
</template>

<style scoped>
.toast {
  display: flex; align-items: center; gap: var(--tt-3);
  border-radius: 13px; padding: 14px 16px; font-size: 14px; font-weight: 700;
  background: var(--tt-surface-2); border: 1px solid var(--tt-border);
}
.info { color: var(--tt-info); border-color: rgba(56, 189, 248, 0.28); }
.danger { color: var(--tt-danger); border-color: rgba(251, 113, 133, 0.28); }
.warning { color: var(--tt-warning); border-color: rgba(251, 191, 36, 0.28); }
.success { color: var(--tt-accent); border-color: rgba(52, 211, 153, 0.28); }
</style>
```

- [ ] **Step 4: Run test to verify it passes**

Run: `npx vitest run src/components/ui/__tests__/CodeInput.spec.ts`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/components/ui
git commit -m "feat: add UI leaf components (button, code input, avatar, toast)"
```

---

## Task 11: PlayerRow component

**Files:**
- Create: `frontend/src/components/player/PlayerRow.vue`
- Test: `frontend/src/components/player/__tests__/PlayerRow.spec.ts`

Status reads three ways: dot color **+ text label + icon** (never color alone). Design Section 04, lines 676–704.

- [ ] **Step 1: Write the failing test**

`frontend/src/components/player/__tests__/PlayerRow.spec.ts`:

```ts
import { describe, it, expect } from 'vitest'
import { mount } from '@vue/test-utils'
import PlayerRow from '@/components/player/PlayerRow.vue'
import { i18n } from '@/i18n'

const player = { id: 'p1', name: 'Sam', is_host: true, connected: true }

describe('PlayerRow', () => {
  it('shows host badge and a connected label', () => {
    const w = mount(PlayerRow, { props: { player }, global: { plugins: [i18n] } })
    expect(w.text()).toContain('Sam')
    expect(w.text()).toContain('Host')
  })

  it('shows a disconnected label when not connected', () => {
    const w = mount(PlayerRow, {
      props: { player: { ...player, is_host: false, connected: false } },
      global: { plugins: [i18n] },
    })
    expect(w.text().toLowerCase()).toContain('disconnected')
  })

  it('shows skipped state with label', () => {
    const w = mount(PlayerRow, {
      props: { player: { ...player, is_host: false }, skipped: true },
      global: { plugins: [i18n] },
    })
    expect(w.text().toUpperCase()).toContain('SKIPPED')
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

Run: `npx vitest run src/components/player/__tests__/PlayerRow.spec.ts`
Expected: FAIL — cannot find module.

- [ ] **Step 3: Create `frontend/src/components/player/PlayerRow.vue`**

```vue
<script setup lang="ts">
import type { PublicPlayer } from '@/types/wire'
import Avatar from '@/components/ui/Avatar.vue'
import { useI18n } from 'vue-i18n'

const props = defineProps<{
  player: PublicPlayer
  skipped?: boolean
  isYou?: boolean
  draggable?: boolean
}>()
const { t } = useI18n()
</script>

<template>
  <div class="row" :class="{ dim: !player.connected }">
    <span v-if="draggable" class="handle" aria-hidden="true"><i /><i /><i /></span>
    <Avatar :name="player.name" :accent="player.is_host" />
    <span class="name">
      {{ player.name }}
      <span v-if="isYou" class="you">{{ t('lobby.you') }}</span>
    </span>

    <span v-if="player.is_host" class="badge host">{{ t('lobby.host') }}</span>
    <span v-else-if="skipped" class="badge skip">» {{ t('status.skipped') }}</span>
    <span v-else class="status-label">
      {{ player.connected ? t('status.connected') : t('status.disconnected') }}
    </span>

    <span class="dot" :class="player.connected ? 'on' : 'off'" aria-hidden="true" />
  </div>
</template>

<style scoped>
.row {
  display: flex; align-items: center; gap: var(--tt-3);
  min-height: 56px; padding: 0 14px;
  border-radius: var(--tt-r-md); background: var(--tt-surface-2);
  border: 1px solid var(--tt-border);
}
.row.dim { opacity: 0.65; background: var(--tt-surface-1); }
.handle { display: flex; flex-direction: column; gap: 3px; }
.handle i { width: 16px; height: 2px; background: var(--tt-text-faint); border-radius: 2px; }
.name { flex: 1; font-size: 15px; font-weight: 600; color: var(--tt-text); }
.you { color: var(--tt-accent); font-weight: 700; font-size: 13px; }
.badge { padding: 3px 9px; border-radius: var(--tt-r-full); font-family: var(--tt-font-mono); font-size: 10px; font-weight: 700; }
.badge.host { background: rgba(52, 211, 153, 0.14); color: var(--tt-accent); }
.badge.skip { background: rgba(251, 191, 36, 0.14); color: var(--tt-warning); }
.status-label { font-size: 11px; color: var(--tt-text-faint); }
.dot { width: 8px; height: 8px; border-radius: 50%; }
.dot.on { background: var(--tt-accent); }
.dot.off { background: var(--tt-text-faint); }
</style>
```

- [ ] **Step 4: Run test to verify it passes**

Run: `npx vitest run src/components/player/__tests__/PlayerRow.spec.ts`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/components/player
git commit -m "feat: add PlayerRow with host/connected/disconnected/skipped states"
```

---

## Task 12: LandingView (Create + Join)

**Files:**
- Replace stub: `frontend/src/views/LandingView.vue`
- Test: `frontend/src/views/__tests__/LandingView.spec.ts`

Screens 01 + 02. Create calls `POST /api/rooms`, persists the host token, then routes to `/room/:code`. Join routes to `/room/:code` with the name carried in store.

- [ ] **Step 1: Write the failing test**

`frontend/src/views/__tests__/LandingView.spec.ts`:

```ts
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { createRouter, createMemoryHistory } from 'vue-router'
import LandingView from '@/views/LandingView.vue'
import { i18n } from '@/i18n'

const routes = [
  { path: '/', component: LandingView },
  { path: '/room/:code', component: { template: '<div>room</div>' } },
]

function mountView() {
  const router = createRouter({ history: createMemoryHistory(), routes })
  return { wrapper: mount(LandingView, { global: { plugins: [createPinia(), router, i18n] } }), router }
}

describe('LandingView', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    localStorage.clear()
    vi.restoreAllMocks()
  })

  it('creates a room and navigates to it', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue({
      ok: true,
      json: async () => ({ room_code: 'GR7K9P', player_id: 'p1', token: 'tok' }),
    }))
    const { wrapper, router } = mountView()
    await router.isReady()
    await wrapper.get('[data-test=name]').setValue('Sam')
    await wrapper.get('[data-test=create]').trigger('click')
    await flushPromises()
    expect(localStorage.getItem('tt:token:GR7K9P')).toBe('tok')
    expect(router.currentRoute.value.fullPath).toContain('/room/GR7K9P')
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

Run: `npx vitest run src/views/__tests__/LandingView.spec.ts`
Expected: FAIL — the stub view has no `[data-test=name]`.

- [ ] **Step 3: Create `frontend/src/views/LandingView.vue`**

```vue
<script setup lang="ts">
import { ref } from 'vue'
import { useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { saveToken } from '@/services/tokenStore'
import AppButton from '@/components/ui/AppButton.vue'
import CodeInput from '@/components/ui/CodeInput.vue'

const { t } = useI18n()
const router = useRouter()
const mode = ref<'create' | 'join'>('create')
const name = ref('')
const code = ref('')
const error = ref('')

async function create() {
  error.value = ''
  const res = await fetch('/api/rooms', {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ host_name: name.value.trim() }),
  })
  if (!res.ok) { error.value = t('errors.generic'); return }
  const data = (await res.json()) as { room_code: string; player_id: string; token: string }
  saveToken(data.room_code, data.token)
  router.push({ path: `/room/${data.room_code}` })
}

function join() {
  if (code.value.length !== 6 || !name.value.trim()) return
  router.push({ path: `/room/${code.value}`, query: { name: name.value.trim() } })
}
</script>

<template>
  <main class="wrap">
    <div class="brand"><span class="dot" />{{ t('appName') }}</div>

    <template v-if="mode === 'create'">
      <h1>{{ t('landing.createTitle') }}</h1>
      <p>{{ t('landing.createSubtitle') }}</p>
      <label>{{ t('landing.yourName') }}</label>
      <input data-test="name" v-model="name" class="field" />
      <AppButton data-test="create" :disabled="!name.trim()" @click="create">
        {{ t('landing.createCta') }}
      </AppButton>
      <button class="link" @click="mode = 'join'">{{ t('landing.joinPrompt') }}</button>
    </template>

    <template v-else>
      <h1>{{ t('landing.joinTitle') }}</h1>
      <p>{{ t('landing.joinSubtitle') }}</p>
      <CodeInput v-model="code" />
      <label>{{ t('landing.yourName') }}</label>
      <input data-test="name" v-model="name" class="field" />
      <AppButton :disabled="code.length !== 6 || !name.trim()" @click="join">
        {{ t('landing.joinCta') }}
      </AppButton>
    </template>

    <p v-if="error" class="err">{{ error }}</p>
  </main>
</template>

<style scoped>
.wrap { max-width: 420px; margin: 0 auto; padding: 56px 26px; display: flex; flex-direction: column; gap: var(--tt-3); min-height: 100%; }
.brand { display: inline-flex; align-items: center; gap: 10px; font-family: var(--tt-font-mono); font-weight: 700; letter-spacing: .12em; color: var(--tt-text); }
.brand .dot { width: 13px; height: 13px; border-radius: 50%; background: var(--tt-accent); box-shadow: 0 0 12px var(--tt-accent); }
h1 { font-size: 34px; font-weight: 800; letter-spacing: -.03em; margin: 24px 0 0; }
p { color: var(--tt-text-muted); margin: 0; line-height: 1.55; }
label { font-family: var(--tt-font-mono); font-size: 12px; letter-spacing: .08em; color: var(--tt-text-faint); margin-top: var(--tt-4); }
.field { height: 56px; border-radius: var(--tt-r-md); background: var(--tt-surface-2); border: 1.5px solid var(--tt-border); color: var(--tt-text); padding: 0 18px; font-size: 18px; font-weight: 600; }
.field:focus { outline: none; border-color: var(--tt-accent); }
.link { background: none; border: none; color: var(--tt-text-muted); margin-top: var(--tt-4); cursor: pointer; }
.err { color: var(--tt-danger); }
</style>
```

- [ ] **Step 4: Run test to verify it passes**

Run: `npx vitest run src/views/__tests__/LandingView.spec.ts`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/views/LandingView.vue frontend/src/views/__tests__/LandingView.spec.ts
git commit -m "feat: add LandingView with create and join flows"
```

---

## Task 13: ShareCodeBlock, LobbySubview

**Files:**
- Create: `frontend/src/components/lobby/ShareCodeBlock.vue`, `frontend/src/components/lobby/LobbySubview.vue`
- Test: `frontend/src/components/lobby/__tests__/LobbySubview.spec.ts`

Screens 03 (host) + 04 (waiting). Host sees the share block, reorderable list, Start; non-host sees the waiting spinner + roster.

- [ ] **Step 1: Write the failing test**

`frontend/src/components/lobby/__tests__/LobbySubview.spec.ts`:

```ts
import { describe, it, expect, beforeEach } from 'vitest'
import { mount } from '@vue/test-utils'
import { setActivePinia, createPinia } from 'pinia'
import LobbySubview from '@/components/lobby/LobbySubview.vue'
import { useRoomStore } from '@/stores/room'
import { i18n } from '@/i18n'

function withHost(isHost: boolean) {
  const s = useRoomStore()
  s.me = { playerId: isHost ? 'p1' : 'p2', token: 't' }
  s._handle({
    type: 'room_state',
    room: {
      code: 'GR7K9P', state: 'lobby', current_player_id: null,
      players: [
        { id: 'p1', name: 'Sam', is_host: true, connected: true },
        { id: 'p2', name: 'Bob', is_host: false, connected: true },
      ],
    },
  })
  return s
}

describe('LobbySubview', () => {
  beforeEach(() => setActivePinia(createPinia()))

  it('host sees Start button', () => {
    withHost(true)
    const w = mount(LobbySubview, { global: { plugins: [i18n] } })
    expect(w.text()).toContain('Start game')
  })

  it('non-host sees waiting message', () => {
    withHost(false)
    const w = mount(LobbySubview, { global: { plugins: [i18n] } })
    expect(w.text()).toContain('Waiting for Sam')
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

Run: `npx vitest run src/components/lobby/__tests__/LobbySubview.spec.ts`
Expected: FAIL — cannot find module.

- [ ] **Step 3: Create `frontend/src/components/lobby/ShareCodeBlock.vue`**

```vue
<script setup lang="ts">
import { ref, onMounted, watch } from 'vue'
import QRCode from 'qrcode'
import { useI18n } from 'vue-i18n'

const props = defineProps<{ code: string }>()
const { t } = useI18n()
const copied = ref(false)
const showQr = ref(false)
const canvas = ref<HTMLCanvasElement | null>(null)

function joinUrl() {
  return `${window.location.origin}/room/${props.code}`
}

async function copy() {
  try { await navigator.clipboard.writeText(joinUrl()) } catch { /* no-op */ }
  copied.value = true
  setTimeout(() => (copied.value = false), 2000)
}

async function renderQr() {
  if (showQr.value && canvas.value) await QRCode.toCanvas(canvas.value, joinUrl(), { width: 180 })
}
watch(showQr, renderQr)
onMounted(renderQr)
</script>

<template>
  <div class="share">
    <div class="eyebrow">{{ t('lobby.roomCode') }}</div>
    <div class="code">{{ code }}</div>
    <div class="actions">
      <button @click="copy">{{ copied ? t('lobby.copied') : t('lobby.copyLink') }}</button>
      <button @click="showQr = !showQr">{{ t('lobby.showQr') }}</button>
    </div>
    <canvas v-show="showQr" ref="canvas" class="qr" />
  </div>
</template>

<style scoped>
.share { border-radius: var(--tt-r-xl); background: var(--tt-surface-1); border: 1px solid var(--tt-surface-2); padding: 20px; text-align: center; }
.eyebrow { font-family: var(--tt-font-mono); font-size: 11px; letter-spacing: .14em; color: var(--tt-text-faint); }
.code { margin-top: 12px; font-family: var(--tt-font-mono); font-weight: 700; font-size: 42px; letter-spacing: .16em; color: var(--tt-accent); text-shadow: 0 0 18px rgba(52, 211, 153, 0.35); }
.actions { margin-top: 16px; display: flex; gap: 9px; }
.actions button { flex: 1; height: 42px; border-radius: 11px; background: var(--tt-surface-2); color: var(--tt-text); border: none; font-weight: 600; cursor: pointer; }
.qr { margin: 16px auto 0; border-radius: 8px; }
</style>
```

- [ ] **Step 4: Create `frontend/src/components/lobby/LobbySubview.vue`**

```vue
<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRoomStore } from '@/stores/room'
import ShareCodeBlock from './ShareCodeBlock.vue'
import PlayerRow from '@/components/player/PlayerRow.vue'
import AppButton from '@/components/ui/AppButton.vue'

const { t } = useI18n()
const store = useRoomStore()
const hostName = computed(() => store.room?.players.find((p) => p.is_host)?.name ?? '')
</script>

<template>
  <section class="lobby" v-if="store.room">
    <template v-if="store.isHost">
      <ShareCodeBlock :code="store.room.code" />
      <div class="hdr">
        <span>{{ t('lobby.players') }} · {{ store.room.players.length }}</span>
        <span class="muted">{{ t('lobby.dragToReorder') }}</span>
      </div>
      <div class="list">
        <PlayerRow
          v-for="p in store.room.players"
          :key="p.id"
          :player="p"
          :is-you="p.id === store.me?.playerId"
          draggable
        />
      </div>
      <AppButton class="start" @click="store.startGame()">{{ t('lobby.startGame') }}</AppButton>
    </template>

    <template v-else>
      <div class="waiting">
        <span class="spinner" aria-hidden="true" />
        <h2>{{ t('lobby.waitingForHost', { host: hostName }) }}</h2>
        <p>{{ t('lobby.waitingHint') }}</p>
      </div>
      <div class="list">
        <PlayerRow
          v-for="p in store.room.players"
          :key="p.id"
          :player="p"
          :is-you="p.id === store.me?.playerId"
        />
      </div>
    </template>
  </section>
</template>

<style scoped>
.lobby { display: flex; flex-direction: column; gap: var(--tt-4); padding: 18px 22px; }
.hdr { display: flex; justify-content: space-between; font-size: 13px; font-weight: 700; color: var(--tt-text); }
.muted { font-family: var(--tt-font-mono); font-size: 11px; color: var(--tt-text-faint); }
.list { display: flex; flex-direction: column; gap: 8px; }
.start { margin-top: var(--tt-4); }
.waiting { display: flex; flex-direction: column; align-items: center; text-align: center; gap: var(--tt-3); padding: var(--tt-7) 0; }
.waiting h2 { font-size: 23px; font-weight: 700; margin: 0; }
.waiting p { color: var(--tt-text-muted); max-width: 240px; margin: 0; }
.spinner { width: 40px; height: 40px; border-radius: 50%; border: 2px solid var(--tt-surface-2); border-top-color: var(--tt-accent); animation: ttSpin 1s linear infinite; }
@media (prefers-reduced-motion: reduce) { .spinner { animation: none; } }
</style>
```

- [ ] **Step 5: Run test to verify it passes**

Run: `npx vitest run src/components/lobby/__tests__/LobbySubview.spec.ts`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add frontend/src/components/lobby
git commit -m "feat: add lobby subview, share-code block, reorderable roster"
```

---

## Task 14: Active-turn components (TurnButton, NudgeButton, TurnEmblem)

**Files:**
- Create: `frontend/src/components/active/TurnButton.vue`, `NudgeButton.vue`, `TurnEmblem.vue`
- Test: `frontend/src/components/active/__tests__/NudgeButton.spec.ts`

- [ ] **Step 1: Write the failing test for NudgeButton cooldown**

`frontend/src/components/active/__tests__/NudgeButton.spec.ts`:

```ts
import { describe, it, expect, beforeEach, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import { setActivePinia, createPinia } from 'pinia'
import NudgeButton from '@/components/active/NudgeButton.vue'
import { useRoomStore } from '@/stores/room'
import { i18n } from '@/i18n'

describe('NudgeButton', () => {
  beforeEach(() => { setActivePinia(createPinia()); vi.restoreAllMocks() })

  it('calls store.nudge on click and shows label', () => {
    const store = useRoomStore()
    const spy = vi.spyOn(store, 'nudge')
    const w = mount(NudgeButton, { props: { name: 'Alice' }, global: { plugins: [i18n] } })
    expect(w.text()).toContain('Nudge Alice')
    w.get('button').trigger('click')
    expect(spy).toHaveBeenCalled()
  })

  it('shows the wait countdown when cooling down', async () => {
    const store = useRoomStore()
    store.nudgeCooldownRemaining = 6
    const w = mount(NudgeButton, { props: { name: 'Alice' }, global: { plugins: [i18n] } })
    expect(w.text()).toContain('Wait 6s')
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

Run: `npx vitest run src/components/active/__tests__/NudgeButton.spec.ts`
Expected: FAIL — cannot find module.

- [ ] **Step 3: Create the three components**

`frontend/src/components/active/TurnButton.vue`:

```vue
<script setup lang="ts">
import { useI18n } from 'vue-i18n'
const props = defineProps<{ disabled?: boolean }>()
const emit = defineEmits<{ (e: 'done'): void }>()
const { t } = useI18n()
</script>

<template>
  <button class="turn" :class="{ disabled }" :disabled="disabled" @click="emit('done')">
    <span class="sheen" v-if="!disabled" aria-hidden="true" />
    <span class="label">{{ t('active.done') }}</span>
  </button>
</template>

<style scoped>
.turn {
  width: 100%; min-height: 130px; border: none; cursor: pointer;
  border-radius: var(--tt-r-xl); background: var(--tt-on-accent);
  color: var(--tt-accent); font-size: 40px; font-weight: 800; letter-spacing: .04em;
  position: relative; overflow: hidden; box-shadow: inset 0 -5px 0 rgba(0, 0, 0, .45);
}
.turn.disabled { background: var(--tt-surface-1); color: var(--tt-text-faint); box-shadow: none; cursor: default; }
.turn:active:not(.disabled) { transform: scale(.985); }
.sheen { position: absolute; top: 0; bottom: 0; width: 60px; background: linear-gradient(90deg, transparent, rgba(52,211,153,.22), transparent); animation: ttSheen 3.2s ease-in-out infinite; }
.label { position: relative; }
@media (prefers-reduced-motion: reduce) { .sheen { animation: none; } }
</style>
```

`frontend/src/components/active/NudgeButton.vue`:

```vue
<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRoomStore } from '@/stores/room'

const props = defineProps<{ name: string }>()
const { t } = useI18n()
const store = useRoomStore()
const cooling = computed(() => store.nudgeCooldownRemaining > 0)
const pct = computed(() => (store.nudgeCooldownRemaining / 10) * 100)
const label = computed(() =>
  cooling.value
    ? t('active.nudgeWait', { n: Math.ceil(store.nudgeCooldownRemaining) })
    : t('active.nudge', { name: props.name }),
)
</script>

<template>
  <button class="nudge" :class="{ cooling }" @click="store.nudge()">
    <span v-if="cooling" class="bar" :style="{ width: pct + '%' }" aria-hidden="true" />
    <span class="emoji">👋</span>
    <span class="label">{{ label }}</span>
  </button>
</template>

<style scoped>
.nudge {
  position: relative; width: 100%; min-height: 62px; cursor: pointer;
  border-radius: var(--tt-r-md); background: var(--tt-surface-2); border: 1.5px solid var(--tt-border);
  display: flex; align-items: center; justify-content: center; gap: 11px; overflow: hidden;
  color: var(--tt-text); font-size: 17px; font-weight: 700;
}
.nudge.cooling .label { color: var(--tt-warning); }
.bar { position: absolute; left: 0; top: 0; bottom: 0; background: rgba(251, 191, 36, .16); transition: width .1s linear; }
.emoji, .label { position: relative; }
</style>
```

`frontend/src/components/active/TurnEmblem.vue`:

```vue
<script setup lang="ts">
defineProps<{ fast?: boolean }>()
</script>

<template>
  <div class="emblem">
    <span class="pulse" :class="{ fast }" aria-hidden="true" />
    <span class="pulse delay" :class="{ fast }" aria-hidden="true" />
    <span class="core">!</span>
  </div>
</template>

<style scoped>
.emblem { position: relative; width: 150px; height: 150px; display: flex; align-items: center; justify-content: center; }
.pulse { position: absolute; width: 150px; height: 150px; border-radius: 50%; background: rgba(4, 20, 13, .16); animation: ttPulse 2.4s ease-out infinite; }
.pulse.delay { animation-delay: 1.2s; }
.pulse.fast { animation-duration: 1.6s; }
.core { position: relative; width: 108px; height: 108px; border-radius: 50%; background: var(--tt-on-accent); color: var(--tt-accent); display: flex; align-items: center; justify-content: center; font-size: 46px; font-weight: 800; }
@media (prefers-reduced-motion: reduce) { .pulse { animation: none; opacity: 0; } }
</style>
```

- [ ] **Step 4: Run test to verify it passes**

Run: `npx vitest run src/components/active/__tests__/NudgeButton.spec.ts`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/components/active
git commit -m "feat: add turn button, nudge button, and turn emblem"
```

---

## Task 15: ActiveSubview (Your turn / Not your turn / Claim)

**Files:**
- Create: `frontend/src/components/active/ActiveSubview.vue`
- Test: `frontend/src/components/active/__tests__/ActiveSubview.spec.ts`

Screens 05/06/07, branched on `isMyTurn` / `amINext`.

- [ ] **Step 1: Write the failing test**

`frontend/src/components/active/__tests__/ActiveSubview.spec.ts`:

```ts
import { describe, it, expect, beforeEach } from 'vitest'
import { mount } from '@vue/test-utils'
import { setActivePinia, createPinia } from 'pinia'
import ActiveSubview from '@/components/active/ActiveSubview.vue'
import { useRoomStore } from '@/stores/room'
import { i18n } from '@/i18n'
import type { PublicRoom } from '@/types/wire'

function room(current: string): PublicRoom {
  return {
    code: 'GR7K9P', state: 'active', current_player_id: current,
    players: [
      { id: 'p1', name: 'Sam', is_host: true, connected: true },
      { id: 'p2', name: 'Alice', is_host: false, connected: true },
      { id: 'p3', name: 'Bob', is_host: false, connected: true },
    ],
  }
}

function setup(meId: string, current: string) {
  setActivePinia(createPinia())
  const s = useRoomStore()
  s.me = { playerId: meId, token: 't' }
  s._handle({ type: 'room_state', room: room(current) })
  return mount(ActiveSubview, { global: { plugins: [i18n] } })
}

describe('ActiveSubview', () => {
  it('shows YOUR TURN hero when it is my turn', () => {
    const w = setup('p2', 'p2')
    expect(w.text().toLowerCase()).toContain("it's your turn")
  })
  it('shows claim screen when I am next', () => {
    const w = setup('p3', 'p2') // current p2 idx1, next idx2 = p3
    expect(w.text()).toContain('Claim my turn')
  })
  it('shows current-player view otherwise', () => {
    const w = setup('p1', 'p2') // p1 is 2 away
    expect(w.text()).toContain("Alice's turn")
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

Run: `npx vitest run src/components/active/__tests__/ActiveSubview.spec.ts`
Expected: FAIL — cannot find module.

- [ ] **Step 3: Create `frontend/src/components/active/ActiveSubview.vue`**

```vue
<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRoomStore } from '@/stores/room'
import TurnButton from './TurnButton.vue'
import TurnEmblem from './TurnEmblem.vue'
import NudgeButton from './NudgeButton.vue'
import AppButton from '@/components/ui/AppButton.vue'
import Avatar from '@/components/ui/Avatar.vue'

const { t } = useI18n()
const store = useRoomStore()
const currentName = computed(() => store.currentPlayer?.name ?? '')
</script>

<template>
  <section class="active" v-if="store.room">
    <!-- 05 YOUR TURN -->
    <div v-if="store.isMyTurn" class="hero">
      <div class="eyebrow">{{ t('active.yourTurnEyebrow') }}</div>
      <TurnEmblem />
      <h1 class="hero-title" aria-live="assertive">{{ t('active.yourTurnTitle') }}</h1>
      <p class="hero-hint">{{ t('active.yourTurnHint') }}</p>
      <div class="spacer" />
      <TurnButton @done="store.endTurn()" />
    </div>

    <!-- 07 CLAIM -->
    <div v-else-if="store.amINext" class="claim">
      <div class="pill">{{ t('active.youreUpNext') }}</div>
      <h1>{{ t('active.finishingTurn', { name: currentName }) }}</h1>
      <p>{{ t('active.claimHint') }}</p>
      <div class="spacer" />
      <AppButton @click="store.claimTurn()">{{ t('active.claim') }}</AppButton>
      <AppButton variant="ghost">{{ t('active.wait') }}</AppButton>
    </div>

    <!-- 06 NOT YOUR TURN -->
    <div v-else class="watch">
      <div class="eyebrow">{{ t('active.currentTurn') }}</div>
      <Avatar :name="currentName" :size="96" />
      <h1>{{ t('active.turnOf', { name: currentName }) }}</h1>
      <div class="away">{{ t('active.away', { n: store.playersAway }) }}</div>
      <div class="spacer" />
      <NudgeButton :name="currentName" />
      <p class="nudge-hint">{{ t('active.nudgeHint') }}</p>
    </div>
  </section>
</template>

<style scoped>
.active { min-height: 100%; }
.hero { min-height: 100%; display: flex; flex-direction: column; align-items: center; text-align: center; padding: 30px 24px; background: var(--tt-accent); color: var(--tt-on-accent); }
.eyebrow { font-family: var(--tt-font-mono); font-size: 11px; letter-spacing: .18em; }
.hero-title { font-size: 46px; font-weight: 800; letter-spacing: -.035em; margin: 38px 0 0; }
.hero-hint { font-weight: 600; opacity: .72; }
.spacer { flex: 1; }
.claim, .watch { min-height: 100%; display: flex; flex-direction: column; align-items: center; text-align: center; gap: var(--tt-3); padding: 30px 24px; }
.pill { display: inline-flex; padding: 8px 15px; border-radius: var(--tt-r-full); background: rgba(52,211,153,.12); border: 1px solid rgba(52,211,153,.3); color: var(--tt-accent); font-family: var(--tt-font-mono); font-size: 12px; font-weight: 700; }
.away { display: inline-flex; gap: 9px; padding: 9px 16px; border-radius: var(--tt-r-full); background: var(--tt-surface-1); border: 1px solid var(--tt-surface-2); color: var(--tt-accent); font-family: var(--tt-font-mono); font-weight: 700; }
.nudge-hint { font-size: 12px; color: var(--tt-text-faint); }
.claim :deep(.btn) { margin-top: var(--tt-2); }
</style>
```

- [ ] **Step 4: Run test to verify it passes**

Run: `npx vitest run src/components/active/__tests__/ActiveSubview.spec.ts`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/components/active/ActiveSubview.vue frontend/src/components/active/__tests__/ActiveSubview.spec.ts
git commit -m "feat: add active subview (your turn / claim / watching)"
```

---

## Task 16: Overlays — ToastHost, NudgeToast, HostSheet

**Files:**
- Create: `frontend/src/components/overlays/ToastHost.vue`, `NudgeToast.vue`, `HostSheet.vue`
- Test: `frontend/src/components/overlays/__tests__/ToastHost.spec.ts`

- [ ] **Step 1: Write the failing test**

`frontend/src/components/overlays/__tests__/ToastHost.spec.ts`:

```ts
import { describe, it, expect, beforeEach } from 'vitest'
import { mount } from '@vue/test-utils'
import { setActivePinia, createPinia } from 'pinia'
import ToastHost from '@/components/overlays/ToastHost.vue'
import { useRoomStore } from '@/stores/room'
import { i18n } from '@/i18n'

describe('ToastHost', () => {
  beforeEach(() => setActivePinia(createPinia()))

  it('renders a localized message for the current error code', async () => {
    const store = useRoomStore()
    const w = mount(ToastHost, { global: { plugins: [i18n] } })
    store.lastError = { code: 'not_your_turn', message: 'server text' }
    await w.vm.$nextTick()
    expect(w.text()).toContain("not your turn")
  })

  it('shows reconnecting banner when status is reconnecting', async () => {
    const store = useRoomStore()
    const w = mount(ToastHost, { global: { plugins: [i18n] } })
    store.connStatus = 'reconnecting'
    await w.vm.$nextTick()
    expect(w.text()).toContain('Reconnecting')
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

Run: `npx vitest run src/components/overlays/__tests__/ToastHost.spec.ts`
Expected: FAIL — cannot find module.

- [ ] **Step 3: Create the overlays**

`frontend/src/components/overlays/ToastHost.vue`:

```vue
<script setup lang="ts">
import { computed, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRoomStore } from '@/stores/room'
import { errorKey } from '@/i18n'
import Toast from '@/components/ui/Toast.vue'

const { t } = useI18n()
const store = useRoomStore()
const errorText = computed(() => (store.lastError ? t(errorKey(store.lastError.code)) : ''))

watch(() => store.lastError, (e) => {
  if (e) setTimeout(() => store.dismissError(), 4000)
})
</script>

<template>
  <div class="host">
    <Toast v-if="store.connStatus === 'reconnecting'" tone="info">
      {{ t('errors.reconnecting') }}
    </Toast>
    <Toast v-if="store.lastError" :tone="store.lastError.code === 'nudge_cooldown' ? 'warning' : 'danger'">
      {{ errorText }}
    </Toast>
  </div>
</template>

<style scoped>
.host { position: fixed; top: 12px; left: 12px; right: 12px; z-index: 50; display: flex; flex-direction: column; gap: 8px; pointer-events: none; }
.host :deep(.toast) { pointer-events: auto; }
</style>
```

`frontend/src/components/overlays/NudgeToast.vue`:

```vue
<script setup lang="ts">
import { ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRoomStore } from '@/stores/room'

const { t } = useI18n()
const store = useRoomStore()
const visible = ref(false)
const fromName = ref('')

watch(() => store.nudgeReceivedAt, (at) => {
  if (!at) return
  fromName.value = store.currentPlayer?.name ?? ''
  visible.value = true
  setTimeout(() => (visible.value = false), 3000)
})
</script>

<template>
  <div v-if="visible" class="nudge-toast" role="alert" aria-live="assertive">
    <span class="emoji">👋</span>
    <div>
      <div class="title">{{ t('nudge.nudgedYou', { name: fromName }) }}</div>
      <div class="sub">{{ t('nudge.waiting') }}</div>
    </div>
  </div>
</template>

<style scoped>
.nudge-toast { position: fixed; top: 58px; left: 18px; right: 18px; z-index: 60; display: flex; align-items: center; gap: 14px; border-radius: var(--tt-r-lg); background: var(--tt-on-accent); padding: 16px 18px; box-shadow: var(--tt-shadow-lg); animation: ttShake .6s ease-in-out; }
.title { font-size: 16px; font-weight: 800; color: var(--tt-text); }
.sub { font-size: 13px; color: var(--tt-accent); }
@media (prefers-reduced-motion: reduce) { .nudge-toast { animation: none; } }
</style>
```

`frontend/src/components/overlays/HostSheet.vue`:

```vue
<script setup lang="ts">
import { useI18n } from 'vue-i18n'
import { useRoomStore } from '@/stores/room'

defineProps<{ open: boolean }>()
const emit = defineEmits<{ (e: 'close'): void }>()
const { t } = useI18n()
const store = useRoomStore()

function skip() { const id = store.currentPlayer?.id; if (id) store.skipPlayer(id); emit('close') }
function undo() { store.undoTurn(); emit('close') }
function remove() { const id = store.currentPlayer?.id; if (id) store.removePlayer(id); emit('close') }
</script>

<template>
  <div v-if="open" class="backdrop" @click.self="emit('close')">
    <div class="sheet" role="dialog" aria-modal="true">
      <div class="grab" />
      <button class="item" @click="skip">{{ t('host.skipTurn') }}</button>
      <button class="item" @click="undo">{{ t('host.undoTurn') }}</button>
      <button class="item danger" @click="remove">{{ t('host.remove') }}</button>
    </div>
  </div>
</template>

<style scoped>
.backdrop { position: fixed; inset: 0; background: rgba(0,0,0,.55); z-index: 70; display: flex; align-items: flex-end; }
.sheet { width: 100%; border-radius: 26px 26px 0 0; background: var(--tt-surface-1); border-top: 1px solid var(--tt-border); padding: 14px 18px 30px; box-shadow: 0 -20px 50px -10px rgba(0,0,0,.7); }
.grab { width: 40px; height: 5px; border-radius: 3px; background: var(--tt-border); margin: 0 auto 18px; }
.item { width: 100%; min-height: 54px; border-radius: var(--tt-r-md); background: var(--tt-surface-2); border: none; color: var(--tt-text); font-size: 15px; font-weight: 600; margin-bottom: 8px; cursor: pointer; }
.item.danger { background: rgba(251,113,133,.08); color: var(--tt-danger); border: 1px solid rgba(251,113,133,.2); }
</style>
```

- [ ] **Step 4: Run test to verify it passes**

Run: `npx vitest run src/components/overlays/__tests__/ToastHost.spec.ts`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/components/overlays
git commit -m "feat: add toast host, nudge toast, and host override sheet"
```

---

## Task 17: Device services (sound, haptics, wakeLock, reduced-motion)

**Files:**
- Create: `frontend/src/services/sound.ts`, `frontend/src/services/haptics.ts`, `frontend/src/services/wakeLock.ts`, `frontend/src/composables/useReducedMotion.ts`
- Test: `frontend/src/services/__tests__/haptics.spec.ts`

All graceful no-ops where unsupported.

- [ ] **Step 1: Write the failing test for haptics**

`frontend/src/services/__tests__/haptics.spec.ts`:

```ts
import { describe, it, expect, vi } from 'vitest'
import { vibrate } from '@/services/haptics'

describe('haptics', () => {
  it('calls navigator.vibrate when available', () => {
    const spy = vi.fn()
    vi.stubGlobal('navigator', { vibrate: spy })
    vibrate([0, 80, 40, 80])
    expect(spy).toHaveBeenCalledWith([0, 80, 40, 80])
  })

  it('is a no-op when vibrate is missing', () => {
    vi.stubGlobal('navigator', {})
    expect(() => vibrate([10])).not.toThrow()
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

Run: `npx vitest run src/services/__tests__/haptics.spec.ts`
Expected: FAIL — cannot find module.

- [ ] **Step 3: Create the services**

`frontend/src/services/haptics.ts`:

```ts
export function vibrate(pattern: number | number[]): void {
  const nav = navigator as Navigator & { vibrate?: (p: number | number[]) => boolean }
  if (typeof nav.vibrate === 'function') nav.vibrate(pattern)
}
```

`frontend/src/services/sound.ts`:

```ts
let ctx: AudioContext | null = null

/** Short two-tone chime via Web Audio — no asset file. No-op if unsupported. */
export function chime(): void {
  const AC = (window.AudioContext || (window as any).webkitAudioContext) as
    | typeof AudioContext
    | undefined
  if (!AC) return
  ctx ??= new AC()
  const now = ctx.currentTime
  for (const [i, freq] of [880, 1320].entries()) {
    const osc = ctx.createOscillator()
    const gain = ctx.createGain()
    osc.frequency.value = freq
    osc.connect(gain)
    gain.connect(ctx.destination)
    const start = now + i * 0.12
    gain.gain.setValueAtTime(0.0001, start)
    gain.gain.exponentialRampToValueAtTime(0.2, start + 0.01)
    gain.gain.exponentialRampToValueAtTime(0.0001, start + 0.18)
    osc.start(start)
    osc.stop(start + 0.2)
  }
}
```

`frontend/src/services/wakeLock.ts`:

```ts
type SentinelLike = { release: () => Promise<void> }
let sentinel: SentinelLike | null = null

export async function acquire(): Promise<void> {
  const wl = (navigator as Navigator & { wakeLock?: { request: (t: 'screen') => Promise<SentinelLike> } }).wakeLock
  if (!wl) return
  try { sentinel = await wl.request('screen') } catch { /* denied / unsupported */ }
}

export async function release(): Promise<void> {
  try { await sentinel?.release() } catch { /* ignore */ }
  sentinel = null
}
```

`frontend/src/composables/useReducedMotion.ts`:

```ts
import { ref, onMounted, onUnmounted } from 'vue'

export function useReducedMotion() {
  const reduced = ref(false)
  let mq: MediaQueryList | null = null
  const update = () => { reduced.value = !!mq?.matches }
  onMounted(() => {
    mq = window.matchMedia('(prefers-reduced-motion: reduce)')
    update()
    mq.addEventListener('change', update)
  })
  onUnmounted(() => mq?.removeEventListener('change', update))
  return { reduced }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `npx vitest run src/services/__tests__/haptics.spec.ts`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/services/sound.ts frontend/src/services/haptics.ts frontend/src/services/wakeLock.ts frontend/src/composables/useReducedMotion.ts frontend/src/services/__tests__/haptics.spec.ts
git commit -m "feat: add sound, haptics, wake lock, reduced-motion services"
```

---

## Task 18: RoomView — wire it all together with device effects

**Files:**
- Replace stub: `frontend/src/views/RoomView.vue`
- Modify: `frontend/src/App.vue`
- Test: `frontend/src/views/__tests__/RoomView.spec.ts`

RoomView connects the socket on mount, joins (token or name from query), switches Lobby/Active subviews by `store.phase`, mounts overlays, fires device effects on turn transition, and handles room-gone by routing home.

- [ ] **Step 1: Write the failing test**

`frontend/src/views/__tests__/RoomView.spec.ts`:

```ts
import { describe, it, expect, beforeEach, vi } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import { setActivePinia, createPinia } from 'pinia'
import { createRouter, createMemoryHistory } from 'vue-router'
import RoomView from '@/views/RoomView.vue'
import { useRoomStore } from '@/stores/room'
import { i18n } from '@/i18n'

const routes = [
  { path: '/', component: { template: '<div>landing</div>' } },
  { path: '/room/:code', component: RoomView, props: true },
]

const fakeSocket = () => ({
  connect: vi.fn(),
  send: vi.fn(),
  close: vi.fn(),
  onStatusChange: vi.fn(),
})

async function mountAt(code: string) {
  setActivePinia(createPinia())
  // Inject a fake socket BEFORE mount so onMounted's connect() never constructs
  // a real WebSocket (jsdom has none).
  const store = useRoomStore()
  store._setSocket(fakeSocket() as any)
  const router = createRouter({ history: createMemoryHistory(), routes })
  router.push(`/room/${code}`)
  await router.isReady()
  const wrapper = mount(RoomView, { props: { code }, global: { plugins: [router, i18n] } })
  return { wrapper, router, store }
}

describe('RoomView', () => {
  beforeEach(() => { localStorage.clear(); vi.restoreAllMocks() })

  it('connects on mount (sets code in view)', async () => {
    const { store } = await mountAt('GR7K9P')
    expect(store.codeInView).toBe('GR7K9P')
    expect(store.socket!.connect).toHaveBeenCalled()
  })

  it('routes home when the room is gone', async () => {
    const { store, router } = await mountAt('GR7K9P')
    store.roomGone = true
    await flushPromises()
    expect(router.currentRoute.value.path).toBe('/')
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

Run: `npx vitest run src/views/__tests__/RoomView.spec.ts`
Expected: FAIL — the stub RoomView doesn't set `codeInView` or watch `roomGone`.

- [ ] **Step 3: Create `frontend/src/views/RoomView.vue`**

```vue
<script setup lang="ts">
import { onMounted, onUnmounted, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { useRoomStore } from '@/stores/room'
import LobbySubview from '@/components/lobby/LobbySubview.vue'
import ActiveSubview from '@/components/active/ActiveSubview.vue'
import ToastHost from '@/components/overlays/ToastHost.vue'
import NudgeToast from '@/components/overlays/NudgeToast.vue'
import { vibrate } from '@/services/haptics'
import { chime } from '@/services/sound'
import { acquire, release } from '@/services/wakeLock'

const props = defineProps<{ code: string }>()
const route = useRoute()
const router = useRouter()
const store = useRoomStore()

onMounted(() => {
  store.connect(props.code)
  const name = typeof route.query.name === 'string' ? route.query.name : undefined
  // join is sent once the socket is open; the store sends on connect-open via its callback.
  // For simplicity the store joins immediately after connect resolves the first frame;
  // here we trigger join on first open by sending after a microtask.
  queueMicrotask(() => store.join(name))
})

onUnmounted(() => { void release() })

// Device effects on becoming / leaving your turn.
watch(
  () => store.isMyTurn,
  (mine, was) => {
    if (mine && !was) {
      vibrate([0, 80, 40, 80])
      chime()
      void acquire()
    } else if (!mine && was) {
      void release()
    }
  },
)

// Nudge received → double buzz.
watch(() => store.nudgeReceivedAt, (at) => { if (at) vibrate([0, 60, 40, 60]) })

// Room gone (stale token / server restart) → back to landing.
watch(() => store.roomGone, (gone) => { if (gone) router.replace('/') })
</script>

<template>
  <main class="room">
    <ToastHost />
    <NudgeToast />
    <LobbySubview v-if="store.phase === 'lobby'" />
    <ActiveSubview v-else-if="store.phase === 'active'" />
    <div v-else class="connecting">Connecting…</div>
  </main>
</template>

<style scoped>
.room { min-height: 100%; background: var(--tt-surface-0); }
.connecting { display: flex; align-items: center; justify-content: center; min-height: 100vh; color: var(--tt-text-muted); }
</style>
```

> Note on join timing: the store's `connect` opens the socket and registers the message callback; `join` only `send`s, which the `RoomSocket` will buffer-then-send once open in production. If a worker finds join races the open in real testing, move the `store.join(name)` call into a status watcher (`connStatus === 'open'`) — but keep the public API unchanged.

- [ ] **Step 4: Update `frontend/src/App.vue`** (full-height shell)

```vue
<script setup lang="ts"></script>

<template>
  <RouterView />
</template>

<style>
#app { min-height: 100%; }
</style>
```

- [ ] **Step 5: Run test to verify it passes**

Run: `npx vitest run src/views/__tests__/RoomView.spec.ts`
Expected: PASS.

- [ ] **Step 6: Full unit suite + build green**

Run: `npm run test` then `npm run build`
Expected: all unit tests pass; production build succeeds.

- [ ] **Step 7: Commit**

```bash
git add frontend/src/views/RoomView.vue frontend/src/App.vue frontend/src/views/__tests__/RoomView.spec.ts
git commit -m "feat: wire RoomView with subviews, overlays, and device effects"
```

---

## Task 19: Playwright E2E — multi-player happy path + errors + reconnect

**Files:**
- Create: `frontend/playwright.config.ts`, `frontend/tests/e2e/flow.spec.ts`, `frontend/tests/e2e/helpers.ts`

`webServer` boots the Rust backend + `vite preview`. Device APIs are stubbed via `addInitScript` and spied. The backend must serve the built SPA, so the E2E build copies `dist` into a dir Actix serves.

- [ ] **Step 1: Create `frontend/playwright.config.ts`**

```ts
import { defineConfig } from '@playwright/test'

export default defineConfig({
  testDir: './tests/e2e',
  timeout: 30_000,
  use: { baseURL: 'http://127.0.0.1:8080', trace: 'on-first-retry' },
  webServer: [
    {
      // Build the SPA, then run the Rust backend serving it.
      command:
        'npm run build && cross-env STATIC_DIR=./dist BIND_ADDR=127.0.0.1:8080 cargo run --manifest-path ../Cargo.toml --release',
      url: 'http://127.0.0.1:8080',
      reuseExistingServer: !process.env.CI,
      timeout: 180_000,
      cwd: '.',
    },
  ],
})
```

> Add `cross-env` to devDependencies (`npm i -D cross-env`) so `STATIC_DIR=...` works cross-platform (Windows dev + Linux CI). On Windows, `cargo run --manifest-path ../Cargo.toml` builds the backend from the repo root.

- [ ] **Step 2: Create `frontend/tests/e2e/helpers.ts`**

```ts
import type { Page, BrowserContext, Browser } from '@playwright/test'

/** Stub device APIs so headless Chromium doesn't throw and we can spy. */
export async function stubDeviceApis(context: BrowserContext) {
  await context.addInitScript(() => {
    ;(window as any).__calls = { vibrate: 0, wakeLock: 0 }
    ;(navigator as any).vibrate = () => { ;(window as any).__calls.vibrate++; return true }
    ;(navigator as any).wakeLock = { request: async () => { ;(window as any).__calls.wakeLock++; return { release: async () => {} } } }
  })
}

export async function newPlayer(browser: Browser) {
  const context = await browser.newContext()
  await stubDeviceApis(context)
  const page = await context.newPage()
  return { context, page }
}

export async function createRoom(page: Page, name: string): Promise<string> {
  await page.goto('/')
  await page.getByTestId('name').fill(name)
  await page.getByTestId('create').click()
  await page.waitForURL(/\/room\/[A-Z0-9]{6}/)
  const m = page.url().match(/\/room\/([A-Z0-9]{6})/)
  return m![1]
}

export async function joinRoom(page: Page, code: string, name: string) {
  await page.goto(`/room/${code}?name=${encodeURIComponent(name)}`)
}
```

- [ ] **Step 3: Create `frontend/tests/e2e/flow.spec.ts`**

```ts
import { test, expect } from '@playwright/test'
import { newPlayer, createRoom, joinRoom } from './helpers'

test('multi-player turn flow propagates across clients', async ({ browser }) => {
  const host = await newPlayer(browser)
  const bob = await newPlayer(browser)
  const cara = await newPlayer(browser)

  const code = await createRoom(host.page, 'Sam')
  await joinRoom(bob.page, code, 'Bob')
  await joinRoom(cara.page, code, 'Cara')

  // Host lobby shows 3 players.
  await expect(host.page.getByText('Players · 3')).toBeVisible()

  // Start the game.
  await host.page.getByText('Start game').click()

  // Host is first (created first) → host sees YOUR TURN.
  await expect(host.page.getByText("It's your turn")).toBeVisible()
  // Bob sees Sam's turn (watching) or claim if next.
  await expect(bob.page.getByText(/turn/i)).toBeVisible()

  // Host taps Done → turn advances to the next player.
  await host.page.getByRole('button', { name: 'DONE' }).click()
  await expect(host.page.getByText("It's your turn")).toBeHidden()

  // Vibrate was called on the host when their turn began.
  const vib = await host.page.evaluate(() => (window as any).__calls.vibrate)
  expect(vib).toBeGreaterThan(0)
})

test('non-current player sees an error toast when acting out of turn', async ({ browser }) => {
  const host = await newPlayer(browser)
  const bob = await newPlayer(browser)
  const code = await createRoom(host.page, 'Sam')
  await joinRoom(bob.page, code, 'Bob')
  await host.page.getByText('Start game').click()

  // If Bob is not current and not next, he is watching; trigger a nudge cooldown instead.
  // (Adjust selector to the watching view's Nudge button.)
  await expect(bob.page.getByText(/turn/i)).toBeVisible()
})

test('reconnect restores the player via stored token', async ({ browser }) => {
  const host = await newPlayer(browser)
  const code = await createRoom(host.page, 'Sam')
  await expect(host.page.getByText('Players · 1')).toBeVisible()

  // Reload (new page load reuses localStorage token in the same context).
  await host.page.reload()
  await expect(host.page).toHaveURL(new RegExp(`/room/${code}`))
  await expect(host.page.getByText('Players · 1')).toBeVisible()
})
```

> The second test is a scaffold for the out-of-turn/cooldown assertion — the worker fills the exact selector once the watching view renders in the running app. Keep it as a real assertion, not a skip.

- [ ] **Step 4: Install browsers and run E2E**

Run (from `frontend/`): `npx playwright install --with-deps chromium` then `npm run e2e`
Expected: the 3 tests pass (backend builds + boots, SPA served, multi-context flow works). If `Players · N` text differs from the rendered markup, align the assertion with `LobbySubview`'s actual output.

- [ ] **Step 5: Commit**

```bash
git add frontend/playwright.config.ts frontend/tests/e2e frontend/package.json frontend/package-lock.json
git commit -m "test: add Playwright multi-player e2e suite"
```

---

## Task 20: Docker + CI

**Files:**
- Modify: `Dockerfile`
- Modify: `.github/workflows/*.yml` (the test workflow)

- [ ] **Step 1: Inspect current Dockerfile and CI**

Run: `cat Dockerfile` and `ls .github/workflows`
Read both fully before editing. The frontend build stage must run before the Rust build copies `static`, and the final image must contain `frontend/dist` at the path `STATIC_DIR` points to.

- [ ] **Step 2: Add a frontend build stage to `Dockerfile`**

Add before the final stage (adjust to match the existing multi-stage layout):

```dockerfile
# --- frontend build ---
FROM node:22-alpine AS frontend
WORKDIR /app/frontend
COPY frontend/package.json frontend/package-lock.json ./
RUN npm ci
COPY frontend/ ./
RUN npm run build
```

In the final runtime stage, copy the built SPA next to the binary and point `STATIC_DIR` at it:

```dockerfile
COPY --from=frontend /app/frontend/dist /app/static
ENV STATIC_DIR=/app/static
```

- [ ] **Step 3: Add a frontend CI job**

Add a job to the test workflow (mirror the existing job's style):

```yaml
  frontend:
    runs-on: ubuntu-latest
    defaults:
      run:
        working-directory: frontend
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with:
          node-version: 22
          cache: npm
          cache-dependency-path: frontend/package-lock.json
      - run: npm ci
      - run: npm run build
      - run: npm run test
```

(E2E in CI is optional — it needs the Rust toolchain + Playwright browsers in the same job. If included, add a step that installs Rust + `npx playwright install --with-deps chromium` then `npm run e2e`. Otherwise document E2E as a local/pre-release gate.)

- [ ] **Step 4: Verify locally**

Run: `docker build -t turn-tracker .`
Expected: image builds; both frontend and backend stages succeed.

- [ ] **Step 5: Commit**

```bash
git add Dockerfile .github/workflows
git commit -m "ci: build frontend in Docker image and add frontend CI job"
```

---

## Task 21: Final verification gate

- [ ] **Step 1: Backend**

Run: `cargo test && cargo clippy --all-targets -- -D warnings`
Expected: green.

- [ ] **Step 2: Frontend unit + build**

Run (from `frontend/`): `npm run test && npm run build`
Expected: green.

- [ ] **Step 3: E2E**

Run (from `frontend/`): `npm run e2e`
Expected: green.

- [ ] **Step 4: Manual smoke (optional but recommended)**

Start `cargo run` + `npm run dev`, open two browser profiles, create + join a room, start, take turns, nudge, reconnect by refreshing. Confirm device cues fire on a real phone if available.

- [ ] **Step 5: Final commit / branch wrap-up**

```bash
git add -A
git commit -m "chore: frontend v1 complete"
```

---

## Notes for the executor

- **Stub-then-replace ordering:** Task 9 creates stub views so the router import resolves; Tasks 12 and 18 replace them. Don't commit the stubs as final.
- **Join timing** (Task 18): the store sends `join` after `connect`. `RoomSocket.send` requires an open socket; in production the first `room_state`/`welcome` only arrives after open, so a microtask is usually enough. If E2E shows a race, gate `join` on `connStatus === 'open'` via a watcher — public API unchanged.
- **WebSocket scheme:** never type the plaintext literal in source (semgrep hook). Always derive from `location.protocol`.
- **Commits via Bash tool**, not PowerShell (BOM).
- **Version floors:** the `package.json` versions are floors; confirm current majors at install time.
