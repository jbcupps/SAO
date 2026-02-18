# SAO - Claude Code Project Guide

## Project Overview
SAO (Secure Agent Orchestrator) is the centralized key management and multi-agent orchestration platform. It provides a secure vault for all cryptographic keys (Ed25519 identity keys, API provider keys, GPG keys, OAuth tokens), agent identity management, and coordination. It does NOT contain agent-specific logic (that's in abigail).

## Tech Stack
- **Language**: Rust (primary), TypeScript (frontend)
- **Backend**: Axum (REST + WebSocket)
- **Frontend**: React + TypeScript (served as static files by Axum)
- **Database**: PostgreSQL 16 (via sqlx, compile-time checked queries)
- **Deployment**: Docker + docker-compose (all services containerized)
- **Auth**: WebAuthn/FIDO2 (local auth, Windows Hello), OIDC (SSO)

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
- `sao-server`: Binary crate with Axum server. Depends on sao-core. Contains routes, state, WebSocket handler, auth middleware, and serves the React frontend.
- `frontend/`: React + TypeScript SPA. Built separately, output served as static files by sao-server.

### Database (PostgreSQL)
- All persistent data lives in PostgreSQL (users, key metadata, agent registry, audit logs, SSO config)
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

### Key Management (Full Vault)
SAO manages ALL key types across the ecosystem:

| Key Type | Examples | Storage |
|----------|----------|---------|
| **Ed25519 Identity Keys** | Master key, agent signing keys | Encrypted in DB, master key also on filesystem |
| **API Provider Keys** | OpenAI, Anthropic, Google, GitHub tokens | Encrypted in DB |
| **GPG Keys** | Mentor signing keys, service keys | Encrypted in DB |
| **OAuth Tokens** | OIDC refresh tokens, service tokens | Encrypted in DB |

- All secrets encrypted at rest in PostgreSQL (using vault encryption key derived from WebAuthn or passphrase)
- Key metadata (name, type, created, last_used, expiry) stored alongside encrypted blobs
- Audit log for every key access, creation, rotation, and deletion
- Keys organized by owner (user or agent) with RBAC permissions

### Authentication & Authorization

#### Local Auth (WebAuthn / FIDO2) - Primary
- First-run setup wizard orchestrates initial credential registration
- Browser calls Windows Hello (or any FIDO2 authenticator) via WebAuthn API
- Server stores credential IDs and public keys in PostgreSQL
- WebAuthn used to unlock the vault and authenticate sessions
- Supports fingerprint, PIN, security keys
- Session tokens (JWT) issued after successful WebAuthn ceremony

#### SSO (OIDC) - Enterprise
- Admin-configurable OIDC providers (Entra ID, Auth0, Google, etc.)
- Standard Authorization Code flow with PKCE
- OIDC config stored in PostgreSQL (issuer, client_id, client_secret, scopes)
- User accounts linked to OIDC identities
- Agents still authenticate via Ed25519 (OIDC is for human users only)

#### Roles
- **User**: Manage own keys, manually store/retrieve secrets, register agents/hives, view audit logs for own resources
- **Administrator**: Configure SSO providers, manage persistent data connections, manage all users, view full audit logs, system configuration

### First-Run Setup
On first launch (no users in database):
1. Redirect to setup wizard
2. Create admin account with WebAuthn credential registration
3. Generate master Ed25519 signing key
4. Initialize vault encryption
5. Optionally configure SSO provider
6. Redirect to dashboard

### Security
- Master key never leaves the SAO data directory
- All secrets encrypted at rest (AES-256-GCM with key derived from auth)
- Agent public keys are verified against master key signatures
- All agent registration requires Ed25519 signature verification
- Never log secrets or key material
- Use the SSRF validation pattern from abigail for any URL inputs
- WebAuthn challenge-response prevents replay attacks
- CSRF protection on all state-changing endpoints
- Rate limiting on auth endpoints
- Audit logging for all sensitive operations

### Integration Protocol
- Agents connect via REST API or WebSocket
- SAO verifies agent identity before accepting connections
- Agents retrieve their API keys from SAO vault at startup
- Ethical evaluations are forwarded to Ethical_AI_Reg via REST
- WebSocket broadcasts for real-time event distribution
- "Hive" = a logical group of agents sharing a key set

### API Structure
```
# Public (no auth)
POST /api/auth/webauthn/register/begin     # Start WebAuthn registration
POST /api/auth/webauthn/register/complete   # Complete registration
POST /api/auth/webauthn/login/begin         # Start WebAuthn login
POST /api/auth/webauthn/login/complete      # Complete login
GET  /api/auth/oidc/:provider/login         # Redirect to OIDC provider
GET  /api/auth/oidc/:provider/callback      # OIDC callback
GET  /api/health                            # Health check
GET  /api/setup/status                      # First-run check

# Authenticated (user)
GET    /api/keys                            # List own keys
POST   /api/keys                            # Store a key
GET    /api/keys/:id                        # Retrieve a key (decrypted)
DELETE /api/keys/:id                        # Delete a key
PUT    /api/keys/:id                        # Update a key
GET    /api/agents                          # List registered agents
POST   /api/agents                          # Register an agent
GET    /api/audit                           # Own audit log

# Authenticated (admin)
GET    /api/admin/users                     # Manage users
POST   /api/admin/sso                       # Configure SSO provider
GET    /api/admin/sso                       # List SSO configs
DELETE /api/admin/sso/:id                   # Remove SSO provider
GET    /api/admin/audit                     # Full audit log
POST   /api/admin/connections               # Configure data connections

# Agent API (Ed25519 auth)
POST   /api/agent/auth                      # Agent authenticates with signature
GET    /api/agent/keys                      # Agent retrieves its assigned keys
WS     /ws/agent/:agent_id                  # WebSocket for real-time events
```

## Frontend Structure (React + TypeScript)
```
frontend/
  src/
    components/           # Reusable UI components
    pages/                # Route-level page components
      Dashboard.tsx
      KeyVault.tsx        # Key management UI
      AgentRegistry.tsx   # Agent/hive management
      SetupWizard.tsx     # First-run setup
      AdminSSO.tsx        # SSO configuration (admin)
      AdminUsers.tsx      # User management (admin)
      AuditLog.tsx        # Audit log viewer
    hooks/                # Custom React hooks
    api/                  # API client functions
    auth/                 # WebAuthn + OIDC client logic
    types/                # TypeScript type definitions
```

## Related Repos
- `abigail` - Agent implementation (Tauri desktop app)
- `Orion_Dock` - Orion agent platform (uses DPAPI-encrypted keys locally)
- `Ethical_AI_Reg` - Ethical alignment platform
- `Phoenix` - Coordination and project tracking
- `prometheus-bound` - Infrastructure (GPG signing, host vault)
