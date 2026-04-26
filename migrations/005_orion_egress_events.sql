-- Durable Orion egress idempotency store.

CREATE TABLE orion_egress_events (
    event_id UUID PRIMARY KEY,
    user_id UUID NOT NULL,
    agent_id UUID,
    orion_id UUID NOT NULL,
    event_type TEXT NOT NULL,
    payload JSONB NOT NULL,
    enqueued_at TIMESTAMPTZ NOT NULL,
    attempts INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_orion_egress_events_user ON orion_egress_events(user_id);
CREATE INDEX idx_orion_egress_events_agent ON orion_egress_events(agent_id);
CREATE INDEX idx_orion_egress_events_created ON orion_egress_events(created_at);
