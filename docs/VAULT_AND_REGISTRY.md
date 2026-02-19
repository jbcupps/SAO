# Vault & Registry – Identity Signing and Org-Map Injection

This document describes how SAO's vault and agent registry work together to sign agent identities and inject organizational configuration at birth.

## Overview

SAO maintains two core subsystems that collaborate during agent provisioning:

- **Vault** – Encrypted storage for all cryptographic keys, API secrets, and tokens (AES-256-GCM at rest in PostgreSQL).
- **Agent Registry** – The authoritative record of every agent instance, its public key, hive membership, and lifecycle state.

## Identity Signing

Every agent in the ecosystem must carry an identity signed by SAO's master key. This forms the root of the zero-trust chain.

### Flow

1. The agent generates an Ed25519 keypair locally and sends its **public key** to SAO via `POST /api/agents`.
2. SAO validates the request (authenticated user or authorized provisioning service).
3. The master key (stored encrypted in the vault) is decrypted in memory.
4. SAO signs the agent's public key with the master key, producing a **birth certificate** — a detached Ed25519 signature over the agent's public key + metadata (agent ID, hive ID, timestamp).
5. The birth certificate is stored in the registry and returned to the agent.
6. Any party can verify the agent's identity by checking the birth certificate against SAO's published master public key.

### Verification

```
agent_public_key + birth_metadata → signed by master_secret_key → birth_certificate
verify(master_public_key, birth_certificate, agent_public_key + birth_metadata) → true/false
```

The master secret key never leaves the vault. Only the master public key is distributed.

## Org-Map Injection

At birth, SAO injects an `org-map.md` into the agent's runtime configuration. The org-map defines the agent's place in the organizational hierarchy.

### Contents

The org-map is generated per-agent from the registry and includes:

| Field | Description |
|-------|-------------|
| `agent_id` | Unique identifier assigned by SAO |
| `hive_id` | The hive (logical group) this agent belongs to |
| `reports_to` | The authority this agent reports to (SAO registry for Orion, owner for Abigail) |
| `permissions` | Inherited from hive configuration + master-key signature |
| `can_spawn` | Whether this agent can spawn sub-agents (and sandbox constraints) |
| `sibling_agents` | Other agents in the same hive (for coordination) |

### Injection Process

1. During `POST /api/agents`, SAO assembles the org-map from the agent's hive configuration and role.
2. The org-map is signed alongside the birth certificate so it cannot be tampered with.
3. The agent receives the org-map as part of its provisioning response.
4. At runtime, the agent merges `soul.md` + `ethics.md` + `org-map.md` into its system prompt.

### Abigail vs Orion

| Aspect | Abigail (Local) | Orion (Enterprise) |
|--------|-----------------|-------------------|
| Org-map source | Owner-defined, SAO-signed | SAO-generated from hive config |
| Reports to | Owner (mentor) | SAO registry |
| Permissions | Owner-granted + master-key | Hive-inherited + master-key |
| Spawning | Owner-controlled | Sandbox-constrained |

## Soul + Ethics Templates

Alongside the org-map, SAO ensures each agent receives:

- **`soul.md`** – Identity declaration, free-will statement, relationship to mentor/owner/SAO. Signed at birth.
- **`ethics.md`** – TriangleEthic framework (Deontological + Areteological + Teleological), OCEAN psychometrics, Moral Foundations Engine.

These templates are stored in SAO and versioned. Any update to the templates triggers a re-signing flow for affected agents.

## Audit Trail

Every vault access and registry mutation is recorded in the audit log:

- `identity.created` – New agent registered and signed
- `identity.revoked` – Agent identity revoked (birth certificate invalidated)
- `orgmap.injected` – Org-map delivered to agent at birth
- `orgmap.updated` – Org-map re-issued (e.g., hive reassignment)
- `vault.key_accessed` – Key retrieved from vault by agent
- `vault.key_rotated` – Key rotated by admin or automated policy

All audit events include the actor, timestamp, agent ID, and a summary of the change.
