-- SAO Platform Database Schema
-- All tables for vault, auth, agents, audit, and configuration

CREATE EXTENSION IF NOT EXISTS "pgcrypto";

-- Vault Master Key storage (sealed VMK envelope)
CREATE TABLE vault_master_key (
    id SERIAL PRIMARY KEY,
    encrypted_key BYTEA NOT NULL,
    kdf_salt BYTEA NOT NULL,
    kdf_memory_cost INTEGER NOT NULL DEFAULT 65536,
    kdf_time_cost INTEGER NOT NULL DEFAULT 3,
    kdf_parallelism INTEGER NOT NULL DEFAULT 1,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    rotated_at TIMESTAMPTZ
);

-- Users
CREATE TABLE users (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    username TEXT NOT NULL UNIQUE,
    display_name TEXT,
    role TEXT NOT NULL DEFAULT 'user' CHECK (role IN ('user', 'admin')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- WebAuthn credentials
CREATE TABLE webauthn_credentials (
    id SERIAL PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    credential_id TEXT NOT NULL UNIQUE,
    credential_json JSONB NOT NULL,
    label TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_used_at TIMESTAMPTZ
);

-- WebAuthn challenge state (ephemeral, with TTL)
CREATE TABLE webauthn_challenges (
    id TEXT PRIMARY KEY,
    challenge_json JSONB NOT NULL,
    challenge_type TEXT NOT NULL CHECK (challenge_type IN ('registration', 'authentication')),
    user_id UUID REFERENCES users(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at TIMESTAMPTZ NOT NULL DEFAULT (now() + INTERVAL '5 minutes')
);

-- OIDC providers (admin-configurable)
CREATE TABLE oidc_providers (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL UNIQUE,
    issuer_url TEXT NOT NULL,
    client_id TEXT NOT NULL,
    client_secret_encrypted BYTEA,
    scopes TEXT NOT NULL DEFAULT 'openid profile email',
    enabled BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- OIDC user identity links
CREATE TABLE oidc_user_links (
    id SERIAL PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    provider_id UUID NOT NULL REFERENCES oidc_providers(id) ON DELETE CASCADE,
    subject TEXT NOT NULL,
    email TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (provider_id, subject)
);

-- Registered agents
CREATE TABLE agents (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    owner_user_id UUID REFERENCES users(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    public_key BYTEA,
    master_signature BYTEA,
    capabilities JSONB NOT NULL DEFAULT '[]',
    state TEXT NOT NULL DEFAULT 'offline',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Encrypted vault secrets
CREATE TABLE vault_secrets (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    owner_user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    secret_type TEXT NOT NULL CHECK (secret_type IN ('ed25519', 'api_key', 'gpg', 'oauth_token', 'other')),
    label TEXT NOT NULL,
    provider TEXT,
    ciphertext BYTEA NOT NULL,
    nonce BYTEA NOT NULL,
    metadata JSONB DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Refresh tokens for JWT sessions
CREATE TABLE refresh_tokens (
    id SERIAL PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token_hash TEXT NOT NULL UNIQUE,
    expires_at TIMESTAMPTZ NOT NULL,
    revoked BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Audit log
CREATE TABLE audit_log (
    id BIGSERIAL PRIMARY KEY,
    user_id UUID,
    agent_id UUID,
    action TEXT NOT NULL,
    resource TEXT,
    details JSONB,
    ip_address TEXT,
    user_agent TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- System configuration (key-value store)
CREATE TABLE system_config (
    key TEXT PRIMARY KEY,
    value JSONB NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Indexes
CREATE INDEX idx_agents_owner ON agents(owner_user_id);
CREATE INDEX idx_vault_secrets_owner ON vault_secrets(owner_user_id);
CREATE INDEX idx_vault_secrets_type ON vault_secrets(secret_type);
CREATE INDEX idx_audit_log_user ON audit_log(user_id);
CREATE INDEX idx_audit_log_created ON audit_log(created_at);
CREATE INDEX idx_refresh_tokens_user ON refresh_tokens(user_id);
CREATE INDEX idx_webauthn_credentials_user ON webauthn_credentials(user_id);
CREATE INDEX idx_webauthn_challenges_expires ON webauthn_challenges(expires_at);
