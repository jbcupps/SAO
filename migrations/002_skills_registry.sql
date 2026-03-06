-- Skills & Tools Registry
-- Global catalog of skills, agent-skill bindings, and review audit trail

-- Global skill definitions
CREATE TABLE skill_catalog (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL,
    version TEXT NOT NULL DEFAULT '1.0.0',
    description TEXT,
    author TEXT,
    category TEXT,
    tags TEXT[] NOT NULL DEFAULT '{}',
    permissions TEXT[] NOT NULL DEFAULT '{}',
    api_endpoints TEXT[] NOT NULL DEFAULT '{}',
    input_schema JSONB,
    output_schema JSONB,
    risk_level TEXT NOT NULL DEFAULT 'unknown' CHECK (risk_level IN ('low', 'medium', 'high', 'critical', 'unknown')),
    status TEXT NOT NULL DEFAULT 'pending_review' CHECK (status IN ('pending_review', 'approved', 'rejected', 'deprecated')),
    policy_score INTEGER,
    policy_details JSONB,
    created_by_user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    created_by_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    reviewed_by_user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    review_notes TEXT,
    reviewed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (name, version)
);

-- Which agents use which skills
CREATE TABLE agent_skill_bindings (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    agent_id UUID NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    skill_id UUID NOT NULL REFERENCES skill_catalog(id) ON DELETE CASCADE,
    status TEXT NOT NULL DEFAULT 'pending_review' CHECK (status IN ('pending_review', 'approved', 'rejected', 'revoked')),
    config JSONB DEFAULT '{}',
    declared_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    reviewed_by_user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    review_notes TEXT,
    reviewed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (agent_id, skill_id)
);

-- Immutable audit trail of review actions
CREATE TABLE skill_reviews (
    id BIGSERIAL PRIMARY KEY,
    target_type TEXT NOT NULL CHECK (target_type IN ('catalog', 'binding')),
    target_id UUID NOT NULL,
    action TEXT NOT NULL CHECK (action IN ('auto_approve', 'auto_flag', 'manual_approve', 'manual_reject', 'request_changes', 'revoke')),
    reviewer_user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    policy_score INTEGER,
    policy_details JSONB,
    notes TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Indexes
CREATE INDEX idx_skill_catalog_status ON skill_catalog(status);
CREATE INDEX idx_skill_catalog_category ON skill_catalog(category);
CREATE INDEX idx_skill_catalog_name_version ON skill_catalog(name, version);
CREATE INDEX idx_skill_catalog_created ON skill_catalog(created_at);
CREATE INDEX idx_agent_skill_bindings_agent ON agent_skill_bindings(agent_id);
CREATE INDEX idx_agent_skill_bindings_skill ON agent_skill_bindings(skill_id);
CREATE INDEX idx_agent_skill_bindings_status ON agent_skill_bindings(status);
CREATE INDEX idx_skill_reviews_target ON skill_reviews(target_type, target_id);
CREATE INDEX idx_skill_reviews_created ON skill_reviews(created_at);
