# SAO ↔ OrionII — Project Status

_Last updated: 2026-04-26_

## Where we are

The end-to-end self-serve entity loop is **working and verified live** against a local
Docker Compose stack with a real Anthropic upstream:

> Admin signs in → registers an installer source (GitHub Releases URL) → configures
> Anthropic API key + Test connection → user creates an entity (auto-pinned to the default
> installer's sha) → user clicks Download bundle → installs the MSI → launches OrionII →
> entity calls `/api/orion/birth` and self-configures → chat input → SAO LLM proxy →
> Anthropic Haiku 4.5 → real response in the OrionII chat panel.

## What's shipped

### Identity
- **Entity JWTs** — OIDC-shaped, minted at bundle download, jti-keyed for revocation.
  `principal_type=non_human`, `human_owner=<creating user>`, `entity_kind=orion`,
  `scope=orion:policy orion:egress llm:generate`. Wire-shape is portable to a future Entra/IdP
  swap.
- **One live token per agent** — re-downloading a bundle revokes the prior token. Deleting
  an agent bulk-revokes all of its tokens.

### LLM proxy
- `POST /api/llm/generate` — entity-token-only, dispatches to **OpenAI**, **Anthropic Claude**,
  **xAI Grok**, **Google Gemini**, or **Ollama**.
- Provider keys live in the SAO vault under `provider:<name>:api_key`. Entities never hold
  upstream credentials.
- Per-call audit (`llm.generate` / `llm.generate.failed`) with provider, model, latency,
  error.
- `/api/llm/*` is a machine-client route group — bypasses browser CSRF when a Bearer token is
  present.

### Admin surface (`/admin/*`)
- `/admin/llm-providers` — per-provider catalog cards with key-format hints, console links,
  preset model lists, **Refresh models** for Ollama, **Test connection** that exercises the
  real upstream call path.
- `/admin/installer-sources` — register a download URL + expected sha256 + version. SAO
  downloads → sha-verifies → caches under `SAO_DATA_DIR/installers/<sha>/`. URL field
  pre-fills the GitHub Releases `/releases/latest/download/` convention. **Probe sha256**
  computes the digest before commit. Set-default and delete supported.

### User surface (`/agents`, `/agents/:id/events`)
- Agent registration wizard: name + provider + Id model + Ego model.
- Per-card **Download bundle** (mints fresh JWT, packages cached MSI + config.json + README)
  and **Logs** (per-agent live egress feed, polls every 5s).
- Live `last_heartbeat` derived from the latest egress event.

### Runtime config — dynamic via birth event
- `GET /api/orion/birth` returns one rich payload: agent metadata, endpoint URLs, owner,
  scopes, current policy, personality seed.
- OrionII calls birth on every launch; the response overrides bundle defaults so admin
  changes (provider switch, model swap, policy update) take effect on the next OrionII boot
  with no re-bundling.
- Bundle `config.json` is now an anchor (`sao_base_url` + `agent_token`); fallback fields
  remain for offline mode.

### OrionII desktop
- Tauri 2 + React 19 shell. Boots in under a second.
- Three status modes: **birthed** (live SAO, real LLM), **anchor only** (config loaded but
  birth call failed — running on bundle defaults), **offline** (no anchor at all).
- **In-app paste-config UI** — yellow "Enroll with SAO" panel visible until birth succeeds;
  pastes the JSON, validates, writes it to `%APPDATA%\OrionII\config.json`, hot-swaps the
  running OrionCore. No restart.
- Identity continuity preserved across reinstalls (durable JSON state file).
- Egress payloads stamp `clientVersion` for fleet observability.

### Operational ergonomics
- **Auto-unseal** — `SAO_VAULT_PASSPHRASE` env var unseals on startup so LLM keys stay
  readable across container restarts. Wired through compose by default.
- **Self-serve installer staging** — no more host-shell access required to drop an MSI on
  the server. Admin pastes a URL; SAO downloads + verifies + caches.
- **Sha-based pinning** — re-rolling the default installer never breaks existing agents;
  each agent stays bound to the sha it was created with.

## Verification

| Gate | Status |
|---|---|
| `cargo clippy --workspace --all-targets -- -D warnings` (SAO) | ✅ clean |
| `cargo test --workspace` (SAO) | ✅ 50 tests pass |
| `npx tsc --noEmit` + `npm test` (SAO frontend) | ✅ clean, 8 contract tests pass |
| `cargo clippy --all-targets -- -D warnings` (OrionII) | ✅ clean |
| `cargo test` (OrionII) | ✅ 18 tests pass |
| `npm run tauri build -- --bundles msi` | ✅ produces working MSI |
| `docker compose config` | ✅ validates |
| Live e2e against Anthropic Haiku 4.5 | ✅ real Claude response in OrionII chat |

## What's open

Tactical follow-ups (not blocking the loop):

- **Markdown rendering in the OrionII chat bubble** — Claude returns `**bold**`/numbered
  lists; today they show as raw asterisks. One-line drop-in for `react-markdown`.
- **Streaming LLM responses** — `/api/llm/generate` is request/response only. No SSE/WS
  streaming yet; the chat shows a static "Sending" until the full reply lands.
- **Token-at-rest encryption in OrionII** — bundle config holds the entity JWT in plaintext
  on disk. Threat model is local desktop, but Stronghold/DPAPI is a defensible follow-up.
- **GitHub Releases automation for the OrionII MSI** — workflow exists at
  [.github/workflows/release-installer.yml](https://github.com/jbcupps/OrionII/blob/main/.github/workflows/release-installer.yml);
  needs a tag push to fire and produce assets the installer-sources registry can consume.
- **Deep-link enrollment** — `orion://enroll?token=...` URL handler so the bundle page can
  one-click enroll an installed OrionII without paste/file-drop.
- **Tauri auto-updater** — install once, self-update against SAO. Needs Windows code signing
  for trust UX.
- **`/admin/console`** — built-in live view of the audit log + tracing stream over WebSocket
  so admins don't have to `docker compose logs`.
- **Per-agent provider override at runtime** — currently the agent picks one provider+model
  at create time; consider letting the entity request a different model per call (still
  gated by SAO's approved list).
- **Per-provider quota / rate limit** — keys are global today. Useful when multiple entities
  share one cloud key.
- **Dependabot vulns** — GitHub flags 19 on the SAO default branch (7 high). Pre-existing,
  unrelated to entity work; worth a separate sweep.

## Open PRs

- **SAO** — [#18 feat/orion-entity-bundle-llm-proxy](https://github.com/jbcupps/SAO/pull/18) —
  carries everything above on the SAO side: LLM proxy + 5 providers, entity JWTs, bundle
  endpoint, birth endpoint, installer source registry, CSRF exemption fix, auto-unseal,
  admin pages.
- **OrionII** —
  [#1 fix/tauri-bundle-icon](https://github.com/jbcupps/OrionII/pull/1) (one-line) and
  [#2 feat/dynamic-bootstrap-and-paste-ui](https://github.com/jbcupps/OrionII/pull/2)
  (birth client + paste UI; stacked on #1 — merge #1 first).

## Where to look when something breaks

| What | Where |
|---|---|
| What just happened | `/audit` (admin-wide) or `/agents/:id/events` (per entity). |
| LLM call failures + latencies | `audit_log` rows where `action LIKE 'llm.%'`. |
| Why a request was rejected | `docker compose logs sao` — `tracing::warn!` lines with `request_id` you can grep against `audit_log.details->>request_id`. |
| Vault state | `GET /api/vault/status`. |
| Installer cache contents | `docker compose exec sao ls /data/sao/installers/`. |
| Live chat traffic at handler depth | `RUST_LOG=sao_server=debug` then `docker compose logs -f sao`. |

## Coordinates

- SAO repo: <https://github.com/jbcupps/SAO>
- OrionII repo: <https://github.com/jbcupps/OrionII>
- Local SAO: <http://localhost:3100>
- Runbook: [docs/runbooks/local-orion-sao-mvp.md](runbooks/local-orion-sao-mvp.md)
- API contract: [docs/orion-sao-mvp.md](orion-sao-mvp.md)
- Architecture: [docs/architecture.md](architecture.md)
