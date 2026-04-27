# Verify OrionII Topic/Durable-Bus Shift From the SAO Side

This runbook verifies that OrionII can change its internal transport
(for example, from in-memory topics to NATS JetStream-backed topics) without breaking
the SAO-facing seam.

What this runbook proves:

- SAO can still issue a bundle with the expected anchor fields.
- The bundle includes `deployment.json` so the downloaded package records the SAO origin and
  install intent without requiring a user to edit JSON.
- The bundle includes a double-click installer launcher that writes `config.json` automatically.
- The bundle requests OrionII's `nats_jetstream` bus transport without making SAO a bus participant.
- The bundle still carries a valid entity JWT.
- The entity JWT still works on `GET /api/orion/birth`,
  `POST /api/llm/generate`, and `POST /api/orion/egress`.
- SAO still persists egress, updates `last_heartbeat`, and records the
  expected audit evidence.

What this runbook does not prove on its own:

- OrionII's internal topic graph is correct.
- OrionII's bus subscribers are wired as intended.
- OrionII's UI is showing the right state.

Use it in one of two ways:

1. `PrepareOnly` to create a fresh SAO verification subject for a manual
   two-window UAT run with OrionII.
2. Full contract exercise mode to simulate the OrionII seam from SAO by
   extracting the bundle's entity JWT and calling the entity routes
   directly.

## Contract Invariants

Treat `docs/orion-sao-mvp.md` as the shared contract. OrionII internal
transport changes must preserve:

- bundle `config.json` anchor fields plus `bus_transport.kind = nats_jetstream`
- bundle install manifest + launcher entries: `deployment.json`, `Install-OrionII.cmd`, and
  `Install-OrionII.ps1`
- entity bearer auth on `/api/orion/*` and `/api/llm/generate`
- current egress JSON shape
- current event types: `identitySync`, `auditAction`, `memoryEvent`

SAO is intentionally blind to OrionII's internal bus implementation. The
verification surface here is the HTTP seam only.

## Preflight

Bring up SAO and confirm the local control plane is healthy before the
verification run:

```powershell
cd C:\Repo\SAO
.\scripts\local-mvp-smoke.ps1 -StartCompose
```

If you need the smoke flow to configure a local Ollama provider and prove
bundle download before the seam verifier, use:

```powershell
cd C:\Repo\SAO
.\scripts\local-mvp-smoke.ps1 -StartCompose -OllamaBaseUrl "http://host.docker.internal:11434" -OllamaModel "llama3.2"
```

The smoke script remains preflight only. It does not count as the
milestone proof because it does not exercise the bundle's entity JWT on
`birth -> llm -> egress`.

## Manual Two-Window Verification

Use this when OrionII is being driven in a separate window and SAO should
only prepare the verification subject and observe evidence.

```powershell
cd C:\Repo\SAO
.\scripts\verify-orion-topic-shift.ps1 `
  -Provider "anthropic" `
  -IdModel "claude-haiku-4-5-20251001" `
  -EgoModel "claude-haiku-4-5-20251001" `
  -PrepareOnly `
  -OutputDir ".\artifacts"
```

The script will:

- verify SAO health, setup mode, and vault state
- confirm a default installer source exists
- create a fresh agent
- download a fresh bundle
- extract `config.json`
- extract `deployment.json`
- record SAO commit, OrionII commit, agent id, installer sha/version,
  provider, models, and bundle download time into `report.json`

Then in the OrionII window:

1. Install or launch the build under test.
2. Extract the bundle and double-click `Install-OrionII.cmd`.
3. Confirm the launcher installs OrionII, writes `%APPDATA%\OrionII\config.json`, and starts the app.
4. Perform first launch / birth.
5. Send one chat that should hit `POST /api/llm/generate`.
6. Perform one action expected to emit egress after chat.

While that happens, watch these SAO surfaces:

- `/agents`
- `/agents/:id/events`
- `/api/admin/audit`
- `docker compose -f docker\docker-compose.yml logs -f sao | Select-String "orion|llm"`

Required evidence:

- `agents.bundle_downloaded` audit row exists.
- `orion.birth` audit row exists after OrionII launch.
- `llm.generate` audit row exists and `llm.generate.failed` does not.
- `/agents/:id/events` shows `identitySync` and at least one post-chat
  event such as `auditAction` or `memoryEvent`.
- `/api/agents/:id` shows a fresh `last_heartbeat`.

## Automated SAO-Side Contract Exercise

Use this when you want SAO to simulate the entity seam directly with the
bundle's entity token.

```powershell
cd C:\Repo\SAO
.\scripts\verify-orion-topic-shift.ps1 `
  -Provider "anthropic" `
  -IdModel "claude-haiku-4-5-20251001" `
  -EgoModel "claude-haiku-4-5-20251001" `
  -OutputDir ".\artifacts"
```

This mode:

- prepares the same fresh agent and bundle
- calls `GET /api/orion/birth` with the real entity JWT
- calls `POST /api/llm/generate` with the real entity JWT
- posts `identitySync` and `auditAction` egress with the real entity JWT
- waits for `/api/agents/:id/events`, `/api/agents/:id`, and
  `/api/admin/audit` to show the expected proof

The generated `report.json` captures the exact coordinates and observed
evidence for the run.

## Optional Regression Checks

Add `-RunRegressionChecks` to include:

- repeated `birth` call for restart continuity
- duplicate egress replay for idempotency
- bundle re-download to prove old-token revocation and new-token success
- agent deletion to prove entity-token revocation returns 401

Example:

```powershell
cd C:\Repo\SAO
.\scripts\verify-orion-topic-shift.ps1 `
  -Provider "anthropic" `
  -IdModel "claude-haiku-4-5-20251001" `
  -EgoModel "claude-haiku-4-5-20251001" `
  -RunRegressionChecks `
  -OutputDir ".\artifacts"
```

## Failure Isolation

- If bundle download fails, inspect installer pinning and staging first.
- If bundle succeeds but `birth` fails, inspect token/config drift or
  `/api/orion/birth` contract drift.
- If `birth` and `llm` succeed but no egress appears, treat it as an
  OrionII bus-to-egress subscriber problem, not a SAO problem.
- If egress reaches SAO but is rejected or malformed, treat it as shared
  contract drift and update both repos intentionally before retesting.
