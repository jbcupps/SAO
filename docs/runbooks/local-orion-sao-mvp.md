# Local OrionII + SAO MVP Runbook

End-to-end walkthrough: log in → admin configures Ollama → user creates an entity → downloads the
bundle → installs OrionII → entity phones home and chats through the SAO LLM proxy.

## Prerequisites

- Rust stable, Node.js 24, npm, Docker Desktop, PowerShell.
- SAO checked out at `C:\Repo\SAO`, OrionII at `C:\Repo\OrionII`.
- A locally running Ollama (or a Docker-network-reachable one) — required for the closed loop.
- A local-only `SAO_JWT_SECRET` shared across SAO and OrionII (for the dev fallback path; the
  bundle flow is fully self-contained once the bundle is downloaded).

## 1. Build the OrionII installer (one-time per code change)

The bundle endpoint serves a real `.msi`. Build it before the first run:

```powershell
cd C:\Repo\OrionII
npm ci
npm run tauri build -- --bundles msi
```

This produces an installer at:

```
C:\Repo\OrionII\src-tauri\target\release\bundle\msi\OrionII_0.1.0_x64_en-US.msi
```

## 2. Start SAO with the installer mount

The SAO container can't see host filesystem paths, so we mount the OrionII MSI directory into
the container and tell the bundle endpoint where to find it:

```powershell
cd C:\Repo\SAO
$env:POSTGRES_PASSWORD            = "local-dev-only-change-me"
$env:SAO_JWT_SECRET               = "local-dev-only-change-me"
$env:SAO_LOCAL_BOOTSTRAP          = "true"
$env:SAO_LOCAL_ADMIN_USERNAME     = "local-admin"
# Bundle endpoint config — embedded in every config.json:
$env:SAO_PUBLIC_BASE_URL          = "http://localhost:3100"
# Installer mount (host path) + filename — Compose mounts this dir read-only at /installer:
$env:SAO_ORION_INSTALLER_DIR      = "C:\Repo\OrionII\src-tauri\target\release\bundle\msi"
$env:SAO_ORION_INSTALLER_FILENAME = "OrionII_0.1.0_x64_en-US.msi"

docker compose -f docker\docker-compose.yml up --build
```

If you're not yet ready to serve installers, omit `SAO_ORION_INSTALLER_DIR` —
Compose falls back to an empty placeholder mount, and the bundle endpoint returns a 503 with a
clear remediation message until you stage the MSI.

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

Browse to `http://localhost:3100`, register Windows Hello for `local-admin`, then sign in.

## 3. (Admin) Configure LLM providers

Go to **/admin/llm-providers**. Each provider has its own card. Configure as many as you want;
entities pick one at creation time and SAO routes their `/api/llm/generate` calls to it.

### Cloud providers (OpenAI, Anthropic Claude, xAI Grok, Google Gemini)

For each one you want to enable:

1. Get an API key from the provider console linked under the key field:
   - OpenAI: `sk-...` from https://platform.openai.com/api-keys
   - Anthropic: `sk-ant-...` from https://console.anthropic.com/settings/keys
   - Grok: `xai-...` from https://console.x.ai/
   - Gemini: `AIza...` from https://aistudio.google.com/apikey
2. Toggle **Enabled** on.
3. Paste the key into **API key** (write-only — never returned by GET).
4. Tick the preset models you want to allow, or paste comma-separated overrides.
5. Set **Default model** (auto-filled from the provider's preset).
6. Click **Save**. The vault encrypts the key at rest under
   `(provider=<name>, label='api_key', secret_type='api_key')`.
7. Click **Test connection** — SAO sends a tiny ping prompt through the real provider path and
   surfaces the latency + preview text or the upstream error.

### Ollama (local self-hosted)

1. Toggle **Enabled** on.
2. Set base URL — from the SAO container, the host's Ollama is at
   `http://host.docker.internal:11434`.
3. Click **Refresh models** — the live `/api/tags` response populates a checkbox list.
4. Tick the models you want to allow.
5. Set **Default model** (e.g., `llama3.2`).
6. Click **Save**, then **Test connection**.

> Whichever provider you enable, the entity always calls SAO. There is no direct entity → upstream
> path. Switching providers, rotating a key, or revoking a token in SAO applies instantly without
> touching any installed entity.

## 4. (User) Create an entity

Go to **/agents** → **+ Register Agent**:

1. **Name**: e.g. `abigail`.
2. **LLM Provider**: choose `ollama`.
3. **Id model** / **Ego model**: `llama3.2` (autofills from provider default).
4. Click **Register**.

A new card appears with the agent's UUID, `LLM: ollama / llama3.2 / llama3.2`, status `offline`.

## 5. (User) Download the bundle

Click **Download bundle** on the agent card. A file like
`Orion-abigail-12345678.zip` downloads. Inside:

| File | Purpose |
|---|---|
| `config.json` | SAO base URL, agent_id, **entity JWT**, default models. |
| `OrionII-Setup.msi` | Tauri installer. |
| `README-FIRST-RUN.txt` | Install steps. |

The download mints a fresh entity JWT, revokes any prior tokens for that agent, and audit-logs
`agents.bundle_downloaded`.

## 6. Install + run OrionII

1. Run `OrionII-Setup.msi` and finish the install wizard.
2. Drop `config.json` into `%APPDATA%\OrionII\config.json` (create the folder if needed).
   The bootstrap loader also accepts `config.json` co-located with the executable.
3. Launch OrionII.

On first launch:
- Bootstrap reads `config.json` and adopts the SAO-assigned `agent_id` as the local
  `orion_id`.
- The model router is configured for `SaoProxyWithFallback` against the chosen provider.
- The first chat call goes out as `POST {sao_base_url}/api/llm/generate` with the entity JWT.

Type a message in OrionII; you should see a real response from `llama3.2` proxied through SAO.

## 7. Verify the loop

Back in SAO:

- **/agents** — the badge should turn `active` after the first egress event lands.
- Click **Logs** on the agent card → **/agents/:id/events** — see `identitySync`, `auditAction`,
  and (if you indexed a doc) `memoryEvent` rows trickle in (page polls every 5s).
- **/audit** — see `llm.generate`, `agents.bundle_downloaded`, `orion.egress` rows attributed to
  the human owner with the agent_id surfaced.

## 8. Re-issue / revoke

- **Re-download**: clicking **Download bundle** again revokes the old token and mints a new
  one — install the new config.json and restart OrionII.
- **Delete**: deleting the agent in SAO bulk-revokes all tokens for that agent. The next
  `/api/orion/policy` or `/api/llm/generate` call from the old install returns 401.

## Dev/legacy fallback flow (no bundle)

The original env-driven path still works for development:

```powershell
cd C:\Repo\OrionII
$env:SAO_BASE_URL          = "http://localhost:3100"
$env:SAO_DEV_BEARER_TOKEN  = (docker compose -f C:\Repo\SAO\docker\docker-compose.yml run --rm sao sao-server mint-dev-token | Select-Object -Last 1)
$env:SAO_AGENT_ID          = "<optional>"
npm run tauri dev
```

In this mode the bearer is a user JWT (admin acting as the entity) and the model layer falls back
to local Ollama (no SAO LLM proxy). Useful for iterating on OrionII before rebuilding the MSI.

## Troubleshooting

- `503 OrionII installer is not staged` from the bundle endpoint — set
  `SAO_ORION_INSTALLER_DIR` + `SAO_ORION_INSTALLER_FILENAME` and re-run `docker compose up`
  (the directory is mounted into the container at `/installer`).
- `400 Agent has no default LLM provider configured` — the agent was created before the wizard
  fields existed. Delete and recreate it.
- `400 Configured provider is currently disabled` — admin disabled the provider after the agent
  was created. Re-enable in `/admin/llm-providers`.
- Ollama probe returns `connection refused` from the SAO container — use
  `http://host.docker.internal:11434` instead of `127.0.0.1`.
- SAO LLM proxy returns 502 with `connection refused` — same fix; check the OrionII container
  can reach the host's Ollama.
- Entity JWT decoded but **revoked** — someone deleted the agent or downloaded a fresh bundle.
  Download again.

## Validation Gate

Run before calling MVP green:

```powershell
cd C:\Repo\SAO
cargo test --workspace
cargo clippy --workspace -- -D warnings
npm --prefix frontend test

cd C:\Repo\OrionII
npm ci
npm run build
cargo test --manifest-path src-tauri\Cargo.toml --locked
cargo clippy --manifest-path src-tauri\Cargo.toml --locked -- -D warnings
```
