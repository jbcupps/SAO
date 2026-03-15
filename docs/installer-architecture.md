# SAO Installer Architecture

## Conversational Flow

The installer is a multi-turn conversation between the admin and a Claude-powered agent. The agent uses tool calls to perform bootstrap actions, explains each step, and handles errors gracefully.

### Conversation Structure

```
┌─────────────────────────────────────────────┐
│              Frontend (Browser)              │
│  ┌───────────────────────────────────────┐  │
│  │         Chat Terminal (React)         │  │
│  │  - Renders agent messages (Markdown)  │  │
│  │  - Accepts admin input               │  │
│  │  - Shows inline forms when needed    │  │
│  └──────────────┬────────────────────────┘  │
│  ┌──────────────┴────────────────────────┐  │
│  │    Bash Pane (optional, toggleable)   │  │
│  │  - Shows tool execution in real-time  │  │
│  └───────────────────────────────────────┘  │
└──────────────────┬──────────────────────────┘
                   │ WebSocket (/ws/installer)
┌──────────────────┴──────────────────────────┐
│             SAO Server (Axum)                │
│  ┌───────────────────────────────────────┐  │
│  │        Installer Agent Runtime        │  │
│  │  - Manages conversation state         │  │
│  │  - Dispatches tool calls              │  │
│  │  - Persists progress to PostgreSQL    │  │
│  └──────────────┬────────────────────────┘  │
│                 │                            │
│  ┌──────────────┴────────────────────────┐  │
│  │          Claude API (Anthropic)       │  │
│  │  - System prompt + conversation       │  │
│  │  - Tool definitions                   │  │
│  │  - Multi-turn with tool results       │  │
│  └───────────────────────────────────────┘  │
└──────────────────┬──────────────────────────┘
                   │
        ┌──────────┼──────────┐
        │          │          │
   PostgreSQL   Entra ID   Graph API
```

### Message Flow (Single Turn)

1. Admin types message in chat terminal
2. Frontend sends message via WebSocket to SAO server
3. Server appends message to conversation history
4. Server sends conversation + tools to Claude API
5. Claude responds with text and/or tool_use blocks
6. Server executes tool calls, collects results
7. Server sends tool results back to Claude (if tool_use)
8. Claude produces final text response
9. Server streams response to frontend via WebSocket
10. Frontend renders response in chat terminal

The browser shell is local to the operator, but the installer conversation and tool results are sent to Anthropic for model execution. If an operator saves a transcript to a local file, that file remains local until they explicitly share it elsewhere.

### Tool Execution Visibility

When the agent executes a tool:
- The chat terminal shows a "thinking" indicator
- The bash pane (if open) shows the tool name, parameters, and result
- Tool execution is logged to the audit table with timestamps

## Tool Definitions (Claude API Format)

Tools are defined in the Anthropic tool-use format and passed to each Claude API call:

### `entra_authenticate_admin`
```json
{
  "name": "entra_authenticate_admin",
  "description": "Initiate OIDC authentication for the admin via Microsoft Entra ID",
  "input_schema": {
    "type": "object",
    "properties": {
      "tenant_id": {
        "type": "string",
        "description": "Entra ID tenant ID or domain"
      }
    },
    "required": ["tenant_id"]
  }
}
```

### `entra_validate_app_registration`
```json
{
  "name": "entra_validate_app_registration",
  "description": "Validate an existing Entra app registration has correct SAO config",
  "input_schema": {
    "type": "object",
    "properties": {
      "client_id": { "type": "string" },
      "tenant_id": { "type": "string" }
    },
    "required": ["client_id", "tenant_id"]
  }
}
```

### `entra_discover_tenant`
```json
{
  "name": "entra_discover_tenant",
  "description": "Query Graph API to discover tenant structure (groups, roles, users)",
  "input_schema": {
    "type": "object",
    "properties": {
      "access_token": { "type": "string" }
    },
    "required": ["access_token"]
  }
}
```

### `vault_generate_master_key`
```json
{
  "name": "vault_generate_master_key",
  "description": "Generate the master Ed25519 signing key for SAO",
  "input_schema": { "type": "object", "properties": {} }
}
```

### `vault_initialize_encryption`
```json
{
  "name": "vault_initialize_encryption",
  "description": "Initialize AES-256-GCM vault encryption",
  "input_schema": {
    "type": "object",
    "properties": {
      "admin_oid": { "type": "string" }
    },
    "required": ["admin_oid"]
  }
}
```

### `vault_test_operations`
```json
{
  "name": "vault_test_operations",
  "description": "Test vault seal/unseal and secret storage round-trip",
  "input_schema": { "type": "object", "properties": {} }
}
```

### `postgres_check_connectivity`
```json
{
  "name": "postgres_check_connectivity",
  "description": "Verify PostgreSQL database connectivity and version",
  "input_schema": { "type": "object", "properties": {} }
}
```

### `postgres_seed_admin`
```json
{
  "name": "postgres_seed_admin",
  "description": "Create the initial admin user record from Entra OID",
  "input_schema": {
    "type": "object",
    "properties": {
      "oid": { "type": "string" },
      "name": { "type": "string" },
      "email": { "type": "string" }
    },
    "required": ["oid", "name", "email"]
  }
}
```

## Bicep Resource Summary

For production Azure deployment, the Bicep template provisions:

```
Resource Group
├── Log Analytics Workspace
│   └── Centralized logging for all resources
├── Container Apps Environment
│   └── SAO Container App
│       ├── Image: ghcr.io/jbcupps/sao:<tag> (built from docker/Dockerfile)
│       ├── Port: 3100 (external ingress)
│       ├── Min replicas: 1, Max: 3
│       ├── DATABASE_URL via Container Apps secretRef
│       └── Startup, readiness, and liveness probes on /api/health
└── PostgreSQL Flexible Server (v16)
    ├── SKU: B_Standard_B1ms (burstable)
    ├── Storage: 32 GB
    └── Database: sao
```

The deployment is only considered ready after ARM succeeds, the latest Container App revision is healthy, and `/api/health` returns a healthy response.

## Error Recovery

The installer is designed for graceful error handling:

| Failure | Recovery |
|---------|----------|
| Entra auth fails | Agent explains the error, suggests checking tenant ID and app config |
| Database unreachable | Agent checks the secret-backed `DATABASE_URL`, startup retry window, revision health, replicas, and Container Apps logs |
| Graph API permission denied | Agent walks admin through adding required API permissions |
| Vault operation fails | Agent checks encryption state, suggests re-initialization |
| Container restart during install | Agent loads state from PostgreSQL, resumes from last step |
| Admin closes browser | State persisted; re-opening resumes the conversation |

## Audit Trail

Every installer action is recorded:

```sql
INSERT INTO installer_audit_log (
    step,           -- e.g., 'entra_authenticate'
    tool_name,      -- e.g., 'entra_authenticate_admin'
    parameters,     -- JSON (secrets redacted)
    result_status,  -- 'success' | 'error'
    error_message,  -- NULL on success
    created_at      -- timestamp
);
```

The installer agent itself is never deleted — its conversation history and audit trail are preserved for compliance and debugging.
