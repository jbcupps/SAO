# Agent Archetype

SAO governs agents as owned, reviewable runtime entities.

## Identity Artifacts

The repo still models agents around durable identity material such as:

- `soul.md`
- `ethics.md`
- `org-map.md`
- `personality.md`

These documents matter because they define the intended trust and behavioral envelope for an agent, even when the operational control plane is handling identity, sessions, and governed skills.

## Skills As Governed Artifacts

Each skill should be treated as a capability package with:

- a clear operational purpose
- declared permissions
- reviewable inputs and outputs
- lifecycle status such as pending review, approved, or deprecated

The `skills/` directory is kept in the repo to illustrate that governance model.
