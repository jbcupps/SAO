import { apiRequest } from './client';
import type {
  AdminEntityOverview,
  User,
  EntityArchive,
  OidcProvider,
  AuditLogEntry,
  CreateOidcProviderData,
  UpdateOidcProviderData,
  AuditLogParams,
} from '../types';

export async function listUsers(): Promise<User[]> {
  const res = await apiRequest<{ users: User[] }>('/api/admin/users');
  return res.users;
}

export async function updateUserRole(
  id: string,
  role: 'user' | 'admin',
): Promise<{ updated: boolean }> {
  return apiRequest<{ updated: boolean }>(`/api/admin/users/${id}/role`, {
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
): Promise<{ id: string }> {
  return apiRequest<{ id: string }>('/api/admin/oidc/providers', {
    method: 'POST',
    body: JSON.stringify(data),
  });
}

export async function listOidcProviders(): Promise<OidcProvider[]> {
  const res = await apiRequest<{ providers: OidcProvider[] }>(
    '/api/admin/oidc/providers',
  );
  return res.providers;
}

export async function updateOidcProvider(
  id: string,
  data: UpdateOidcProviderData,
): Promise<{ updated: boolean }> {
  return apiRequest<{ updated: boolean }>(`/api/admin/oidc/providers/${id}`, {
    method: 'PUT',
    body: JSON.stringify(data),
  });
}

export async function deleteOidcProvider(id: string): Promise<void> {
  return apiRequest<void>(`/api/admin/oidc/providers/${id}`, {
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
  const res = await apiRequest<{ audit_log: AuditLogEntry[] }>(
    `/api/admin/audit${qs ? `?${qs}` : ''}`,
  );
  return res.audit_log;
}

export async function getAdminEntityOverview(): Promise<AdminEntityOverview> {
  return apiRequest<AdminEntityOverview>('/api/admin/admin-entity');
}

export async function listEntityArchives(): Promise<EntityArchive[]> {
  const res = await apiRequest<{ archives: EntityArchive[] }>(
    '/api/admin/entity-archives',
  );
  return res.archives;
}
