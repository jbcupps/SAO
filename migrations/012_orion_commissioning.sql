-- OrionII interactive commissioning state.
--
-- The bundle still carries only the SAO anchor and bearer token. The first-run
-- commissioning flow creates one active session per agent, stores mentor/entity
-- private keys in vault_secrets, and persists the canonical charter plus signed
-- birth certificate after finalize.

CREATE TABLE orion_commissions (
    commission_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    agent_id UUID NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    mentor_id UUID NOT NULL,
    orion_id UUID NOT NULL DEFAULT gen_random_uuid(),
    mentor_secret_id UUID REFERENCES vault_secrets(id) ON DELETE SET NULL,
    entity_secret_id UUID REFERENCES vault_secrets(id) ON DELETE SET NULL,
    mentor_public_key BYTEA NOT NULL,
    entity_public_key BYTEA NOT NULL,
    role_key TEXT,
    charter_text TEXT,
    charter_hash TEXT,
    birth_certificate JSONB,
    finalized_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX idx_orion_commissions_one_active_per_agent
    ON orion_commissions (agent_id)
    WHERE finalized_at IS NULL;

CREATE UNIQUE INDEX idx_orion_commissions_one_finalized_per_agent
    ON orion_commissions (agent_id)
    WHERE finalized_at IS NOT NULL;

CREATE INDEX idx_orion_commissions_agent
    ON orion_commissions (agent_id);

CREATE INDEX idx_orion_commissions_finalized
    ON orion_commissions (finalized_at);
