export interface User {
  id: string;
  username: string;
  display_name?: string;
  role: 'user' | 'admin';
  created_at?: string;
  updated_at?: string;
}

export interface VaultSecret {
  id: string;
  secret_type: string;
  label: string;
  provider: string | null;
  value?: string;
  metadata: Record<string, unknown>;
  created_at: string;
  updated_at: string;
}

export interface Agent {
  id: string;
  name: string;
  public_key?: string;
  state: string;
  capabilities: string[];
  created_at: string;
  updated_at: string;
  birth_status?: 'pending' | 'ready' | 'failed' | 'archived';
  birthed_at?: string | null;
  default_provider?: string | null;
  default_id_model?: string | null;
  default_ego_model?: string | null;
}

export interface AgentLlmProviderOption {
  provider: LlmProviderName;
  approved_models: string[];
  default_model: string | null;
}

export type LlmProviderName =
  | 'openai'
  | 'anthropic'
  | 'ollama'
  | 'grok'
  | 'gemini';

export interface LlmProvider {
  provider: LlmProviderName;
  enabled: boolean;
  base_url: string | null;
  approved_models: string[];
  default_model: string | null;
  has_api_key: boolean;
  updated_at: string;
}

export interface LlmProviderTestResult {
  ok: boolean;
  provider: LlmProviderName;
  model: string;
  latency_ms: number;
  preview?: string;
  error?: string;
}

export interface UpdateLlmProviderData {
  enabled: boolean;
  base_url?: string | null;
  approved_models?: string[];
  default_model?: string | null;
  api_key?: string;
}

export interface InstallerSource {
  id: string;
  kind: 'orion-msi';
  url: string;
  filename: string;
  version: string;
  expected_sha256: string;
  is_default: boolean;
  enabled: boolean;
  created_at: string;
}

export interface ProbeInstallerResult {
  url: string;
  sha256: string;
}

export interface CreateInstallerSourceData {
  url: string;
  filename: string;
  version: string;
  expected_sha256: string;
  is_default?: boolean;
}

export interface AgentEgressEvent {
  event_id: string;
  user_id: string;
  agent_id: string | null;
  orion_id: string;
  event_type: string;
  payload: unknown;
  enqueued_at: string;
  attempts: number;
  created_at: string;
}

export interface AgentBirthResponse {
  status: 'READY';
  agent_id: string;
  identity_agent_id?: string;
  birth_status?: 'pending' | 'ready' | 'failed' | 'archived';
  birthed_at?: string | null;
  documents: string[];
  soul_immutable: boolean;
  personality_preview?: string;
  triangleethic_preview?: Record<string, unknown>;
}

export interface AgentStatusResponse {
  agent_id: string;
  status: 'READY';
  birth_status?: 'pending' | 'ready' | 'failed' | 'archived';
  birthed_at?: string | null;
  documents: string[];
  soul_immutable: boolean;
  personality_preview?: string;
  default_provider?: string | null;
  default_id_model?: string | null;
  default_ego_model?: string | null;
  available_llm_providers?: AgentLlmProviderOption[];
  last_heartbeat?: string | null;
}

export interface OidcProvider {
  id: string;
  name: string;
  issuer_url?: string;
  client_id?: string;
  has_client_secret?: boolean;
  scopes?: string;
  enabled: boolean;
  created_at?: string;
  updated_at?: string;
}

export interface AuditLogEntry {
  id: string;
  user_id: string | null;
  agent_id: string | null;
  action: string;
  resource: string | null;
  details: unknown;
  ip_address: string | null;
  user_agent: string | null;
  created_at: string;
}

export interface EntityArchive {
  id: string;
  agent_id: string;
  agent_name: string;
  owner_user_id: string | null;
  created_by: string | null;
  reason: string | null;
  archive_path: string;
  manifest: unknown;
  egress_event_count: number;
  memory_event_count: number;
  created_at: string;
}

export interface SetupStatus {
  initialized: boolean;
  has_users: boolean;
  needs_setup: boolean;
  bootstrap_mode?: 'installer_required' | 'operational';
  recommended_installer?: {
    command: string;
    commands?: {
      powershell?: string;
      bash?: string;
      published_image?: string;
    };
    image_role: string;
  };
}

export interface AdminEntitySummary {
  id: string;
  identity_agent_id: string;
  name: string;
  provider: string;
  model: string;
  secret_id: string;
  role?: string;
  deployment_target?: string;
  iac_strategy?: string;
  capabilities?: string[];
}

export interface AdminWorkItem {
  id: string;
  admin_agent_id: string;
  sequence_no: number;
  slug: string;
  title: string;
  description: string | null;
  area: string;
  status: 'pending' | 'in_progress' | 'blocked' | 'done';
  priority: number;
  metadata: Record<string, unknown>;
  created_at: string;
  updated_at: string;
}

export interface AdminEntityOverview {
  admin_entity: AdminEntitySummary;
  work_items: AdminWorkItem[];
}

export interface VaultStatus {
  status: 'uninitialized' | 'sealed' | 'unsealed';
}

export interface CreateSecretData {
  secret_type: string;
  label: string;
  provider?: string;
  value: string;
  metadata?: Record<string, unknown>;
}

export interface UpdateSecretData {
  label?: string;
  provider?: string;
  value?: string;
  metadata?: Record<string, unknown>;
}

export interface CreateOidcProviderData {
  name: string;
  issuer_url: string;
  client_id: string;
  client_secret?: string;
  scopes: string;
}

export interface UpdateOidcProviderData {
  name?: string;
  issuer_url?: string;
  client_id?: string;
  client_secret?: string;
  scopes?: string;
  enabled?: boolean;
}

export interface AuditLogParams {
  user_id?: string;
  offset?: number;
  limit?: number;
}

// --- Skills & Tools Registry ---

export interface SkillCatalogEntry {
  id: string;
  name: string;
  version: string;
  description: string | null;
  author: string | null;
  category: string | null;
  tags: string[];
  permissions: string[];
  api_endpoints: string[];
  input_schema: Record<string, unknown> | null;
  output_schema: Record<string, unknown> | null;
  risk_level: 'low' | 'medium' | 'high' | 'critical' | 'unknown';
  status: 'pending_review' | 'approved' | 'rejected' | 'deprecated';
  policy_score: number | null;
  policy_details: PolicyCheck[] | null;
  created_by_user_id: string | null;
  created_by_agent_id: string | null;
  reviewed_by_user_id: string | null;
  review_notes: string | null;
  reviewed_at: string | null;
  created_at: string;
  updated_at: string;
}

export interface AgentSkillBinding {
  id: string;
  agent_id: string;
  skill_id: string;
  status: 'pending_review' | 'approved' | 'rejected' | 'revoked';
  config: Record<string, unknown> | null;
  declared_at: string;
  reviewed_by_user_id: string | null;
  review_notes: string | null;
  reviewed_at: string | null;
  created_at: string;
  updated_at: string;
}

export interface SkillReview {
  id: string;
  target_type: 'catalog' | 'binding';
  target_id: string;
  action: string;
  reviewer_user_id: string | null;
  policy_score: number | null;
  policy_details: PolicyCheck[] | null;
  notes: string | null;
  created_at: string;
}

export interface PolicyCheck {
  name: string;
  passed: boolean;
  weight: number;
  message: string;
}

export interface CreateSkillData {
  name: string;
  version?: string;
  description?: string;
  author?: string;
  category?: string;
  tags?: string[];
  permissions?: string[];
  api_endpoints?: string[];
  input_schema?: Record<string, unknown>;
  output_schema?: Record<string, unknown>;
}

export interface SkillCheckinEntry {
  name: string;
  version: string;
  description?: string;
  author?: string;
  category?: string;
  tags?: string[];
  permissions?: string[];
  api_endpoints?: string[];
  input_schema?: Record<string, unknown>;
  output_schema?: Record<string, unknown>;
}

export interface SkillCheckinResult {
  name: string;
  version: string;
  skill_id: string;
  binding_id: string;
  skill_status: string;
  binding_status: string;
  policy_score: number | null;
  auto_approved: boolean;
}

export type ReviewAction = 'approve' | 'reject' | 'request_changes' | 'deprecate' | 'revoke';
