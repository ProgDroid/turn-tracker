---
name: deploying-from-a-phone
description: "Termux/Android deployment gotchas — Go binaries cannot resolve DNS, proot-distro is the fix, and what Fly's web UI cannot do"
metadata:
  node_type: memory
  type: feedback
  originSessionId: 9ef2eb60-98fe-5a01-9457-2dd80719116a
---

He deploys from **Termux on Android** (2026-09-16, whole Turn Tracker launch).
Four things cost real time; expect all of them again on the next project.

**1. Upstream Go binaries cannot resolve DNS in Termux — install proot-distro
first, not as a fallback.** Go's pure-Go resolver reads `/etc/resolv.conf`,
which Android does not have and which you cannot create without root. So
`flyctl` (and any other upstream Go tool) reports "cannot access the internet"
while `curl` works perfectly — `curl` is a Termux-native build using Android's
resolver. That contrast is the diagnostic.

The fix is one command and ends the entire category:
`pkg install proot-distro && proot-distro install debian && proot-distro login debian --bind $HOME/<repo>:/root/<repo>`.
Debian has a real `/etc/resolv.conf`. **Lead with this** rather than
drip-feeding API workarounds — it was offered twice and declined before being
adopted, and everything after it worked first time.

**2. `flyctl auth login` fails with connection refused** — it opens a loopback
listener for the OAuth redirect that the Android browser cannot reach. There is
**no `flyctl auth token` subcommand** (I claimed there was; there isn't). The
supported path is the `FLY_API_TOKEN` environment variable, which is what CI
uses anyway.

**3. Fly's web dashboard cannot create volumes or certificates.** It is
deliberately thin; the CLI is the real interface. Do not plan a browser-only
path around it — secrets and app creation work, volumes and certs do not.

**4. Inside proot, `flyctl deploy` MUST use `--remote-only`.** A local Docker
build is impossible there.

**Cloudflare, unrelated to Termux but hit in the same session:** importing a
domain copies whatever DNS it finds — including the registrar's parking A/AAAA
records — and imports them **proxied**. Multiple A records on one name
round-robin rather than failing over, so the site intermittently resolves to
parking and you get **HTTP 525**. A Cloudflare error code is itself proof that
Cloudflare is in the path, so the record is still orange. Delete every address
record that is not the host's, keep MX/TXT.

**How to apply:** when the next deploy starts from a phone, install
proot-distro before anything else, authenticate via `FLY_API_TOKEN`, and after
adding DNS verify with `curl "https://dns.google/resolve?name=<host>&type=A"`
that exactly ONE answer comes back and it is the host's.
