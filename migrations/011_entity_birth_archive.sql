-- Entity birth lifecycle and deletion archive records.
--
-- New agents are born as signed identity documents and a DB row under the same UUID.
-- When an agent is deleted, SAO writes an immutable archive manifest plus exported
-- Orion egress/memory events before removing the active agent row.

ALTER TABLE agents
    ADD COLUMN birth_status TEXT NOT NULL DEFAULT 'ready'
        CHECK (birth_status IN ('pending', 'ready', 'failed', 'archived'));

ALTER TABLE agents
    ADD COLUMN birthed_at TIMESTAMPTZ;

UPDATE agents
SET birthed_at = created_at
WHERE birthed_at IS NULL;

CREATE TABLE entity_archives (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    agent_id UUID NOT NULL,
    agent_name TEXT NOT NULL,
    owner_user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    created_by UUID REFERENCES users(id) ON DELETE SET NULL,
    reason TEXT,
    archive_path TEXT NOT NULL,
    manifest JSONB NOT NULL,
    egress_event_count INTEGER NOT NULL DEFAULT 0,
    memory_event_count INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_entity_archives_agent ON entity_archives(agent_id);
CREATE INDEX idx_entity_archives_owner ON entity_archives(owner_user_id);
CREATE INDEX idx_entity_archives_created ON entity_archives(created_at);
