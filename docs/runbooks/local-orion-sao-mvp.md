# Local OrionII + SAO MVP Runbook

End-to-end walkthrough — admin configures provider keys + an installer source, a user creates an
entity, downloads a self-contained bundle, and the entity births dynamically and chats through
SAO's LLM proxy.

## Prerequisites

- Rust stable, Node.js 24, npm, Docker Desktop, PowerShell.
- SAO checked out at `C:\Repo\SAO`, OrionII at `C:\Repo\OrionII`.
- An API key for at least one cloud LLM provider (OpenAI / Anthropic / Grok / Gemini), OR a
  reachable Ollama instance.

## 1. Build the OrionII installer (one-time per OrionII code change)

```powershell
cd C:\Repo\OrionII
npm ci
npm run tauri build -- --bundles msi
```

Output: `src-tauri/target/release/bundle/msi/OrionII_<version>_x64_en-US.msi`. SAO will fetch
this from a URL you give it later — see step 4.

## 2. Start SAO

```powershell
cd C:\Repo\SAO
$env:POSTGRES_PASSWORD            = "local-dev-only-change-me"
$env:SAO_JWT_SECRET               = "local-dev-only-change-me"
$env:SAO_LOCAL_BOOTSTRAP          = "true"
$env:SAO_LOCAL_ADMIN_USERNAME     = "local-admin"
$env:SAO_LOCAL_VAULT_PASSPHRASE   = "local-dev-only-change-me"
# Auto-unseal the vault on every container start so LLM provider keys stay readable
# without a manual unseal step:
$env:SAO_VAULT_PASSPHRASE         = "local-dev-only-change-me"
# Embedded into every config.json so OrionII knows where to phone home:
$env:SAO_PUBLIC_BASE_URL          = "http://localhost:3100"

docker compose -f docker\docker-compose.yml up --build -d
```

In a second window, run the bootstrap:

```powershell
cd C:\Repo\SAO
$env:POSTGRES_PASSWORD            = "local-dev-only-change-me"
$env:SAO_JWT_SECRET               = "local-dev-only-change-me"
$env:SAO_LOCAL_BOOTSTRAP          = "true"
$env:SAO_LOCAL_VAULT_PASSPHRASE   = "local-dev-only-change-me"
$env:SAO_LOCAL_ADMIN_USERNAME     = "local-admin"
docker compose -f docker\docker-compose.yml run --rm sao sao-server bootstrap-local
```

Confirm `bootstrap_mode: operational` and the vault is unsealed:

```powershell
Invoke-RestMethod http://localhost:3100/api/setup/status
Invoke-RestMethod http://localhost:3100/api/vault/status   # status: "unsealed"
```

Open `http://localhost:3100`, register Windows Hello for `local-admin`, then sign in.

## 3. (Admin) Configure LLM providers

Go to **/admin/llm-providers**. Each provider has its own card:

### Cloud (OpenAI / Anthropic Claude / xAI Grok / Google Gemini)

1. Get a key from the provider console (link is on the card):
   - OpenAI `sk-...` — https://platform.openai.com/api-keys
   - Anthropic `sk-ant-...` — https://console.anthropic.com/settings/keys
   - Grok `xai-...` — https://console.x.ai/
   - Gemini `AIza...` — https://aistudio.google.com/apikey
2. Toggle **Enabled**.
3. Paste the key (write-only).
4. Tick the preset models you want allowed (or paste comma-separated overrides).
5. Set **Default model**.
6. Click **Save**, then **Test connection** — SAO sends a real ping prompt and surfaces
   latency + preview text.

### Ollama

Set the base URL (`http://host.docker.internal:11434` from inside the SAO container), click
**Refresh models** to pull the live `/api/tags`, tick what you want, save, test.

## 4. (Admin) Register the OrionII installer source

Go to **/admin/installer-sources** → **+ Register installer source**:

1. Paste a download URL — convention is the GitHub Releases `latest` URL:
   ```
   https://github.com/jbcupps/OrionII/releases/latest/download/OrionII_0.1.0_x64_en-US.msi
   ```
   (Or any URL SAO can `GET`.)
2. Click **Probe sha256** — SAO computes the digest of what it sees at that URL.
3. Confirm the sha matches what you expect, then fill **Filename** + **Version label** (the
   form pre-fills sensible defaults).
4. Tick **Make this the default**.
5. Click **Register + warm cache** — SAO downloads, sha-verifies, and writes the file under
   `SAO_DATA_DIR/installers/<sha>/`.

After this, every new agent gets pinned to that sha at create time, and bundle downloads
serve straight from the local cache. No host-side MSI staging required.

## 5. (User) Create an entity

Visit **/agents** → **+ Register Agent**:

1. Name (e.g., `abigail`).
2. LLM provider (dropdown of admin-enabled providers).
3. Id model + Ego model (autofill from the provider's default).
4. Click **Register**.

The new card shows the agent's UUID, the chosen LLM, and an `offline` badge. The agent row in
the DB now has the current default installer's sha pinned to it.

## 6. (User) Download the bundle

Click **Download bundle**. SAO mints a fresh OIDC-shaped entity JWT (revoking any prior tokens
for that agent), packages a ZIP from the cached MSI, and audit-logs `agents.bundle_downloaded`.

The ZIP contains:

| File | Purpose |
|---|---|
| `config.json` | SAO base URL + entity JWT (the anchor). Two extra fields are kept as
fallback for offline mode + back-compat. |
| `OrionII-Setup.msi` | Tauri installer for Windows. |
| `README-FIRST-RUN.txt` | Install steps for the user. |

## 7. Install + run OrionII

1. Run `OrionII-Setup.msi` and finish the install wizard.
2. **Either** drop `config.json` into `%APPDATA%\OrionII\config.json`,
   **or** launch OrionII first and paste the JSON into the yellow **Enroll with SAO** panel
   that appears in the app — click **Apply config** and OrionII writes it for you and
   hot-swaps the running core (no restart needed).
3. The status card flips from `offline` → `birthed`, showing
   `Birthed as <name> via <provider> (policy v1)` plus owner / id-model / ego-model.

On launch:
- Bootstrap reads `config.json` (sao_base_url + entity JWT).
- Calls `GET /api/orion/birth` to fetch live agent metadata, endpoints, scopes, current
  policy, personality seed.
- Adopts the SAO-assigned `agent_id` as the local `orion_id`.
- The model router is in `SaoProxyWithFallback` mode: every Id/Ego prompt POSTs to
  `/api/llm/generate` with `Authorization: Bearer <entity-JWT>`. Keys never leave SAO.

## 8. Verify the loop

In SAO:

- **/agents** — badge turns `active` after the first egress event.
- **Logs** button on the agent card → **/agents/:id/events** — `identitySync`, `auditAction`,
  `memoryEvent` rows arrive every few seconds (page polls).
- **/audit** — `llm.generate` rows show provider/model/latency_ms; `agents.bundle_downloaded`,
  `orion.egress`, `orion.birth` are attributed to the human owner.

In OrionII:

- Status card shows `birthed`, owner, provider, models.
- Send a chat message — you should see a real model response (no longer the deterministic
  fallback "Orion is operating as a persistent local companion..." stub).

## 9. Re-issue / revoke

- **Re-download bundle** — revokes the old token and mints a new one. Re-paste the new
  config.json (or drop it into `%APPDATA%\OrionII\` again).
- **Delete the agent in SAO** — bulk-revokes all of its tokens. Subsequent `/api/orion/policy`
  / `/api/llm/generate` calls from the old install return 401.
- **Roll the installer source forward** — register a new source with a new sha, set as
  default. New agents pin to the new sha; existing agents keep their original pin so they
  stay reproducible.

## What lives where on disk (inside the container)

| Path | Contents |
|---|---|
| `/data/sao/jwt_secret.bin` | Local JWT signing key (persists across restarts). |
| `/data/sao/installers/<sha>/<filename>` | sha256-verified installer cache. |
| `/data/sao/installers/<sha>/.url` | Source URL marker for traceability. |
| Postgres `installer_sources` | Registered MSI sources (URL, expected sha, version, default). |
| Postgres `agents.installer_sha256/filename/version` | Per-agent pin. |
| Postgres `agent_tokens` | jti-keyed revocation rows for entity JWTs. |
| Postgres `orion_egress_events` | Idempotent ingress of OrionII events. |
| Postgres `audit_log` | All authenticated actions + LLM proxy calls. |

## Watching live activity (the SAO console)

```powershell
# tail everything
docker compose -f docker\docker-compose.yml logs -f sao

# just LLM/orion-related lines
docker compose -f docker\docker-compose.yml logs -f sao | Select-String "llm|orion|generate|birth"

# debug-level (handler entry/exit, request bodies on warns)
$env:RUST_LOG = "sao_server=debug,axum=info"
docker compose -f docker\docker-compose.yml up -d
```

For the canonical record of what happened, query the audit log directly:

```sql
SELECT created_at, action, details->>'model' AS model,
       details->>'latency_ms' AS ms, details->>'error' AS error
FROM audit_log WHERE action LIKE 'llm.%' ORDER BY created_at DESC LIMIT 20;
```

## Troubleshooting

| Symptom | Cause | Fix |
|---|---|---|
| `503 OrionII installer is not available` | No installer source registered AND no `SAO_ORION_INSTALLER_PATH` env var. | Register a source under `/admin/installer-sources` (preferred) or set the env var to a mounted MSI path. |
| Bundle download 503 with sha-mismatch | Cached file doesn't match registered sha (substituted upstream / corruption). | SAO refetches automatically on the next call; if upstream is permanently gone, register a new source. |
| OrionII shows `Degraded fallback` for MODEL after a fresh container start | Vault is sealed → LLM proxy can't decrypt the API key. | Set `SAO_VAULT_PASSPHRASE` so compose auto-unseals on startup. |
| OrionII chat returns the deterministic fallback text | `/api/llm/generate` is failing — check `audit_log` for `llm.generate.failed`. | Common causes: vault sealed, model not on approved list, upstream API rejected the key. |
| `503 Vault is sealed` on the LLM endpoint | Same as above. | Set `SAO_VAULT_PASSPHRASE`. |
| `400 Configured provider is currently disabled` | Admin disabled the provider after the agent was created. | Re-enable in `/admin/llm-providers`. |
| Ollama probe `connection refused` from container | `127.0.0.1` doesn't reach the host. | Use `http://host.docker.internal:11434`. |
| Entity token returns 401 | Token revoked (agent deleted, or new bundle downloaded). | Download a fresh bundle, re-paste config. |
| `release-installer` workflow fails on icon | Missing `bundle.icon` in `tauri.conf.json`. | Already fixed in OrionII PR #1 — make sure it's merged. |

## Validation Gate (before declaring MVP green)

```powershell
cd C:\Repo\SAO
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
npm --prefix frontend test

cd C:\Repo\OrionII
npm ci
npm run build
cargo test --manifest-path src-tauri\Cargo.toml --locked
cargo clippy --manifest-path src-tauri\Cargo.toml --locked --all-targets -- -D warnings
npm run tauri build -- --bundles msi
```
