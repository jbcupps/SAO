# SAO Project Guide

SAO is the enterprise control plane for AI agents. The current product story is Azure-first, Entra-first, and installer-led: the standalone Claude-powered bootstrap container provisions the platform, then operators land in the live SAO control plane.

## Current Architecture

- `docker/Dockerfile` is the production runtime image for SAO.
- `installer/Dockerfile` is the standalone conversational bootstrap image.
- `crates/sao-server` serves the API and frontend, manages cookie-based browser sessions, and enforces CSRF plus request-level audit context.
- `frontend/` is the React SPA for operator workflows after bootstrap.
- `installer/` contains the Azure bootstrap agent, Bicep templates, and troubleshooting tools.
- `skills/` contains example governed skill artifacts.

## Security Guardrails

- Do not reintroduce `POST /api/setup/initialize` or any browser setup wizard path.
- Browser auth is cookie-based. Do not store tokens in `localStorage`.
- Mutating browser requests must keep CSRF protection intact.
- New routes should default to authenticated and least-privilege access.
- Avoid public experimental endpoints unless they have a documented authz model.
- Never add unused secrets, dead env vars, or broad Azure permissions back into the deployment contract.

## Useful Commands

```bash
cargo test
cargo clippy --workspace -- -D warnings
npm --prefix frontend test
npm --prefix frontend run build
python -m unittest discover installer/tests
POSTGRES_PASSWORD=local-dev-only-change-me docker compose -f docker/docker-compose.yml config
az bicep build --file installer/bicep/main.bicep
```

## Documentation Alignment

- Keep repo-facing docs aligned with the installer-led Azure deployment story.
- Prefer the markdown files under `docs/` for repo-local explanations.
- Treat `documents/SAO_Orion_Architecture_Analysis_v2.docx` as the architecture source of truth for major decisions.
