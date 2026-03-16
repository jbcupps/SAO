# Installer Architecture

The SAO installer is a standalone Claude-powered bootstrap container that talks the operator through Azure deployment, permission validation, troubleshooting, and runtime handoff.

## Why It Exists

The installer replaces brittle bootstrap runbooks with a governed conversation:

- every major write action is announced before execution
- read-only discovery can be batched for faster review
- failures stay inside a troubleshooting loop until the operator understands them
- the deployment is not considered finished until the live runtime passes health verification

## Current Flow

1. Device-code Azure login.
2. Read-only discovery of operator identity, subscriptions, and effective permissions.
3. Resource group selection.
4. Bicep validation.
5. Azure provisioning of networking, private PostgreSQL, Key Vault, Storage, Log Analytics, Container Apps environment, and the SAO runtime.
6. Health verification and handoff to the live SAO URL.

## Security Model

- The installer runs locally in a container.
- Azure actions are performed through explicit `az` CLI argv calls, not shell-constructed command strings.
- Only approved provisioning and troubleshooting actions are available through tool calls.
- Optional bootstrap inputs such as OIDC seed values and browser origins are forwarded as explicit Bicep parameters.
- The operator shell remains local, but the active installer conversation and tool outputs are sent to Anthropic while the session is running.

## Failure Handling

The installer supports guided recovery for the failure modes that matter most during Azure bootstrap:

- soft-deleted Key Vault name collisions
- image pull failures
- resource group cleanup and retry
- suffix-based retry for globally unique Azure names

The troubleshooting path is intentionally narrow. If a deployment fails, the installer should first explain the failing resource, likely cause, and safest next action before retrying anything.
