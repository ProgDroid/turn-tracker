# Deployment

Two complete, working deployment paths. **Fly is the live one**; the VPS variant
is kept ready so it can be swapped in quickly if Fly disappoints on price,
reliability or anything else.

| | Fly (live) | VPS (standby) |
|---|---|---|
| Config | `fly.toml` (repo root) | `deploy/vps/` |
| TLS | Fly-managed | Caddy, HTTP-01 |
| Host maintenance | none | yours: OS patching, Docker, ufw, SSH |
| Client IP source | `Fly-Client-IP` | `X-Forwarded-For`, overwritten by Caddy |
| Deploy | `flyctl deploy` | build → GHCR → SSH → `docker compose up -d` |
| Cost (Sept 2026) | ~$2.20/mo | provider-dependent |

Both run the **same `Dockerfile`** and the same binary. Only the surrounding
infrastructure differs — swapping is a deployment change, not a code change.

## The one setting that MUST match the platform

`TT_CLIENT_IP_HEADER` names a header the app may trust for per-IP rate limiting.

- **On Fly: `Fly-Client-IP`.** Fly sets it from the real TCP connection and
  strips any client-supplied value. `X-Forwarded-For` is *not* trustworthy on
  Fly — a client can spoof it and pick its own rate-limit bucket.
- **On the VPS: leave it UNSET.** Caddy is configured to overwrite
  `X-Forwarded-For` (`header_up X-Forwarded-For {remote_host}`), so the default
  behaviour is correct there.

Getting this backwards is a silent security hole, in both directions:

- Unset on Fly → the limiter keys on a spoofable header, so rotating it gives
  unlimited `POST /api/rooms` attempts and removes the throttling that makes
  guessing `TT_CREATE_TOKEN` impractical.
- Set to `Fly-Client-IP` on the VPS → a client sends that header, Caddy forwards
  it untouched, and the app trusts it. Same hole, opposite cause.

There is no safe default that covers both, which is why it is explicit
configuration rather than auto-detection.

## Switching from Fly to the VPS

1. Provision a host and run `deploy/vps/bootstrap.sh` (Ubuntu 24.04).
2. Copy `deploy/vps/docker-compose.yml`, `Caddyfile` and `.env.example` to
   `/opt/turn-tracker/`; create `.env` from the example and `chmod 600` it.
3. **Do not set `TT_CLIENT_IP_HEADER`** in that `.env`.
4. Point DNS at the host, **grey cloud / DNS-only** so Caddy can complete
   HTTP-01 and no extra proxy sits in the path.
5. Restore the GHCR-and-SSH deploy job in `.github/workflows/ci.yml`.

Room state does not transfer. Rooms expire after 24h idle anyway, so switch
between sessions rather than mid-game.
