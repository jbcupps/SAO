export interface User {
  id: string;
  username: string;
  display_name: string;
  role: 'user' | 'admin';
  created_at: string;
  updated_at: string;
}

export interface AuthTokens {
  access_token: string;
  refresh_token: string;
  token_type: string;
  expires_in: number;
}

export interface VaultSecret {
  id: string;
  secret_type: string;
  label: string;
  provider: string;
  value?: string;
  metadata: Record<string, string>;
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
  resource: string;
  details: string | null;
  ip_address: string | null;
  user_agent: string | null;
  created_at: string;
}

export interface SetupStatus {
  initialized: boolean;
  has_users: boolean;
  needs_setup: boolean;
}

export type FrontierProvider = 'openai' | 'anthropic' | 'google';

export interface BootstrapModelConfig {
  provider: FrontierProvider;
  model: string;
  api_key: string;
  entity_name?: string;
}

export interface AdminEntitySummary {
  id: string;
  identity_agent_id: string;
  name: string;
  provider: FrontierProvider;
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

export interface SetupInitializationResult extends AdminEntityOverview {
  status: 'initialized';
  user_id: string;
  vault_status: 'unsealed';
}

export interface VaultStatus {
  status: 'uninitialized' | 'sealed' | 'unsealed';
}

export interface CreateSecretData {
  secret_type: string;
  label: string;
  provider: string;
  value: string;
  metadata?: Record<string, string>;
}

export interface UpdateSecretData {
  label?: string;
  provider?: string;
  value?: string;
  metadata?: Record<string, string>;
}

export interface CreateOidcProviderData {
  name: string;
  issuer_url: string;
  client_id: string;
  client_secret: string;
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
