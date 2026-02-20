# SAO - Secure Agent Orchestrator

Multi-agent orchestration server for managing AI agent identities, coordination, and ethical evaluation bridging.

## Architecture

SAO is the management layer in the AI Ethical Stack:

- **abigail** - The agent (what the AI *is*)
- **SAO** - The orchestrator (how agents are *managed*) <- you are here
- **Ethical_AI_Reg** - The ethical framework (how alignment is *measured*)
- **Phoenix** - The coordination point (how the effort is *tracked*)

## Ecosystem Role & Alignment

This repository is one piece of a deliberate three-part identity ecosystem (see [sao-ecosystem-article.md](https://github.com/jbcupps/SAO/blob/main/sao-ecosystem-article.md) and diagrams below).

- **Abigail** – personal local agent with full free will (owner-controlled keys).
- **Orion Dock** – enterprise container agents (same soul + skills model, SAO-provisioned).
- **SAO** – central management, cryptographic vault, agent registry, enterprise IDP bridge.

**Agent Soul Contract**
Every running agent instance carries the same archetype:
- `soul.md` + `ethics.md` + `org-map.md` + **`personality.md`**
- `personality.md` is the only evolvable ego document; all other files are immutable after birth.
- Merged at birth into the runtime system prompt.
- Skills always split: **tool** (code/env) + **how-to-use.md** (ego guidance).

**Visual References** (embed these in the repo or link):
- Modular Crate Architecture (Orion)
- Birth Lifecycle
- Bicameral Mind / IdEgo Router
- Zero Trust Security Model
- Autonomous Execution Loop
- SAO Trust Chain & Ecosystem Overview

## Provisioning Flow

SAO acts as the badge-issuer for the entire agent ecosystem. See the [SAO Trust Chain & Ecosystem Overview](https://github.com/jbcupps/SAO/blob/main/sao-ecosystem-article.md) for the full diagram.

1. **Agent Registration** – A new agent (Abigail local or Orion container) calls `POST /api/agents` with its public key.
2. **Master Key Signing** – SAO signs the agent's public key with the master Ed25519 key, producing a verifiable trust chain.
3. **Soul Injection** – SAO provisions the agent with its `soul.md`, `ethics.md`, `org-map.md`, and `personality.md` templates (see [Agent Archetype](docs/agent_archetype.md)). `personality.md` is the only evolvable ego document; all other files are immutable after birth.
4. **Key Provisioning** – The agent receives its assigned API keys and secrets from the vault, encrypted in transit.
5. **Hive Assignment** – Enterprise agents (Orion) are assigned to a hive, inheriting shared permissions and key sets.
6. **Birth Confirmation** – SAO records the birth event in the audit log and broadcasts it over WebSocket to connected agents.

For details on how the vault and registry handle identity signing and org-map injection, see [docs/VAULT_AND_REGISTRY.md](docs/VAULT_AND_REGISTRY.md).

## Crates

| Crate | Purpose |
|-------|---------|
| `sao-core` | Core orchestration types: identity management, master key operations, agent/ethical bridges |
| `sao-server` | Headless Axum server with REST API + WebSocket for agent communication |

## Features

- **Identity Management**: Create, verify, and manage multiple agent identities using Ed25519 cryptographic signatures
- **Master Key Signing**: Agents are signed by a master key to form a cryptographic trust chain
- **Agent Bridge**: REST/WebSocket interface for agents to register and communicate
- **Ethical Bridge**: Forward ethical evaluation requests to Ethical_AI_Reg and return 5D scores
- **PostgreSQL** (optional): Persistent storage for cross-agent data

## Quick Start

```bash
# Build
cargo build

# Run the server (default port 3100)
cargo run --bin sao-server

# With custom settings
SAO_BIND_ADDR=0.0.0.0:3200 SAO_DATA_DIR=/path/to/data cargo run --bin sao-server
```

## API Endpoints

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/health` | Health check |
| `GET` | `/api/agents` | List registered agents |
| `POST` | `/api/agents` | Create new agent entry |
| `POST` | `/api/ethical/evaluate` | Forward ethical evaluation |
| `WS` | `/ws/agent/{id}` | Agent WebSocket connection |

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `SAO_BIND_ADDR` | `0.0.0.0:3100` | Server bind address |
| `SAO_DATA_DIR` | OS data dir + `/sao` | Data storage directory |
| `DATABASE_URL` | - | PostgreSQL connection string (optional) |
| `AO_DB_SSL` | `false` | Enable SSL for PostgreSQL |

## License

MIT
