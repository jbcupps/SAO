# OrionII + SAO MVP Contract

This document is the shared local MVP contract for wiring `C:\Repo\OrionII` to `C:\Repo\SAO`.
SAO owns the control plane: identity, secrets, agent lifecycle, LLM key custody, and audit. OrionII
owns the durable local desktop runtime: identity continuity, document indexing, and egress to SAO.

## MVP Scope

- A signed-in SAO admin configures LLM provider keys (OpenAI / Anthropic / Ollama) once.
- A signed-in SAO user creates an OrionII entity with a chosen provider + model.
- The user downloads a bundle (`config.json` + `OrionII-Setup.msi`) from SAO.
- The downloaded entity adopts the SAO-assigned identity, phones home for policy, ships egress
  events, and routes all model calls through SAO's LLM proxy.
- Browser setup remains installer-led; production bootstrap is the Azure conversational installer.
- Local Docker MVP uses `sao-server bootstrap-local`.

## Endpoints

All endpoints are under the SAO API root.

### `GET /api/orion/policy` *(entity bearer or user JWT)*

Returns the current policy overlay for OrionII.

```json
{
  "version": 1,
  "source": "sao",
  "rules": [
    "Only ship sanitized Orion egress events.",
    "Preserve correlation IDs on audit events."
  ],
  "updatedAt": "2026-04-25T00:00:00Z"
}
```

### `POST /api/orion/egress` *(entity bearer or user JWT)*

Accepts a batch of pending OrionII events. SAO trusts the entity bearer's `agent_id` claim over
any `agentId` in the body.

```json
{
  "agentId": "optional-sao-agent-id",
  "orionId": "00000000-0000-0000-0000-000000000000",
  "clientVersion": "0.1.0",
  "events": [
    {
      "id": "00000000-0000-0000-0000-000000000001",
      "enqueuedAt": "2026-04-25T00:00:00Z",
      "attempts": 1,
      "event": {
        "auditAction": {
          "action": "open local document",
          "correlationId": "00000000-0000-0000-0000-000000000002"
        }
      }
    }
  ]
}
```

Returns per-event status. Duplicate event ids ack idempotently.

```json
{
  "accepted": 1,
  "duplicate": 0,
  "failed": 0,
  "results": [
    { "id": "00000000-0000-0000-0000-000000000001", "status": "acked" }
  ]
}
```

### `POST /api/llm/generate` *(entity bearer ONLY)*

OrionII calls this for every Id/Ego prompt — **regardless of which provider the entity was
created against**. There is no direct entity → provider call path; SAO is always in the middle so
keys never leave the server, every call is auditable, and revoking an entity token immediately
cuts off model access. SAO holds the provider keys and forwards to the configured upstream
(OpenAI / Anthropic / Grok / Gemini / Ollama).

Request:
```json
{
  "provider": "ollama",
  "model": "llama3.2",
  "system": "You are Orion's Ego layer...",
  "prompt": "User query: ...",
  "temperature": 0.2,
  "role": "ego"
}
```

Response:
```json
{
  "text": "...",
  "model": "llama3.2",
  "latencyMs": 873
}
```

Errors:
| Status | Cause |
|---|---|
| 400 | Provider not enabled / not registered / model not on the approved list. |
| 401 | Bearer is not a valid (non-revoked) entity JWT. |
| 502 | Provider call failed (network, bad response, upstream error). |
| 503 | Vault is sealed. |

### `GET /api/agents/:id/bundle` *(user session, owner or admin)*

Mints a fresh entity JWT, revokes prior tokens for the same agent, packages a ZIP:

```
config.json            -- sao_base_url, agent_id, agent_token (JWT), default models, client_version_min
OrionII-Setup.msi      -- Tauri installer (read from SAO_ORION_INSTALLER_PATH inside the container)
README-FIRST-RUN.txt   -- install steps
```

Returns 503 if the installer is not staged with a clear remediation message.

### `GET /api/agents/:id/events` *(user session, owner or admin)*

Lists `orion_egress_events` rows for the agent. Pagination via `?limit=&offset=`. Backs the
`/agents/:id/events` page in the SAO UI.

### Admin LLM provider management

Supported providers: `openai`, `anthropic`, `grok` (xAI), `gemini` (Google), `ollama` (local).
The first four are key-bearing (API key stored in vault); Ollama is base-URL-only.

| Method | Path | Notes |
|---|---|---|
| GET | `/api/admin/llm-providers` | Returns provider settings + `has_api_key` (no secret material). |
| PUT | `/api/admin/llm-providers/:provider` | Upserts settings; if `api_key` is in the body, replaces the encrypted vault entry at `(provider=<name>, label='api_key', secret_type='api_key')`. Vault must be unsealed. Validates that `ollama` does not receive an `api_key` and key-bearing providers do not receive a meaningless `base_url`. |
| POST | `/api/admin/llm-providers/ollama/probe` | Body `{ "base_url": "..." }`. Calls `GET /api/tags`, returns the live model list. |
| POST | `/api/admin/llm-providers/:provider/test` | Body `{ "model": "..." }` (defaults to `default_model`). Sends a tiny ping prompt through the real provider call path (key vault → upstream API). Returns `{ ok, model, latency_ms, preview, error? }`. Bypasses the approved-models gate so admins can probe new models before adding them to the allowlist. |

### Per-provider key formats

| Provider | Key shape | Console |
|---|---|---|
| `openai` | `sk-...` | https://platform.openai.com/api-keys |
| `anthropic` | `sk-ant-...` | https://console.anthropic.com/settings/keys |
| `grok` | `xai-...` | https://console.x.ai/ |
| `gemini` | `AIza...` | https://aistudio.google.com/apikey |
| `ollama` | (no key — base URL only) | n/a |

## Authentication

Three identities, three token shapes:

1. **Browser users** — Cookie-based session, CSRF-protected. Used at `/admin/*`, `/agents`, `/vault`.
2. **Local dev bearer** — User JWT minted by `sao-server mint-dev-token`. Sent as
   `Authorization: Bearer <token>` for the legacy OrionII env-driven path.
3. **Entity JWT** — Long-lived, OIDC-shaped, minted at bundle download. Each token is a JWT with:

   ```json
   {
     "iss": "sao",
     "aud": "sao-api",
     "sub": "<agent_id>",
     "jti": "<agent_tokens.id>",
     "iat": ..., "nbf": ..., "exp": ...,
     "principalType": "non_human",
     "entityKind": "orion",
     "entityName": "abigail",
     "humanOwner": "<user_id>",
     "scope": "orion:policy orion:egress llm:generate"
   }
   ```

   Validation: HS256 signature against `SAO_JWT_SECRET`, plus a row check on `agent_tokens.id =
   jti` (rejects revoked / unknown / expired). The shape is intentionally compatible with future
   issuance from Entra or another external IdP — only the issuer/verifier swap, not the on-the-
   wire contract.

   Revocation: deleting an agent bulk-revokes all of its tokens. Re-downloading a bundle revokes
   any prior tokens for that agent (one live bundle per agent).

## CSRF Boundary

`/api/orion/*` and `/api/llm/generate` are machine-client route groups. They accept Bearer auth
and bypass browser CSRF. The general browser API keeps CSRF and origin enforcement intact.

## Idempotency

`SaoEgressRecord.id` is the idempotency key. SAO stores accepted ids in `orion_egress_events`.
Duplicate ids ack as `duplicate` rather than `failed`. OrionII only marks records acked when SAO
returns `acked` or `duplicate`.

## Acceptance Criteria

- Admin can configure an Ollama base URL + approved models via `/admin/llm-providers`.
- A user can create an agent with a default provider + Id/Ego model.
- Bundle download produces a ZIP whose `config.json` decodes a JWT with the entity claim shape
  above.
- OrionII installed from the bundle phones home, pulls policy, and chats through `/api/llm/generate`.
- Per-agent events page shows ack'd egress within 5 seconds.
- Deleting an agent revokes its tokens; the next OrionII call returns 401.
