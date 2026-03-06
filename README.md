# SAO - Secure Agent Orchestrator

Local-first control plane for agent identity, birth documents, vault bootstrap, and real-time agent status.

## Current Direction

SAO is currently being built as the Phase 1 foundation for the broader SAO + Orion vision.

The focus in this repo right now is:

- local Docker bootstrap with PostgreSQL
- agent birth flow that creates four signed documents: `soul.md`, `ethics.md`, `org-map.md`, and `personality.md`
- a hard guard that prevents `soul.md` from being modified after birth
- a basic agent WebSocket channel for heartbeat/status traffic
- a minimal Superego proposal stub that can suggest changes to ego-level behavior without touching `soul.md`

Target-state architecture notes in `documents/` and older ecosystem writeups describe later phases. This README describes the current implementation and immediate direction, not the full future platform.

## What Works Today

- Docker compose stack at `docker/docker-compose.yml`
- Axum server on `http://localhost:3100`
- PostgreSQL-backed control-plane state
- `POST /api/agents` birth flow returning a `READY` summary
- initial setup flow that provisions an SAO admin entity and seeds tracked bootstrap work items
- local identity directories under `SAO_DATA_DIR/identities/<agent_id>/`
- agent WebSocket endpoint at `ws://localhost:3100/ws/agent/<agent_id>`
- health, setup, auth, OIDC, admin, and vault server surfaces that support the control plane

## Birth Flow

Current birth flow behavior:

1. `POST /api/agents` creates a local identity entry.
2. SAO creates four birth documents in the agent directory.
3. Each document is stamped with a signature generated from the SAO master key.
4. `soul.md` is treated as the constitutional root and cannot be modified through the identity manager.
5. The API returns a minimal readiness response.

Current response shape:

```json
{
  "status": "READY",
  "documents": ["soul.md", "ethics.md", "org-map.md", "personality.md"],
  "soul_immutable": true
}
```

## Initial Setup

Current first-run setup behavior:

1. `POST /api/setup/initialize` seals the vault master key and creates the initial admin user.
2. SAO provisions an `sao_admin_entity` identity tied to the configured frontier model credential.
3. The setup flow seeds an initial work queue for getting SAO out of local Docker and into Azure container hosting.
4. The highest-priority work item is the AZForge-to-Azure-container IaC track, followed by durable state, database, secret, origin, and container release tasks.

Document roles:

- `soul.md`: immutable constitutional root
- `ethics.md`: ethical baseline document
- `org-map.md`: initial registry and placement metadata
- `personality.md`: evolvable ego/personality surface

## WebSocket Heartbeat

Agents connect on:

```text
ws://localhost:3100/ws/agent/<agent_id>
```

Current heartbeat behavior:

- agent sends raw text `heartbeat`
- SAO logs heartbeat receipt to the server console
- SAO replies with JSON status
- SAO calls a Superego stub that prints a personality tweak proposal

Current response shape:

```json
{
  "status": "ACTIVE",
  "last_heartbeat": "2026-03-05T20:00:00Z"
}
```

The Superego path is intentionally minimal at this stage. It does not patch files, does not touch `soul.md`, and does not yet persist lifecycle state.

## Local Development

Preferred development path: run everything locally in Docker.

```bash
docker-compose -f docker/docker-compose.yml up -d --build
curl http://localhost:3100/api/health
```

Expected health response:

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

Native `cargo run` is possible, but on Windows it currently depends on a working OpenSSL toolchain. Docker is the safer default dev path for this repo.

## Quick Verification

Create an agent:

```bash
curl -X POST http://localhost:3100/api/agents \
  -H "Content-Type: application/json" \
  -d '{"name":"TestAgent","type":"personal","pubkey":"dummy-ed25519-key"}'
```

Test heartbeat with `wscat`:

```bash
wscat -c ws://localhost:3100/ws/agent/test123
```

Then send:

```text
heartbeat
```

Expected server log lines:

```text
Agent test123 heartbeat received
Superego suggestion: Personality tweak proposal for test123: increase caution by 5% (based on roll-up)
```

## API Surface

Core endpoints for the current foundation:

| Method | Path | Purpose |
|--------|------|---------|
| `GET` | `/api/health` | Health check |
| `POST` | `/api/agents` | Create an agent and birth documents |
| `GET` | `/api/agents` | List agents |
| `GET` | `/api/agents/{id}` | Fetch one agent |
| `DELETE` | `/api/agents/{id}` | Delete one agent |
| `WS` | `/ws/agent/<agent_id>` | Agent real-time channel |

Additional control-plane routes already exist for setup, auth, OIDC, admin, vault, and ethical evaluation.

## Repository Layout

| Path | Purpose |
|------|---------|
| `crates/sao-core` | identity manager, master key handling, vault primitives, ethical bridge stubs |
| `crates/sao-server` | Axum API server, DB access, auth, WebSocket handling |
| `docker/` | local Docker build and compose stack |
| `docs/` | implementation notes and supporting repo docs |
| `documents/` | target-state architecture analysis and longer-form planning docs |

## Near-Term Direction

The next slices are expected to stay small, local, and testable:

- persist agent heartbeat and lifecycle state instead of logging only
- keep Superego proposals constrained to ego-level surfaces such as `personality.md`
- tighten the birth artifact and registry flow beyond the current minimal stub
- continue verifying every slice in local Docker before expanding scope

## License

MIT
