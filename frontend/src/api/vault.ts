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
  return apiRequest<VaultSecret[]>('/api/keys');
}

export async function createSecret(data: CreateSecretData): Promise<VaultSecret> {
  return apiRequest<VaultSecret>('/api/keys', {
    method: 'POST',
    body: JSON.stringify(data),
  });
}

export async function getSecret(id: string): Promise<VaultSecret> {
  return apiRequest<VaultSecret>(`/api/keys/${id}`);
}

export async function updateSecret(
  id: string,
  data: UpdateSecretData,
): Promise<VaultSecret> {
  return apiRequest<VaultSecret>(`/api/keys/${id}`, {
    method: 'PUT',
    body: JSON.stringify(data),
  });
}

export async function deleteSecret(id: string): Promise<void> {
  return apiRequest<void>(`/api/keys/${id}`, {
    method: 'DELETE',
  });
}
