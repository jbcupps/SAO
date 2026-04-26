# SAO — Secure Agent Orchestrator

The enterprise control plane for AI agents: centralized identity, secrets vault, skill governance, and lifecycle management.

## Quick Start

Run the conversational bootstrap installer:

```bash
docker run --rm -it \
  -e ANTHROPIC_API_KEY=sk-ant-your-key-here \
  ghcr.io/jbcupps/sao-installer:latest
```

Windows PowerShell:

```powershell
docker run --rm -it -e ANTHROPIC_API_KEY="sk-ant-your-key-here" ghcr.io/jbcupps/sao-installer:latest
```

In one guided session the installer signs you into Azure, validates permissions, provisions the SAO control plane, and prints the live URL when the platform is ready.

The default Azure deployment now hardens the runtime by:

- keeping PostgreSQL on private delegated networking instead of `0.0.0.0`
- mounting durable SAO data at `/data/sao` with Azure Files
- seeding secure browser session settings and Entra bootstrap inputs at deploy time
- verifying the Container App and `/api/health` before handoff

When the installer finishes, open the SAO URL and sign in with Microsoft Entra ID.

## What Is SAO?

SAO is the control plane that installs itself through a governed AI conversation. Instead of shipping employee API keys, ad hoc secrets, and unreviewed skills into every agent runtime, SAO centralizes identity, secret custody, entitlement decisions, audit logging, and lifecycle control in one platform.

The result is an agent estate that behaves more like managed infrastructure than shadow automation.

## Why SAO?

- No employee API keys in agent code or local prompts.
- Governed skills and bindings instead of unrestricted tool sprawl.
- Centralized agent identity, ownership, and lifecycle controls.
- Auditable bootstrap, sign-in, secret access, and review events.
- Zero-trust browser sessions with cookie-based auth, CSRF protection, and least-privilege routing.
- Azure-ready deployment that favors private data paths and durable runtime state.

## One-Command Bootstrap Experience

The installer is the hero workflow for SAO.

1. It opens a conversational session inside the installer container.
2. It walks the operator through Azure login and subscription targeting.
3. It validates permissions before any write action.
4. It deploys the hardened Azure footprint for SAO.
5. It verifies the runtime, prints the endpoint, and hands off to the live control plane.

Optional deployment inputs can be passed as installer environment variables when you want the first SAO login experience pre-seeded with Entra details or stricter browser origin settings:

- `SAO_INSTALLER_FRONTEND_URL`
- `SAO_INSTALLER_ALLOWED_ORIGINS`
- `SAO_INSTALLER_JWT_SECRET`
- `SAO_INSTALLER_OIDC_ISSUER_URL`
- `SAO_INSTALLER_OIDC_CLIENT_ID`
- `SAO_INSTALLER_OIDC_CLIENT_SECRET`
- `SAO_INSTALLER_OIDC_PROVIDER_NAME`
- `SAO_INSTALLER_OIDC_SCOPES`

## Prerequisites

- Docker
- An Azure subscription where you can create resource groups, networking, PostgreSQL Flexible Server, Key Vault, Storage, Log Analytics, and Container Apps
- A Microsoft Entra account that can act as the bootstrap admin
- An Anthropic API key for the conversational installer

Advanced setup: baseline Entra groups

Use this PowerShell helper if you want the standard SAO group scaffold before first login:

```powershell
$ErrorActionPreference = "Stop"

$requiredGroups = @(
    "SAO - Users",
    "SAO - System Admins",
    "SAO - Security Admins",
    "SAO - Developers"
)

if (-not (Get-Command az -ErrorAction SilentlyContinue)) {
    throw "Azure CLI (az) is required but was not found in PATH."
}

$account = az account show --output json | ConvertFrom-Json
if (-not $account) {
    throw "No active Azure session found. Run 'az login' first."
}

$graphToken = az account get-access-token --resource-type ms-graph --output json | ConvertFrom-Json
if (-not $graphToken.accessToken) {
    throw "Unable to acquire a Microsoft Graph access token."
}

$headers = @{
    Authorization = "Bearer $($graphToken.accessToken)"
    "Content-Type" = "application/json"
}

$me = Invoke-RestMethod -Method Get -Uri "https://graph.microsoft.com/v1.0/me" -Headers $headers
if (-not $me.id) {
    throw "Unable to resolve the signed-in Microsoft Entra user."
}

foreach ($displayName in $requiredGroups) {
    $mailNickname = ($displayName -replace '[^A-Za-z0-9]', '').ToLower()
    $encodedFilter = [System.Uri]::EscapeDataString("displayName eq '$displayName'")
    $existing = Invoke-RestMethod -Method Get -Uri "https://graph.microsoft.com/v1.0/groups?`$filter=$encodedFilter" -Headers $headers

    if ($existing.value.Count -gt 0) {
        $group = $existing.value[0]
        Write-Host "Group already exists: $displayName"
    }
    else {
        $body = @{
            displayName     = $displayName
            mailEnabled     = $false
            mailNickname    = $mailNickname
            securityEnabled = $true
        } | ConvertTo-Json

        $group = Invoke-RestMethod -Method Post -Uri "https://graph.microsoft.com/v1.0/groups" -Headers $headers -Body $body
        Write-Host "Created group: $displayName"
    }

    $members = Invoke-RestMethod -Method Get -Uri "https://graph.microsoft.com/v1.0/groups/$($group.id)/members?`$select=id" -Headers $headers
    $alreadyMember = $members.value | Where-Object { $_.id -eq $me.id }

    if (-not $alreadyMember) {
        $memberBody = @{
            "@odata.id" = "https://graph.microsoft.com/v1.0/directoryObjects/$($me.id)"
        } | ConvertTo-Json

        Invoke-RestMethod -Method Post -Uri "https://graph.microsoft.com/v1.0/groups/$($group.id)/members/`$ref" -Headers $headers -Body $memberBody | Out-Null
        Write-Host "Added signed-in user to: $displayName"
    }
    else {
        Write-Host "Signed-in user already present in: $displayName"
    }
}

Write-Host ""
Write-Host "SAO baseline Entra ID security groups are ready."
```

## Architecture Highlights

- Installer-led Azure deployment: [docs/bootstrap-installer.md](docs/bootstrap-installer.md)
- Runtime and trust boundaries: [docs/architecture.md](docs/architecture.md)
- Installer control flow and recovery model: [docs/installer-architecture.md](docs/installer-architecture.md)
- Deployment contract and operator workflow: [docs/SAO_INSTALLER_SPEC.md](docs/SAO_INSTALLER_SPEC.md)
- Vault, registry, and governed skill surfaces: [docs/VAULT_AND_REGISTRY.md](docs/VAULT_AND_REGISTRY.md)
- Skills as governed artifacts: [docs/agent_archetype.md](docs/agent_archetype.md)
- OrionII entity contract (LLM proxy, entity JWT, bundle download): [docs/orion-sao-mvp.md](docs/orion-sao-mvp.md)
- Detailed architecture source: [documents/SAO_Orion_Architecture_Analysis_v2.docx](documents/SAO_Orion_Architecture_Analysis_v2.docx)

## Entity Lifecycle (OrionII)

SAO is the issuer of OrionII entities. The full flow is in
[docs/runbooks/local-orion-sao-mvp.md](docs/runbooks/local-orion-sao-mvp.md). At a glance:

1. Admin signs in, opens **/admin/llm-providers**, and per-provider:
   - OpenAI / Anthropic / xAI Grok / Google Gemini — paste the API key, tick approved models,
     set a default, click **Test connection** to confirm with a real ping.
   - Ollama — set the base URL, click **Refresh models** to pull the live list, tick allowed
     ones.

   Keys are stored encrypted in the vault; admins never see the key after save. **Every entity
   call goes through `POST /api/llm/generate` on SAO**, so keys never leave the server, every
   prompt is auditable, and key rotation/revocation is instant.
2. User signs in, opens **/agents**, registers a new entity (name + provider + Id/Ego model).
3. User clicks **Download bundle**. SAO mints a fresh OIDC-shaped entity JWT (revoking any
   prior tokens for that agent), packages a ZIP with `config.json` + `OrionII-Setup.msi`.
4. User installs OrionII, drops `config.json` into `%APPDATA%\OrionII\`, launches the app.
5. OrionII adopts the SAO-assigned identity, calls `POST /api/llm/generate` for every Id/Ego
   prompt — keys never leave SAO. Per-agent egress events stream into **/agents/:id/events**.

Deleting an agent in SAO bulk-revokes its tokens.

## Development & Contributing

For local development, the Azure installer is still the production story, but the repo includes a local Compose workflow for integration work:

```bash
POSTGRES_PASSWORD=local-dev-only-change-me docker compose -f docker/docker-compose.yml up --build
```

Windows PowerShell:

```powershell
$env:POSTGRES_PASSWORD = "local-dev-only-change-me"
$env:SAO_JWT_SECRET = "local-dev-only-change-me"
docker compose -f docker\docker-compose.yml up --build
```

After Compose starts, use the local development bootstrap command from `docs/runbooks/local-orion-sao-mvp.md` to initialize the local vault and admin user without enabling a browser setup wizard.

To validate the local SAO side of the OrionII integration:

```powershell
# Minimum: health, bootstrap, dev token, policy pull, egress ack.
.\scripts\local-mvp-smoke.ps1 -StartCompose

# Full bundle round-trip: also configures Ollama, creates an agent, downloads the bundle.
.\scripts\local-mvp-smoke.ps1 -StartCompose -OllamaBaseUrl "http://host.docker.internal:11434" -OllamaModel "llama3.2"
```

To serve real OrionII installers from `/api/agents/:id/bundle`, build the MSI in OrionII first and
point Compose at it via two host env vars before running `up`:

```powershell
# In OrionII:
npm run tauri build -- --bundles msi

# In SAO (host shell):
$env:SAO_ORION_INSTALLER_DIR      = "C:\Repo\OrionII\src-tauri\target\release\bundle\msi"
$env:SAO_ORION_INSTALLER_FILENAME = "OrionII_0.1.0_x64_en-US.msi"
$env:SAO_PUBLIC_BASE_URL          = "http://localhost:3100"
docker compose -f docker\docker-compose.yml up --build
```

The Compose file mounts `SAO_ORION_INSTALLER_DIR` read-only at `/installer` inside the container
and exposes the file via `SAO_ORION_INSTALLER_PATH=/installer/<filename>`.

Useful validation commands:

```bash
cargo test
cargo clippy --workspace -- -D warnings
npm --prefix frontend test
npm --prefix frontend run build
python -m unittest discover installer/tests
POSTGRES_PASSWORD=local-dev-only-change-me docker compose -f docker/docker-compose.yml config
az bicep build --file installer/bicep/main.bicep
```

Repository notes:

- `skills/` contains example governed skill artifacts and policies, not a separate deployment target.
- `installer/Dockerfile` builds the conversational bootstrap helper image.
- `docker/Dockerfile` builds the production SAO runtime image for Azure Container Apps.

Please keep documents/SAO_Orion_Architecture_Analysis_v2.docx (and the attached Toward a Decentralized Trust Framework.pdf) up to date in the repo — it is the single source of truth for all architecture decisions, build phases, and security requirements.
