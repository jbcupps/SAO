# SAO Deep Dive

SAO is best understood as an operating model for governed agents, not just a vault or a dashboard.

## What SAO Centralizes

- human operator sign-in and first-admin bootstrap
- ownership-aware agent registration
- secret storage and runtime secret handling
- governed skill registration, review, and binding
- audit logging for sensitive actions

## What SAO Explicitly Avoids

- local token persistence in the browser
- public bootstrap write endpoints
- unauthenticated experimental runtime surfaces
- broad network exposure for stateful data services

## Current Hardening Highlights

- cookie-based browser sessions with `HttpOnly`, `Secure`, and `SameSite=Lax`
- CSRF validation for state-changing browser requests
- nonce and state verification for OIDC callbacks
- server-side short-lived challenge storage for WebAuthn and OIDC state
- request-level rate limiting on auth and other sensitive paths
- ownership checks for agent CRUD and skill check-in

## Skills

`skills/` remains in the repo as example governed skill material. It is useful for policy, review, and lifecycle examples, but it is not a separate runtime or packaging target.
