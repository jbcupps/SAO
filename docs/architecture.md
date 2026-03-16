# SAO Architecture

SAO is the control plane for enterprise AI agents. Its job is to centralize identity, session security, secret custody, skill governance, lifecycle control, and auditability so agents can be deployed like managed infrastructure instead of isolated scripts.

## Core Principles

- Zero trust: every browser request, agent action, and privileged change is authenticated, authorized, and logged.
- No employee API keys: secrets stay in managed custody instead of living in prompts, notebooks, or local shell history.
- Governed capabilities: skills are reviewed and bound to agents deliberately, not attached ad hoc.
- Durable operations: bootstrap state, session integrity, and runtime data survive restart events.
- Ethical runtime verification: the runtime contract must remain explainable, reviewable, and aligned with the architecture source of truth.

## Runtime Shape

- Browser operators authenticate through Microsoft Entra ID or approved local WebAuthn fallback.
- The backend issues secure cookie sessions and enforces CSRF for state-changing browser traffic.
- Agents are owned resources, with CRUD and skill check-in scoped to the owning user or an admin.
- Sensitive events carry request-scoped audit context such as request ID, actor, client IP, and user-agent.

## Azure Deployment Shape

- Public entry point: Azure Container Apps over HTTPS.
- Private data path: PostgreSQL Flexible Server on delegated private networking.
- Durable application state: Azure Files mounted at `/data/sao`.
- Operational telemetry: Log Analytics plus `/api/health` verification.
- Bootstrap contract: installer-provided Entra inputs, browser origin settings, and optional OIDC seed values.

## Reference Documents

- [bootstrap-installer.md](bootstrap-installer.md)
- [installer-architecture.md](installer-architecture.md)
- [SAO_INSTALLER_SPEC.md](SAO_INSTALLER_SPEC.md)
- [VAULT_AND_REGISTRY.md](VAULT_AND_REGISTRY.md)
