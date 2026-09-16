---
name: user-profile
description: "Who the user is — solo developer, preferred stack and infra for personal side projects"
metadata: 
  node_type: memory
  type: user
  originSessionId: 86aa50b1-333c-404b-bee8-9bb1bf11a075
---

Solo developer building personal side projects. Default stack: **Rust + Actix-web** for backends, **Vue 3 (Composition API) + Vite** for frontends, shipped as single Dockerised binaries via **GitHub Actions**. Other projects in the same stack: an anime calendar (likely Cloud Run) and a fighting-game assistant (not yet scoped).

**Hosting default CHANGED 2026-09-16 — do not assume Hetzner.** An earlier version of this file said he ships to a Hetzner CAX11 ARM VPS. That was wrong in two ways and it cost a full design cycle: he had **no box at all**, and by Sept 2026 Hetzner's cost-optimised ARM (CAX) line was **unavailable in every location** while the remaining CPX plans had roughly tripled (CPX12 ~€13.79/mo; CPX11 US-only and 20+ EUR). Turn Tracker now runs on **Fly.io** (~$2.20/mo, single Machine, `lhr`). Ask what a project actually runs on rather than inferring it from this file.

Prefers reusing this stack over adopting new tech, and explicitly wants stack/architecture proposals **challenged with justification** rather than rubber-stamped (asked "do you still think X works?" repeatedly during planning and rewarded pushback). Comfortable deferring features to keep an MVP lean. See [[turn-tracker-status]].

**Finishing habit (corrected 2026-06-18):** Since starting to work with Claude Code, the user reliably **finishes one project before moving on** — productivity "shot up." The many abandoned/scratch repos across G:\*Dev (rusteeze, ruxel, snap-rs, solo_rpg, tmpdir, etc.) are **pre-collaboration** and are NOT evidence of a current finishing problem. Do not frame recommendations around "you don't finish things." The recurring patterns (self-hosted Actix+Vue apps; LLM-pipeline→Telegram digests; gaming + quantified-self themes) are deliberate working preferences, not abandonment risk.
