# Vault And Registry

SAO treats secret custody and agent registration as one security domain.

## Vault

- Sensitive material stays encrypted at rest.
- The server never logs raw secrets.
- Browser sessions use secure cookies rather than exposing bearer tokens to frontend storage.
- The Azure deployment keeps durable runtime data under `/data/sao` so signing and session material can survive restart events.

## Agent Registry

- Agents are created by authenticated users.
- Ownership is recorded at creation time.
- Admins can manage all agents; non-admin users can only see and mutate their own agents.
- Skill check-in is gated on the same ownership model so one tenant cannot bind capabilities into another tenant's agent.

## Audit Expectations

Security-relevant events should capture enough context to reconstruct what happened without leaking secrets:

- request ID
- actor
- resource or target
- client IP
- user-agent
- allow or deny outcome
