# SAO Deep Dive

## Superego Integration – Personality Evolution (added 2026-02-19)

**New constitutional document**
At birth SAO now injects four files (all signed):

- `soul.md` (immutable, never modified post-birth)
- `ethics.md` (TriangleEthic commitments)
- `org-map.md` (hive/role)
- `personality.md` (ego traits, style, tone – the only file Superego may propose changes to)

**Superego provisioning & monitoring surface**
- SAO exposes new WS endpoint `/ws/superego/{agent_id}` for ego-log streaming.
- Periodic roll-up jobs are scheduled via SAO cron (configurable per-agent criticality flag).
- Persistent mode enabled via `criticality: high` in org-map → full conversation stream to Ethical_AI_Reg.
- All tweak proposals are forwarded through ethical_bridge → Ethical_AI_Reg → back to SAO for personality.md patch + re-sign.

Soul integrity check remains unchanged: any attempt to modify `soul.md` fails the signature verification at boot.
