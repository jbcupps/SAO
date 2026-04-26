import { apiRequest } from './client';
import type {
  CreateInstallerSourceData,
  InstallerSource,
  ProbeInstallerResult,
} from '../types';

export async function listInstallerSources(): Promise<InstallerSource[]> {
  const response = await apiRequest<{ sources: InstallerSource[] }>(
    '/api/admin/installer-sources',
  );
  return response.sources;
}

export async function probeInstallerSource(
  url: string,
): Promise<ProbeInstallerResult> {
  return apiRequest<ProbeInstallerResult>(
    '/api/admin/installer-sources/probe',
    {
      method: 'POST',
      body: JSON.stringify({ url }),
    },
  );
}

export async function createInstallerSource(
  data: CreateInstallerSourceData,
): Promise<{ source: InstallerSource; pre_warm: { ok: boolean; cache_path?: string; error?: string } }> {
  return apiRequest('/api/admin/installer-sources', {
    method: 'POST',
    body: JSON.stringify(data),
  });
}

export async function setDefaultInstallerSource(
  id: string,
): Promise<InstallerSource> {
  const response = await apiRequest<{ source: InstallerSource }>(
    `/api/admin/installer-sources/${id}/set-default`,
    { method: 'POST' },
  );
  return response.source;
}

export async function deleteInstallerSource(id: string): Promise<void> {
  await apiRequest<void>(`/api/admin/installer-sources/${id}`, {
    method: 'DELETE',
  });
}
