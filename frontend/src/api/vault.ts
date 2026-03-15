import { apiRequest } from './client';
import type {
  VaultStatus,
  VaultSecret,
  CreateSecretData,
  UpdateSecretData,
} from '../types';

export async function getVaultStatus(): Promise<VaultStatus> {
  return apiRequest<VaultStatus>('/api/vault/status');
}

export async function unsealVault(passphrase: string): Promise<void> {
  return apiRequest<void>('/api/vault/unseal', {
    method: 'POST',
    body: JSON.stringify({ passphrase }),
  });
}

export async function sealVault(): Promise<void> {
  return apiRequest<void>('/api/vault/seal', {
    method: 'POST',
  });
}

export async function listSecrets(): Promise<VaultSecret[]> {
  const res = await apiRequest<{ secrets: VaultSecret[] }>('/api/vault/secrets');
  return res.secrets;
}

export async function createSecret(data: CreateSecretData): Promise<VaultSecret> {
  const res = await apiRequest<{ id: string }>('/api/vault/secrets', {
    method: 'POST',
    body: JSON.stringify(data),
  });
  return {
    id: res.id,
    secret_type: data.secret_type,
    label: data.label,
    provider: data.provider || null,
    metadata: data.metadata ?? {},
    created_at: new Date().toISOString(),
    updated_at: new Date().toISOString(),
  };
}

export async function getSecret(id: string): Promise<VaultSecret> {
  return apiRequest<VaultSecret>(`/api/vault/secrets/${id}`);
}

export async function updateSecret(
  id: string,
  data: UpdateSecretData,
): Promise<VaultSecret> {
  await apiRequest<{ updated: boolean }>(`/api/vault/secrets/${id}`, {
    method: 'PUT',
    body: JSON.stringify(data),
  });
  return getSecret(id);
}

export async function deleteSecret(id: string): Promise<void> {
  return apiRequest<void>(`/api/vault/secrets/${id}`, {
    method: 'DELETE',
  });
}
