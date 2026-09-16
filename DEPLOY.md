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
| `TT_CLIENT_IP_HEADER` | _(unset)_ | Header the app may trust for the per-IP rate limit. Set to `Fly-Client-IP` on Fly. Leave UNSET behind a reverse proxy that overwrites `X-Forwarded-For`. |
| `TT_CREATE_TOKEN` | _(unset)_ | Shared secret required in the `X-Create-Token` header on `POST /api/rooms`. Unset = room creation is open to anyone. |

## Operational requirements

These are not optional in a public deployment — several safety features depend
on them.

### 1. Set `ALLOWED_ORIGINS`

If `ALLOWED_ORIGINS` is unset/empty, WebSocket Origin checking is **disabled**
(all origins allowed) — a dev convenience. In production set it to your site's
origin(s), e.g. `ALLOWED_ORIGINS=https://turns.example.com`. The server logs a
warning at startup when it is unset.

### 2. The client-IP source must match the platform

**On Fly, set `TT_CLIENT_IP_HEADER=Fly-Client-IP` and skip the rest of this
section.** Fly sets that header from the real TCP connection and strips any
client-supplied value, so it is authoritative. `X-Forwarded-For` is *not*
trustworthy on Fly — a client can spoof it. There is no reverse proxy of yours
to configure.

The remainder of this section applies to the **reverse-proxy deploy**
(`deploy/vps/`), where `TT_CLIENT_IP_HEADER` must stay UNSET. Setting it there
would be a hole in the other direction: a client could send `Fly-Client-IP`,
the proxy would forward it untouched, and the app would trust it.

#### Reverse proxy must OVERWRITE `X-Forwarded-For`, and the container must not be exposed directly

Room creation (`POST /api/rooms`) is rate-limited **per client IP**. Because the
app sits behind a proxy, it reads the client IP from `X-Forwarded-For` /
`Forwarded`, taking the **leftmost** entry. For this to be both correct and
unspoofable, two things must hold.

**The proxy must overwrite the header, not append to it.** This is the easy one
to get wrong, because the common defaults append:

- Caddy's bare `reverse_proxy` **appends** the client address to whatever
  `X-Forwarded-For` the client sent.
- nginx's `proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;`
  **also appends** — that is exactly what `$proxy_add_x_forwarded_for` means.

Either way a request carrying `X-Forwarded-For: 203.0.113.9` arrives upstream
as `203.0.113.9, <real client IP>`, and the limiter buckets on the
attacker-chosen leftmost value. Rotating it gives unlimited `POST /api/rooms`
attempts, which also removes the rate limiting that `TT_CREATE_TOKEN` (§5)
relies on to make guessing the shared secret impractical.

Configure the proxy to replace the header outright. In Caddy:

```
reverse_proxy app:8080 {
	header_up X-Forwarded-For {remote_host}
}
```

In nginx, use `$remote_addr` rather than `$proxy_add_x_forwarded_for`:

```
proxy_set_header X-Forwarded-For $remote_addr;
```

**The app container must be reachable only through the proxy.** Do not publish
its port directly to the internet; a direct client could otherwise forge
`X-Forwarded-For` and dodge the rate limit whatever the proxy does.

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

### 6. WebSocket heartbeat

The server pings each WebSocket every 30s. Liveness is only re-checked on those
same 30s ticks, and the check is a strict "more than 90s since the last pong",
so a dead connection is actually closed somewhere in the **90-120s** range —
typically close to 120s, because in steady state the last pong lands just after
a tick.

What this asks of a reverse proxy is the opposite of a long idle timeout: the
server's own pings mean the connection is never idle for more than 30s, so the
proxy's idle timeout only has to comfortably exceed **30s**, not 90s. Any sane
margin over the ping interval (60s or more) is fine. Caddy imposes no WebSocket
idle timeout by default. The heartbeat is also why the app is served with
Cloudflare's proxy disabled (grey cloud) — see the deploy spec.

### 7. Allow time to shut down cleanly

On SIGTERM the server stops accepting connections, drains what is in flight for
up to 5 seconds, and only then writes the final snapshot. Give the container
more grace than that or it is killed mid-drain and the last snapshot interval
of room state is lost:

```yaml
services:
  app:
    stop_grace_period: 30s
```

Docker Compose's default is **10s**, which is not enough margin. Look for
`final snapshot written on shutdown` in the logs to confirm a clean stop.

## Health check

`GET /health` returns `200 {"status":"ok"}` for liveness probes.
