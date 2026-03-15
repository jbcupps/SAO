# SAO - Claude Code Project Guide

## Project Overview
SAO (Secure Agent Orchestrator) is the centralized key management and multi-agent orchestration platform. It provides a secure vault for all cryptographic keys (Ed25519 identity keys, API provider keys, GPG keys, OAuth tokens), agent identity management, and coordination. It does NOT contain agent-specific logic (that's in `abigail`).

**Core differentiator**: SAO's first-run experience is an **agentic conversation**, not a traditional setup wizard. A Claude-powered installer agent walks the admin through bootstrapping, configuring, and validating the entire platform — inside a containerized environment with full transparency into what's happening at the system level.

## Tech Stack
- **Language**: Rust (primary), TypeScript (frontend)
- **Backend**: Axum (REST + WebSocket)
- **Frontend**: React + TypeScript — hybrid of structured forms and a chat terminal
- **Database**: PostgreSQL 16 (via sqlx, compile-time checked queries)
- **Deployment**: Docker + docker-compose (all services containerized)
- **Auth**: Microsoft Entra ID (primary, OIDC), WebAuthn/FIDO2 (secondary/local fallback)
- **LLM**: Claude via Anthropic API (MVP installer agent engine; BYOK for additional providers later)

## Build & Test
```bash
# Development (local)
cargo build                    # Build all crates
cargo test                     # Run all tests
cargo run --bin sao-server     # Start the orchestration server
cargo clippy                   # Lint

# Docker (production)
docker compose -f docker/docker-compose.yml up --build
docker compose -f docker/docker-compose.yml down

# Frontend
cd frontend && npm install && npm run dev    # Dev server
cd frontend && npm run build                 # Production build (output to frontend/dist)
```

## Architecture Rules

### Separation of Concerns
- SAO manages keys, identities, and orchestration ONLY
- SAO does NOT run agent logic, LLM providers, or skills
- Agent-specific code belongs in the `abigail` repo
- Ethical evaluation logic belongs in `Ethical_AI_Reg`
- SAO bridges between agents and the ethical platform

### Crate Structure
- `sao-core`: Pure library crate with no server dependencies. Contains identity management, master key operations, vault logic, crypto primitives, and bridge types.
- `sao-server`: Binary crate with Axum server. Depends on sao-core. Contains routes, state, WebSocket handler, auth middleware, the agentic installer runtime, and serves the React frontend.
- `frontend/`: React + TypeScript SPA. Chat terminal + form hybrid UI. Built separately, output served as static files by sao-server.

### Database (PostgreSQL)
- All persistent data lives in PostgreSQL (users, key metadata, agent registry, audit logs, SSO config, installer state)
- Use sqlx with compile-time checked queries where possible
- Migrations live in `migrations/` directory (sqlx migrate)
- Encrypted key material is stored in the database, never plaintext secrets
- Docker-compose provisions the database automatically
- Connection via `DATABASE_URL` environment variable

### Docker-First Development
- All services run via docker-compose (SAO server, PostgreSQL, frontend build)
- Multi-stage Dockerfile for minimal production images
- Persistent volumes for PostgreSQL data and SAO vault data
- Health checks on all services
- Environment configuration via `.env` file (not committed to git)
- The installer agent runs inside the container — it can see and act on the containerized environment directly

---

## Authentication & Identity

### Entra ID — Primary (OIDC)
SAO is Entra-first. Human users authenticate via Microsoft Entra ID using standard Authorization Code flow with PKCE.

- The installer agent authenticates the first admin via Entra during bootstrap
- The admin's **Entra Object ID (OID)** becomes the seed identity in PostgreSQL — no generated throwaway credentials
- OIDC config (issuer, client_id, client_secret, scopes) stored encrypted in PostgreSQL
- Supports any Entra ID tenant; designed for enterprise integration
- Post-install role alignment uses **Microsoft Graph API** to discover and map organizational roles

### WebAuthn/FIDO2 — Secondary / Local Fallback
- Available as an additional auth factor or for offline/local-only scenarios
- Supports Windows Hello, fingerprint, PIN, security keys
- Not the bootstrap mechanism — Entra is

### Agent Authentication
- Agents authenticate via **Ed25519 signature** (unchanged from original design)
- Agent public keys verified against master key signatures
- OIDC/WebAuthn are for human users only

### Roles
- **User**: Manage own keys, manually store/retrieve secrets, register agents/hives, view audit logs for own resources
- **Administrator**: Configure SSO providers, manage persistent data connections, manage all users, view full audit logs, system configuration. The first admin is bootstrapped from the installer's Entra OID.

---

## Agentic Installer (First-Run Experience)

This is the defining UX of SAO. On first launch (no users in database), the system enters **installer mode** — a Claude-powered conversational agent that guides the admin through the entire bootstrap.

### Installer Flow
1. Container starts with no users in PostgreSQL → installer mode activates
2. Installer agent presents itself in the chat terminal
3. Admin authenticates via Entra ID (OIDC flow)
4. Installer seeds admin record using the authenticated Entra OID
5. Installer generates master Ed25519 signing key
6. Installer initializes vault encryption
7. Installer walks admin through Entra app registration (or validates existing one)
8. Installer uses Graph API to discover tenant structure, suggest role mappings
9. Installer optionally provisions test accounts, configures agent registrations
10. Installer validates the full stack (DB, vault, auth, Graph API connectivity)
11. System transitions from installer mode to operational mode

### Installer Design Principles
- **Conversational, not procedural**: The installer is a dialogue, not a checklist. It asks questions, explains what it's doing, handles errors gracefully, and adapts to the admin's environment.
- **Transparency**: The optional visible bash pane shows exactly what commands the agent is executing. Trust through visibility.
- **Claude MVP**: The installer agent uses Claude (Anthropic API) as its LLM engine. This is the minimum viable path — BYOK support for other LLM providers is a later milestone.
- **Idempotent**: The installer can be re-entered if bootstrap is interrupted. It detects existing state and resumes.
- **Contained**: Everything runs inside the Docker container. The installer doesn't reach outside the container boundary except for Entra auth and Graph API calls.

### Installer Agent Architecture
- The installer is a server-side agent loop running within `sao-server`
- It has scoped permissions: can write to PostgreSQL, generate keys, call Graph API, execute container-local commands
- It does NOT have access to the host filesystem outside mounted volumes
- Installer state is persisted in PostgreSQL so it survives container restarts
- Once bootstrap completes, the installer agent is deactivated (not destroyed — audit trail preserved)

---

## Frontend: Chat Terminal + Form Hybrid

The SAO frontend is NOT a traditional admin dashboard. It's a hybrid of structured form elements and a conversational chat terminal.

### UX Model
- **Chat terminal**: Primary interaction surface. The installer (and later, operational agents) communicate here. Markdown rendering, code blocks, progress indicators.
- **Structured forms**: For data entry that benefits from form validation — key names, OIDC configuration fields, agent registration parameters. Forms appear inline within the conversation flow or as slide-out panels.
- **Visible bash pane**: Optional panel showing real-time command execution. Can be toggled by the admin. Shows exactly what the agent is doing at the system level.
- **Post-install**: The chat terminal persists as the primary operational interface. Configuration changes, agent management, and troubleshooting happen conversationally — not through a settings maze.

### Frontend Structure
```
frontend/
  src/
    components/
      chat/              # Chat terminal, message rendering, input
      forms/             # Inline form components (key entry, OIDC config, etc.)
      bash/              # Visible bash pane (optional, toggleable)
      layout/            # Shell layout, nav, panels
    pages/
      Installer.tsx      # Installer mode (first-run)
      Dashboard.tsx      # Operational dashboard (post-install)
      KeyVault.tsx       # Key management UI
      AgentRegistry.tsx  # Agent/hive management
      AuditLog.tsx       # Audit log viewer
    hooks/               # Custom React hooks
    api/                 # API client functions
    auth/                # Entra OIDC + WebAuthn client logic
    types/               # TypeScript type definitions
```

---

## Key Management (Full Vault)
SAO manages ALL key types across the ecosystem:

| Key Type | Examples | Storage |
|----------|----------|---------|
| **Ed25519 Identity Keys** | Master key, agent signing keys | Encrypted in DB, master key also on filesystem |
| **API Provider Keys** | Anthropic, OpenAI, Google, GitHub tokens | Encrypted in DB |
| **GPG Keys** | Mentor signing keys, service keys | Encrypted in DB |
| **OAuth Tokens** | OIDC refresh tokens, service tokens | Encrypted in DB |

- All secrets encrypted at rest in PostgreSQL (AES-256-GCM, key derived from auth)
- Key metadata (name, type, created, last_used, expiry) stored alongside encrypted blobs
- Audit log for every key access, creation, rotation, and deletion
- Keys organized by owner (user or agent) with RBAC permissions

---

## Security

- Master key never leaves the SAO data directory
- All secrets encrypted at rest (AES-256-GCM with key derived from auth)
- Agent public keys verified against master key signatures
- All agent registration requires Ed25519 signature verification
- Never log secrets or key material
- Use SSRF validation for any URL inputs
- CSRF protection on all state-changing endpoints
- Rate limiting on auth endpoints
- Full audit logging for all sensitive operations
- Installer agent commands are scoped and audited — no unrestricted shell access

---

## Integration Protocol

- Agents connect via REST API or WebSocket
- SAO verifies agent identity (Ed25519) before accepting connections
- Agents retrieve their API keys from SAO vault at startup
- Ethical evaluations forwarded to `Ethical_AI_Reg` via REST
- WebSocket broadcasts for real-time event distribution
- "Hive" = a logical group of agents sharing a key set
- Graph API integration for Entra tenant discovery and role alignment

---

## API Structure

```
# Public (no auth)
GET  /api/health                            # Health check
GET  /api/setup/status                      # Returns: installer | operational
POST /api/auth/oidc/entra/login             # Redirect to Entra ID
GET  /api/auth/oidc/entra/callback          # Entra OIDC callback
POST /api/auth/webauthn/register/begin      # Start WebAuthn registration (secondary)
POST /api/auth/webauthn/register/complete   # Complete registration
POST /api/auth/webauthn/login/begin         # Start WebAuthn login
POST /api/auth/webauthn/login/complete      # Complete login

# Installer mode (active only during first-run, requires Entra auth)
WS   /ws/installer                          # Chat terminal WebSocket for installer agent
GET  /api/installer/state                   # Current installer progress
POST /api/installer/action                  # Structured form submissions during install

# Authenticated (user)
GET    /api/keys                            # List own keys
POST   /api/keys                            # Store a key
GET    /api/keys/:id                        # Retrieve a key (decrypted)
DELETE /api/keys/:id                        # Delete a key
PUT    /api/keys/:id                        # Update a key
GET    /api/agents                          # List registered agents
POST   /api/agents                          # Register an agent
GET    /api/audit                           # Own audit log
WS     /ws/chat                             # Operational chat terminal

# Authenticated (admin)
GET    /api/admin/users                     # Manage users
POST   /api/admin/sso                       # Configure SSO provider
GET    /api/admin/sso                       # List SSO configs
DELETE /api/admin/sso/:id                   # Remove SSO provider
GET    /api/admin/audit                     # Full audit log
POST   /api/admin/connections               # Configure data connections
POST   /api/admin/graph/discover            # Trigger Graph API tenant discovery
GET    /api/admin/graph/roles               # View discovered role mappings

# Agent API (Ed25519 auth)
POST   /api/agent/auth                      # Agent authenticates with signature
GET    /api/agent/keys                      # Agent retrieves its assigned keys
WS     /ws/agent/:agent_id                  # WebSocket for real-time events
```

---

## Test Environment
- **Tenant**: `jbcuppsgmail.onmicrosoft.com` (personal Entra ID tenant)
- Entra test accounts to be provisioned via installer agent during development
- Docker-compose includes a dev profile with hot-reload and debug logging

---

## Related Repos
- `abigail` — Agent implementation (Tauri desktop app)
- `Orion_Dock` — Orion agent platform (uses DPAPI-encrypted keys locally)
- `Ethical_AI_Reg` — Ethical alignment platform
- `Phoenix` — Coordination and project tracking
- `prometheus-bound` — Infrastructure (GPG signing, host vault)

---

## Key Decisions Log

| Decision | Detail |
|----------|--------|
| Installer model | Agentic conversation (Claude-powered), not a static wizard |
| Admin bootstrap | Seeded from installer's Entra OID — no generated credentials |
| Post-install config | Agent-guided via Graph API + conversational UI |
| Frontend UX | Chat terminal + structured forms hybrid, optional visible bash pane |
| Primary auth | Microsoft Entra ID (OIDC). WebAuthn is secondary/fallback. |
| LLM engine | Claude (Anthropic API) for MVP. BYOK for additional providers later. |
| Test tenant | `jbcuppsgmail.onmicrosoft.com` |
