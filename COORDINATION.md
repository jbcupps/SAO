# SAO Coordination

The local OrionII integration MVP is coordinated by these docs:

- `docs/orion-sao-mvp.md` for the shared API contract.
- `docs/runbooks/local-orion-sao-mvp.md` for the local end-to-end runbook.
- `docs/runbooks/verify-orion-topic-shift.md` for SAO-side verification of OrionII internal transport changes.
- `C:\Repo\OrionII\docs\sao-mvp-client.md` for OrionII client behavior.

Keep browser auth cookie-based and CSRF-protected. The Orion MVP uses a separate Bearer-authenticated
machine route group under `/api/orion`.

OrionII transport boundary:

- SAO packages OrionII with `bus_transport: { kind: "nats_jetstream", port: 4222 }` so the entity
  runtime prefers durable local topics.
- SAO does not run or join that bus. SAO remains the external birth, policy, LLM-proxy, and
  sanitized-egress HTTP seam.

Bootstrap boundaries:

- Production and cloud environments use the Azure conversational installer.
- Local Docker MVP work uses `sao-server bootstrap-local` from the Compose service with
  `SAO_LOCAL_BOOTSTRAP=true`.
- Local OrionII tokens come from `sao-server mint-dev-token` and are only for MVP/dev testing.
