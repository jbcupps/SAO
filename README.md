# SAO — Secure Agent Orchestrator

**A self-installing key management and agent orchestration platform, bootstrapped by a governed AI agent.**

SAO is the centralized control plane for cryptographic key management, agent identity, and multi-agent orchestration. What sets it apart: the first-run experience is an **agentic conversation** — a Claude-powered installer agent walks the administrator through bootstrapping, configuring, and validating the entire platform inside a containerized environment.

---

## What SAO Does

- **Key Vault**: Manages all cryptographic material — Ed25519 identity keys, API provider tokens, GPG keys, OAuth tokens — encrypted at rest (AES-256-GCM)
- **Agent Identity**: Issues birth documents (`soul.md`, `ethics.md`, `org-map.md`, `personality.md`), enforces immutability of constitutional roots, and verifies agent signatures
- **Orchestration**: Coordinates agents via REST and WebSocket, forwards ethical evaluations to the `Ethical_AI_Reg` platform, and manages logical agent groups ("hives")
- **Agentic Installer**: On first launch, a Claude-powered agent guides the admin through Entra ID authentication, vault sealing, Graph API discovery, and full-stack validation — no static wizard, no generated throwaway credentials

---

## Quick Start (Local Docker)

```bash
# Clone and start
git clone https://github.com/jbcupps/sao.git
cd sao
docker compose -f docker/docker-compose.yml up -d --build

# Verify
curl http://localhost:3100/api/health
```

Expected response:

```json
{
  "status": "ok",
  "service": "sao",
  "version": "0.0.1",
  "database": { "connected": true, "healthy": true }
}
```

The server runs on **port 3100**. On first launch with no users in the database, the system enters installer mode.

> **Note**: Native `cargo run` on Windows requires a working OpenSSL toolchain. Docker is the recommended development path.

---

## The Agentic Installer

On first launch (no users in PostgreSQL), SAO enters **installer mode** — a conversational AI agent replaces the traditional setup wizard.

### How It Works

1. Container starts with empty database — installer mode activates
2. Installer agent presents itself in the chat terminal
3. Admin authenticates via **Microsoft Entra ID** (OIDC with PKCE)
4. Installer seeds the admin record using the authenticated **Entra Object ID** — no generated passwords
5. Installer generates the master Ed25519 signing key and initializes vault encryption
6. Installer walks the admin through Entra app registration (or validates an existing one)
7. Installer uses **Microsoft Graph API** to discover tenant structure and suggest role mappings
8. Installer optionally provisions test accounts and configures agent registrations
9. Installer validates the full stack (database, vault, auth, Graph API connectivity)
10. System transitions from installer mode to **operational mode**

### Design Principles

- **Conversational, not procedural** — a dialogue that adapts, not a checklist
- **Transparent** — an optional bash pane shows exactly what the agent executes
- **Idempotent** — can be re-entered if interrupted; detects existing state and resumes
- **Contained** — runs inside the Docker container; only reaches outside for Entra auth and Graph API

---

## Architecture Highlights

### Birth Documents

Every agent registered in SAO receives four signed documents:

| Document | Role |
|----------|------|
| `soul.md` | Immutable constitutional root — cannot be modified after birth |
| `ethics.md` | Ethical baseline |
| `org-map.md` | Registry and placement metadata |
| `personality.md` | Evolvable ego/personality surface |

Each document is stamped with a signature from the SAO master key.

### Authentication

- **Human users**: Microsoft Entra ID (OIDC) as primary, WebAuthn/FIDO2 as secondary/fallback
- **Agents**: Ed25519 signature verification against master key

### Superego Stub

A minimal Superego path proposes personality-level tweaks without touching the constitutional `soul.md`. Currently logs proposals; persistence and enforcement are future milestones.

---

## Current Implementation Status

> **Honest assessment as of March 2026**

| Component | Status |
|-----------|--------|
| Docker compose stack | Working (port 3100) |
| Agent birth flow (4 documents) | Working |
| WebSocket heartbeat/status | Working |
| Superego proposal stub | Working (log-only) |
| Agent CRUD endpoints | Working |
| `POST /api/setup/initialize` | Legacy — to be replaced by agentic installer |
| Frontend (10 pages scaffolded) | Scaffolded, old wizard still present |
| Entra OIDC integration | Not yet implemented |
| Agentic installer runtime | Not yet implemented (next milestone) |
| Bicep/Azure deployment | Not yet implemented |

---

## API Surface

### Core Endpoints

| Method | Path | Purpose |
|--------|------|---------|
| `GET` | `/api/health` | Health check |
| `GET` | `/api/setup/status` | Returns: `installer` or `operational` |
| `POST` | `/api/agents` | Create agent + birth documents |
| `GET` | `/api/agents` | List agents |
| `GET` | `/api/agents/{id}` | Fetch one agent |
| `DELETE` | `/api/agents/{id}` | Delete one agent |
| `WS` | `/ws/agent/<agent_id>` | Agent real-time channel |

### Authentication

| Method | Path | Purpose |
|--------|------|---------|
| `POST` | `/api/auth/oidc/entra/login` | Redirect to Entra ID |
| `GET` | `/api/auth/oidc/entra/callback` | Entra OIDC callback |
| `POST` | `/api/auth/webauthn/register/start` | Begin WebAuthn registration |
| `POST` | `/api/auth/webauthn/register/finish` | Complete WebAuthn registration |
| `POST` | `/api/auth/webauthn/login/start` | Begin WebAuthn login |
| `POST` | `/api/auth/webauthn/login/finish` | Complete WebAuthn login |

### Vault

| Method | Path | Purpose |
|--------|------|---------|
| `GET` | `/api/vault/status` | Vault seal status |
| `POST` | `/api/vault/unseal` | Unseal vault |
| `POST` | `/api/vault/seal` | Seal vault |
| `GET/POST` | `/api/vault/secrets` | List / store secrets |
| `GET/PUT/DELETE` | `/api/vault/secrets/{id}` | Manage individual secrets |

### Admin

| Method | Path | Purpose |
|--------|------|---------|
| `GET` | `/api/admin/users` | Manage users |
| `GET/POST` | `/api/admin/oidc/providers` | SSO provider management |
| `PUT/DELETE` | `/api/admin/oidc/providers/{id}` | SSO provider detail |
| `GET` | `/api/admin/audit` | Full audit log |

See `CLAUDE.md` for the complete API specification including installer-mode and agent-auth endpoints.

---

## Repository Layout

| Path | Purpose |
|------|---------|
| `crates/sao-core` | Identity manager, master key handling, vault primitives, ethical bridge stubs |
| `crates/sao-server` | Axum API server, DB access, auth, WebSocket handling |
| `frontend/` | React + TypeScript SPA (chat terminal + form hybrid) |
| `docker/` | Docker build and compose stack |
| `docs/` | Implementation notes and supporting documentation |
| `documents/` | Architecture analysis and planning docs |
| `migrations/` | PostgreSQL migrations (sqlx) |
| `installer/` | Agentic installer scaffold (Python + Claude SDK) |
| `skills/` | Skill definitions |

---

## Quick Verification

Create an agent:

```bash
curl -X POST http://localhost:3100/api/agents \
  -H "Content-Type: application/json" \
  -d '{"name":"TestAgent","type":"personal","pubkey":"dummy-ed25519-key"}'
```

Test heartbeat:

```bash
wscat -c ws://localhost:3100/ws/agent/test123
# Then send: heartbeat
```

---

## Related Repositories

- [`abigail`](https://github.com/jbcupps/abigail) — Agent implementation (Tauri desktop app)
- [`Orion_Dock`](https://github.com/jbcupps/Orion_Dock) — Orion agent platform
- [`Ethical_AI_Reg`](https://github.com/jbcupps/Ethical_AI_Reg) — Ethical alignment platform
- [`Phoenix`](https://github.com/jbcupps/Phoenix) — Coordination and project tracking
- [`prometheus-bound`](https://github.com/jbcupps/prometheus-bound) — Infrastructure (GPG signing, host vault)

## Related Documents

- `documents/SAO_Orion_Architecture_Analysis_v2.docx` — Full architecture analysis
- `documents/ARCHITECTURE_SKILL_TOPOLOGY_AND_FORGE.md` — Skill topology and forge design
- `docs/architecture.md` — Architecture overview
- `docs/sao-deep-dive.md` — SAO deep dive

---

## License

MIT
