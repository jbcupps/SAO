-- Admin-managed LLM provider configuration. API keys themselves live in vault_secrets
-- under labels 'provider:openai:api_key' and 'provider:anthropic:api_key' so they reuse
-- the existing AES-GCM-SIV envelope. Ollama needs no key, only a base_url.

CREATE TABLE llm_provider_settings (
    provider TEXT PRIMARY KEY CHECK (provider IN ('openai', 'anthropic', 'ollama', 'grok', 'gemini')),
    enabled BOOLEAN NOT NULL DEFAULT false,
    base_url TEXT,
    approved_models JSONB NOT NULL DEFAULT '[]'::jsonb,
    default_model TEXT,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by UUID REFERENCES users(id) ON DELETE SET NULL
);

INSERT INTO llm_provider_settings (provider, enabled) VALUES
    ('openai', false),
    ('anthropic', false),
    ('ollama', false),
    ('grok', false),
    ('gemini', false);
