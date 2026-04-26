-- Per-entity LLM provider + model selection. Set at agent creation, used by SAO LLM proxy
-- to dispatch /api/llm/generate calls coming from that agent.

ALTER TABLE agents ADD COLUMN default_provider TEXT;
ALTER TABLE agents ADD COLUMN default_id_model TEXT;
ALTER TABLE agents ADD COLUMN default_ego_model TEXT;
