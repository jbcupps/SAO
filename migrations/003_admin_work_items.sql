CREATE TABLE admin_work_items (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    admin_agent_id UUID NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    sequence_no INTEGER NOT NULL,
    slug TEXT NOT NULL,
    title TEXT NOT NULL,
    description TEXT,
    area TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'in_progress', 'blocked', 'done')),
    priority INTEGER NOT NULL DEFAULT 100,
    metadata JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (admin_agent_id, slug),
    UNIQUE (admin_agent_id, sequence_no)
);

CREATE INDEX idx_admin_work_items_agent_priority
    ON admin_work_items (admin_agent_id, priority, sequence_no);
