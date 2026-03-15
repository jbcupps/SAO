# SAO Agentic Installer Specification

## Overview

The SAO installer is a Claude-powered conversational agent that replaces traditional setup wizards. On first launch (no users in PostgreSQL), the system enters **installer mode** — an AI agent guides the administrator through the entire bootstrap process via a chat terminal.

## Design Principles

1. **Conversational, not procedural**: The installer adapts to the admin's environment rather than following a rigid checklist
2. **Entra-first**: Admin identity is seeded from Microsoft Entra ID — no generated throwaway credentials
3. **Transparent**: An optional bash pane shows exactly what the agent executes
4. **Idempotent**: Interrupted installs resume from the last completed step
5. **Contained with explicit dependencies**: Runs inside the container, while intentionally calling Anthropic, Azure management APIs via `az`, Microsoft Entra ID, and Microsoft Graph

## Bootstrap Flow

### Step 1: Detect Installer Mode
- SAO server starts and queries PostgreSQL for user count
- If `COUNT(*) = 0` in users table → installer mode
- If users exist → operational mode (installer is inactive)

### Step 2: Agent Initialization
- Load system prompt from `installer/system_prompt.md`
- Initialize Claude client with Anthropic API key
- Check for existing installer state in `installer_state` table (resumption)
- Open WebSocket connection to frontend chat terminal

### Step 3: Admin Authentication (Entra OIDC)
- Agent prompts admin for their Entra tenant ID
- Initiates Authorization Code flow with PKCE
- Admin completes authentication in browser
- Agent receives callback with auth code
- Exchanges for tokens, extracts Object ID (OID) from id_token

### Step 4: Seed Admin Record
- INSERT admin into users table with `role = 'administrator'`
- OID becomes the permanent identity — no password stored
- Admin record is the seed from which all other identities derive

### Step 5: Generate Master Key
- Generate Ed25519 signing key pair
- Store private key encrypted in vault (AES-256-GCM)
- Record public key fingerprint in database
- This key signs all agent birth documents

### Step 6: Initialize Vault Encryption
- Derive vault encryption key from admin identity context
- Configure AES-256-GCM encryption for secret storage
- Test round-trip: store → retrieve → delete test secret

### Step 7: Entra App Registration
- Agent asks if admin has an existing app registration
- If yes: validate redirect URIs, permissions, token config
- If no: guide admin through creating one in Azure Portal
- Store OIDC config (issuer, client_id, client_secret) encrypted in PostgreSQL

### Step 8: Graph API Discovery
- Use admin's access token to query Microsoft Graph
- Discover: organization structure, security groups, app roles
- Suggest role mappings: which Entra groups → SAO User/Administrator
- Admin confirms or adjusts mappings

### Step 9: Optional Provisioning
- Agent offers to create test agent registrations
- Agent offers to configure additional SSO providers
- All optional — admin can skip

### Step 10: Full Stack Validation
- Database connectivity and migration status
- Vault seal/unseal cycle
- Auth flow (Entra OIDC round-trip)
- Graph API connectivity
- Agent registration and birth document generation
- Report results to admin

### Step 11: Transition to Operational Mode
- Mark installer state as `complete` in database
- Deactivate installer agent (audit trail preserved)
- System now serves operational frontend
- Chat terminal persists for operational use

## Tool Definitions

The installer agent has access to four tool categories:

### Entra Tools (`entra.*`)

| Tool | Purpose | Parameters |
|------|---------|------------|
| `entra.authenticate_admin` | Initiate OIDC auth flow | `tenant_id` |
| `entra.validate_app_registration` | Check app config | `client_id`, `tenant_id` |
| `entra.discover_tenant` | Query Graph API | `access_token` |
| `entra.suggest_role_mappings` | Map groups to SAO roles | `tenant_structure` |

### Vault Tools (`vault.*`)

| Tool | Purpose | Parameters |
|------|---------|------------|
| `vault.generate_master_key` | Create Ed25519 root key | (none) |
| `vault.initialize_encryption` | Set up AES-256-GCM | `admin_oid` |
| `vault.test_vault_operations` | Round-trip validation | (none) |

### PostgreSQL Tools (`postgres.*`)

| Tool | Purpose | Parameters |
|------|---------|------------|
| `postgres.check_connectivity` | Verify DB connection | (none) |
| `postgres.check_migrations` | Migration status | (none) |
| `postgres.run_migrations` | Apply pending migrations | (none) |
| `postgres.seed_admin` | Create admin record | `oid`, `name`, `email` |
| `postgres.check_existing_state` | Load resume state | (none) |
| `postgres.has_users` | Check if users exist | (none) |

### Bicep Tools (`bicep.*`)

| Tool | Purpose | Parameters |
|------|---------|------------|
| `bicep.validate_template` | Validate without deploying | `template_path` |
| `bicep.deploy` | Deploy Azure resources | `resource_group`, `template_path`, `parameters` |
| `bicep.check_deployment_status` | Check deployment | `resource_group`, `deployment_name` |

## Bicep Resource Summary

The production Azure deployment includes:

| Resource | Type | Purpose |
|----------|------|---------|
| Log Analytics Workspace | `Microsoft.OperationalInsights/workspaces` | Centralized logging |
| Container Apps Environment | `Microsoft.App/managedEnvironments` | Container hosting |
| SAO Container App | `Microsoft.App/containerApps` | SAO server (port 3100) |
| PostgreSQL Flexible Server | `Microsoft.DBforPostgreSQL/flexibleServers` | Database (v16, burstable) |
| PostgreSQL Database | `flexibleServers/databases` | `sao` database |

- Production app image contract: `ghcr.io/jbcupps/sao:<tag>`, built from `docker/Dockerfile`
- `DATABASE_URL` is injected into Container Apps through a `secretRef`, not a plain environment value
- Deployment success means ARM provisioning succeeded, the latest Container App revision is healthy, and `/api/health` reports healthy

## Security Boundaries

### Installer Agent Permissions
- **CAN**: Write to PostgreSQL, generate keys, call Graph API, execute container-local commands
- **CANNOT**: Access host filesystem outside mounted volumes, make arbitrary network calls, modify container runtime
- **AUDITED**: All tool invocations are logged with timestamps and parameters

### Data Protection
- Private keys encrypted at rest (AES-256-GCM)
- OIDC client secrets encrypted in database
- No secrets logged — tool results are sanitized
- Installer state avoids storing raw secrets, but it can contain sensitive operational metadata such as admin object IDs, resource names, and failure diagnostics
- Installer conversation content and tool results are sent to Anthropic; local transcript files remain local unless an operator explicitly shares them

### Network Boundaries
- Internal: SAO server API (localhost:3100), PostgreSQL (internal network)
- External: Anthropic API, Azure management APIs through `az`, Entra ID (login.microsoftonline.com), Graph API (graph.microsoft.com)
- Runtime database traffic: Azure Container Apps connects to Azure Database for PostgreSQL over TLS
- No other outbound connections are intended

## State Machine

```
[NO_USERS] → detect → [INSTALLER_MODE]
                           │
                    ┌──────┴──────┐
                    │  Load State │ (check for resumption)
                    └──────┬──────┘
                           │
                    ┌──────┴──────┐
                    │  Auth Admin │ → entra.authenticate_admin
                    └──────┬──────┘
                           │
                    ┌──────┴──────┐
                    │  Seed Admin │ → postgres.seed_admin
                    └──────┬──────┘
                           │
                    ┌──────┴──────┐
                    │  Gen Master │ → vault.generate_master_key
                    └──────┬──────┘
                           │
                    ┌──────┴──────┐
                    │  Init Vault │ → vault.initialize_encryption
                    └──────┬──────┘
                           │
                    ┌──────┴──────┐
                    │  Entra App  │ → entra.validate_app_registration
                    └──────┬──────┘
                           │
                    ┌──────┴──────┐
                    │  Graph API  │ → entra.discover_tenant
                    └──────┬──────┘
                           │
                    ┌──────┴──────┐
                    │  Validate   │ → vault.test_vault_operations
                    └──────┬──────┘
                           │
                    ┌──────┴──────┐
                    │  Complete   │ → mark operational
                    └──────┬──────┘
                           │
                    [OPERATIONAL_MODE]
```

Each step saves state to PostgreSQL. On resumption, the agent skips completed steps.
