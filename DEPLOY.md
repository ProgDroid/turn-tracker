# Deploying Turn Tracker

The server is a single static binary that serves the SPA over plain HTTP on
`BIND_ADDR`. Put a TLS-terminating reverse proxy (Caddy/nginx) in front of it.

## Environment variables

| Variable | Default | Purpose |
|---|---|---|
| `BIND_ADDR` | `127.0.0.1:8080` | Address the HTTP server binds to. In the container image this is `0.0.0.0:8080`. |
| `STATIC_DIR` | `./static` | Directory holding the built SPA (`index.html` + `assets/`). |
| `ALLOWED_ORIGINS` | _(unset)_ | Comma-separated WebSocket `Origin` allowlist. **Must be set in production** (see below). |
| `TT_SNAPSHOT_PATH` | `./data/rooms.json` | Room-state snapshot file. Point at a mounted volume so it survives container recreation. |
| `TT_SNAPSHOT_INTERVAL_SECS` | `60` | Periodic snapshot interval. A snapshot is written only when state changed. |

## Operational requirements

These are not optional in a public deployment — several safety features depend
on them.

### 1. Set `ALLOWED_ORIGINS`

If `ALLOWED_ORIGINS` is unset/empty, WebSocket Origin checking is **disabled**
(all origins allowed) — a dev convenience. In production set it to your site's
origin(s), e.g. `ALLOWED_ORIGINS=https://turns.example.com`. The server logs a
warning at startup when it is unset.

### 2. Reverse proxy must set `X-Forwarded-For`, and the container must not be exposed directly

Room creation (`POST /api/rooms`) is rate-limited **per client IP**. Because the
app sits behind a proxy, it reads the client IP from `X-Forwarded-For` /
`Forwarded`. For this to be both correct and unspoofable:

- The proxy **must** set/overwrite `X-Forwarded-For` (Caddy's `reverse_proxy`
  and nginx's `proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;` do
  this).
- The app container **must** be reachable **only** through the proxy — do not
  publish its port directly to the internet. Otherwise a client could forge
  `X-Forwarded-For` and dodge the rate limit.

### 3. Run a single instance

The server takes an exclusive OS lock on `<data-dir>/.lock` at startup. A second
process pointed at the same data directory will **refuse to start** (logged
error, exit 1) rather than clobber the snapshot. For a zero-downtime deploy,
stop the old container before starting the new one (or give the new one its own
data dir). The lock is released automatically when the process exits, including
on a crash.

### 4. Snapshot volume

- Point `TT_SNAPSHOT_PATH` at a mounted volume (e.g. `/data/rooms.json`).
- The process user must be able to write that directory; the snapshot is created
  with `0600` permissions.
- The data directory is git-ignored and must never be served by `actix-files`.

## Health check

`GET /health` returns `200 {"status":"ok"}` for liveness probes.
