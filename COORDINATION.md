# SAO Coordination

The local OrionII integration MVP is coordinated by these docs:

- `docs/orion-sao-mvp.md` for the shared API contract.
- `docs/runbooks/local-orion-sao-mvp.md` for the local end-to-end runbook.
- `C:\Repo\OrionII\docs\sao-mvp-client.md` for OrionII client behavior.

Keep browser auth cookie-based and CSRF-protected. The Orion MVP uses a separate Bearer-authenticated
machine route group under `/api/orion`.

Bootstrap boundaries:

- Production and cloud environments use the Azure conversational installer.
- Local Docker MVP work uses `sao-server bootstrap-local` from the Compose service with
  `SAO_LOCAL_BOOTSTRAP=true`.
- Local OrionII tokens come from `sao-server mint-dev-token` and are only for MVP/dev testing.
