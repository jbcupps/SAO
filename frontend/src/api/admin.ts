import { apiRequest } from './client';
import type {
  User,
  OidcProvider,
  AuditLogEntry,
  CreateOidcProviderData,
  UpdateOidcProviderData,
  AuditLogParams,
} from '../types';

export async function listUsers(): Promise<User[]> {
  return apiRequest<User[]>('/api/admin/users');
}

export async function updateUserRole(
  id: string,
  role: 'user' | 'admin',
): Promise<User> {
  return apiRequest<User>(`/api/admin/users/${id}`, {
    method: 'PUT',
    body: JSON.stringify({ role }),
  });
}

export async function deleteUser(id: string): Promise<void> {
  return apiRequest<void>(`/api/admin/users/${id}`, {
    method: 'DELETE',
  });
}

export async function createOidcProvider(
  data: CreateOidcProviderData,
): Promise<OidcProvider> {
  return apiRequest<OidcProvider>('/api/admin/sso', {
    method: 'POST',
    body: JSON.stringify(data),
  });
}

export async function listOidcProviders(): Promise<OidcProvider[]> {
  return apiRequest<OidcProvider[]>('/api/admin/sso');
}

export async function updateOidcProvider(
  id: string,
  data: UpdateOidcProviderData,
): Promise<OidcProvider> {
  return apiRequest<OidcProvider>(`/api/admin/sso/${id}`, {
    method: 'PUT',
    body: JSON.stringify(data),
  });
}

export async function deleteOidcProvider(id: string): Promise<void> {
  return apiRequest<void>(`/api/admin/sso/${id}`, {
    method: 'DELETE',
  });
}

export async function queryAuditLog(
  params?: AuditLogParams,
): Promise<AuditLogEntry[]> {
  const searchParams = new URLSearchParams();
  if (params?.user_id) searchParams.set('user_id', params.user_id);
  if (params?.offset !== undefined)
    searchParams.set('offset', String(params.offset));
  if (params?.limit !== undefined)
    searchParams.set('limit', String(params.limit));

  const qs = searchParams.toString();
  return apiRequest<AuditLogEntry[]>(
    `/api/admin/audit${qs ? `?${qs}` : ''}`,
  );
}
