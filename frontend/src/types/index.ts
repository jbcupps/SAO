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
