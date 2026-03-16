# Testing Status

Validation completed on 2026-03-16:

- `cargo test`
- `cargo clippy --workspace -- -D warnings`
- `npm --prefix frontend test`
- `npm --prefix frontend run build`
- `python -m unittest discover installer/tests`
- `POSTGRES_PASSWORD=local-dev-only-change-me docker compose -f docker/docker-compose.yml config`
- `az bicep build --file installer/bicep/main.bicep`

Result: pass.

Notes:

- Bicep validation was performed locally with `az bicep build`.
- Compose validation used a local development-only password supplied at runtime.
