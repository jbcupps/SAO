-- Add Grok (xAI) and Gemini (Google Generative Language) to the llm_provider_settings allowlist.
-- Idempotent for fresh installs that already have the relaxed constraint from 007.

ALTER TABLE llm_provider_settings DROP CONSTRAINT IF EXISTS llm_provider_settings_provider_check;
ALTER TABLE llm_provider_settings
    ADD CONSTRAINT llm_provider_settings_provider_check
    CHECK (provider IN ('openai', 'anthropic', 'ollama', 'grok', 'gemini'));

INSERT INTO llm_provider_settings (provider, enabled) VALUES
    ('grok', false),
    ('gemini', false)
ON CONFLICT (provider) DO NOTHING;
