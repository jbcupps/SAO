# SAO — Secure Agent Orchestrator

The enterprise control plane for AI agents — centralized identity, secrets vault, skill governance, and lifecycle management.

## Quick Start / Installation

Start with the Azure path if you're evaluating SAO the way an enterprise security or platform team would actually deploy it. Local Docker and standalone installer flows are still included below for development, testing, and implementation work.

### Prerequisites

- Docker Desktop
- Azure CLI (`az`)
- Microsoft Entra ID tenant access with either `Global Administrator` or `Privileged Role Administrator`

### Step 1: Provision Entra ID Security Groups

Run this PowerShell script to create the four baseline SAO security groups and automatically add the currently signed-in user to each one.

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

### Step 2: Choose your install path

#### A. Recommended: Install in your Azure environment

Use this path when you want to deploy SAO into Azure with the production runtime model: Entra-backed identity, Azure resource provisioning, and the conversational installer guiding bootstrap.

Production Azure runtime notes:

- Production image: `ghcr.io/jbcupps/sao:<tag>`
- Production Dockerfile: `docker/Dockerfile`
- Azure runtime target: Azure Container Apps with PostgreSQL and secret-backed `DATABASE_URL`
- Do not use `installer/Dockerfile` as the Azure application image

Run the standalone installer container locally, then let it guide the Azure deployment:

```powershell
# Azure install path
docker build -f installer/Dockerfile -t sao-installer installer
docker run --rm -it -e ANTHROPIC_API_KEY=sk-ant-your-key-here sao-installer
```

The installer will guide you through:

- Azure sign-in and subscription targeting
- Entra administrator authentication
- Resource group and deployment setup
- Production SAO application deployment in Azure
- Post-deployment validation and health checks

#### B. Full SAO Platform (local Docker)

This path brings up the complete local platform: dashboard, backend API, PostgreSQL, `/api/health`, and the first-run installer experience on your workstation.

```powershell
# Full platform (local Docker)
docker compose -f docker/docker-compose.yml up -d --build
# Access: http://localhost:3100
```

#### C. Standalone Conversational Installer

Use this when you want to test or iterate on the installer flow without bringing up the full platform UI.

```powershell
# Standalone conversational installer (testing/dev)
docker build -f installer/Dockerfile -t sao-installer installer
docker run --rm -it -e ANTHROPIC_API_KEY=sk-ant-your-key-here sao-installer
```

### Step 3: Verify the platform

For the full platform path, confirm the service is healthy:

```powershell
curl http://localhost:3100/api/health
```

Expected response:

```json
{
  "status": "ok",
  "service": "sao",
  "version": "0.0.1",
  "database": {
    "connected": true,
    "healthy": true
  }
}
```

Local defaults:

- SAO UI and API: `http://localhost:3100`
- PostgreSQL: `localhost:5432`
- Docker Compose is the recommended local development workflow
- On a fresh database, SAO enters installer mode instead of exposing a legacy setup wizard

## What SAO Delivers

SAO is built for enterprises that need AI agents to operate inside explicit identity, policy, and audit boundaries rather than as isolated tools or opaque scripts.

- Centralized identity for agent and operator access
- Vault-backed secret custody and encryption
- Skill governance with bounded capability surfaces
- Lifecycle management for bootstrap, runtime, and operations
- Conversational installation with traceable actions and resumable progress

## Identity And Access Model

SAO follows an Entra-first bootstrap model.

- The first administrator authenticates through Microsoft Entra ID using OIDC
- SAO records the authenticated Entra Object ID as the founding admin identity
- SAO does not create an operator-facing bootstrap password
- Group-driven access control can be aligned to Entra security groups discovered during install

The four baseline groups above give most teams a clean starting point:

- `SAO - Users`
- `SAO - System Admins`
- `SAO - Security Admins`
- `SAO - Developers`

## Deployment Paths

### Local Docker Platform

Use Docker Compose for the fastest developer and evaluation setup. This runs the SAO application container plus PostgreSQL and exposes the platform on port `3100`.

### Standalone Installer Container

The installer-only container exists for bootstrap testing, development, and conversational install work. It is useful when you want to exercise installer behavior independently from the main dashboard runtime.

### Production Azure Runtime

The production Azure application image is `ghcr.io/jbcupps/sao:<tag>`, built from `docker/Dockerfile`.

- This is the image contract for Azure Container Apps
- It includes the frontend bundle, static assets, `/api/health`, and runtime server behavior
- `installer/Dockerfile` is not the production Azure runtime image path
- `DATABASE_URL` is supplied through a Container Apps secret reference in production

## Security And Governance

SAO is designed so installation and operation begin inside controlled boundaries, not outside them.

- Identity-first provisioning instead of generated bootstrap credentials
- Auditable installer actions and tool execution
- Encrypted secret storage and vault lifecycle controls
- Governed skill surfaces instead of unrestricted embedded logic
- Resume-safe installation flow with explicit operator visibility

Privacy and execution note:

- The installer conversation and tool results are sent to Anthropic as part of runtime model execution
- Local transcript files remain local unless an operator explicitly shares them

## Operational Notes

- Fresh installs enter installer mode automatically when the user table is empty
- If installation is interrupted, SAO is designed to resume from persisted state
- The installer explains each step before acting and can help validate required tenant and application details
- Production readiness means infrastructure deployed successfully, the latest Container App revision is healthy, and `/api/health` returns healthy

## Architecture Overview

SAO should read as a control plane that installs itself through policy-aware dialogue: identity first, credentials never fabricated, and system state established through a traceable conversation.

### Conversational installer

Instead of a traditional setup wizard, SAO boots into a managed conversation that provisions the platform with the same controls it later enforces.

- The installer is a multi-turn agent-driven flow
- It can validate or gather tenant and application prerequisites
- It resumes from partial progress rather than forcing a restart
- It keeps the operator inside a governed, auditable workflow

### Birth documents

Every registered agent is grounded in signed origin material:

- `soul.md`
- `ethics.md`
- `org-map.md`
- `personality.md`

These documents anchor agent identity in signed artifacts rather than informal configuration alone.

### Skill governance

SAO treats skills as governed capability surfaces. Specialized behaviors are meant to be installed, approved, and routed with explicit boundaries, which supports enterprise review, safer reuse, and clearer operator control.

### Runtime verification

SAO is designed to verify more than configuration correctness. Its model connects runtime action to declared constitutional, ethical, and governance artifacts.

## Supporting Documents

- [Architecture overview](docs/architecture.md)
- [Installer architecture](docs/installer-architecture.md)
- [Installer specification](docs/SAO_INSTALLER_SPEC.md)
- [Deep dive](docs/sao-deep-dive.md)
- [Vault and registry notes](docs/VAULT_AND_REGISTRY.md)
- [SAO_Orion_Architecture_Analysis_v2.docx](documents/SAO_Orion_Architecture_Analysis_v2.docx)
- [Toward a Decentralized Trust Framework.pdf](documents/Toward%20a%20Decentralized%20Trust%20Framework.pdf)

## License

MIT
