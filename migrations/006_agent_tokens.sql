-- Long-lived agent-scoped identity tokens minted at bundle download.
--
-- Design: each row's `id` is the JWT `jti` claim. The token itself is a JWT signed with
-- SAO_JWT_SECRET (HS256), carrying entity claims (principal_type=non_human, human_owner=user_id,
-- entity_kind=orion, scope, etc.). This row exists primarily for revocation: validation
-- decodes the JWT, then looks up jti=id and rejects if revoked_at IS NOT NULL or expired.
-- Future migration to Entra/external IdP swaps the issuance + verification path but keeps the
-- bundle/runtime contract (Authorization: Bearer <opaque-or-jwt>) intact.

CREATE TABLE agent_tokens (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    agent_id UUID NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    issued_by UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    issued_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at TIMESTAMPTZ,
    revoked_at TIMESTAMPTZ,
    last_used_at TIMESTAMPTZ,
    scope TEXT NOT NULL DEFAULT 'orion:policy orion:egress llm:generate'
);

CREATE INDEX idx_agent_tokens_active ON agent_tokens(agent_id) WHERE revoked_at IS NULL;
