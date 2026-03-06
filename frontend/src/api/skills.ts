import { apiRequest } from './client';
import type {
  SkillCatalogEntry,
  AgentSkillBinding,
  SkillReview,
  CreateSkillData,
  SkillCheckinEntry,
  SkillCheckinResult,
  ReviewAction,
} from '../types';

// --- User endpoints ---

export async function listSkills(params?: {
  status?: string;
  category?: string;
}): Promise<SkillCatalogEntry[]> {
  const search = new URLSearchParams();
  if (params?.status) search.set('status', params.status);
  if (params?.category) search.set('category', params.category);
  const qs = search.toString();
  const path = qs ? `/api/skills?${qs}` : '/api/skills';
  const res = await apiRequest<{ skills: SkillCatalogEntry[] }>(path);
  return res.skills;
}

export async function getSkill(id: string): Promise<SkillCatalogEntry> {
  return apiRequest<SkillCatalogEntry>(`/api/skills/${id}`);
}

export async function listSkillReviews(
  skillId: string,
): Promise<SkillReview[]> {
  const res = await apiRequest<{ reviews: SkillReview[] }>(
    `/api/skills/${skillId}/reviews`,
  );
  return res.reviews;
}

export async function listBindingReviews(
  bindingId: string,
): Promise<SkillReview[]> {
  const res = await apiRequest<{ reviews: SkillReview[] }>(
    `/api/skills/bindings/${bindingId}/reviews`,
  );
  return res.reviews;
}

export async function listAgentSkills(
  agentId: string,
): Promise<AgentSkillBinding[]> {
  const res = await apiRequest<{ bindings: AgentSkillBinding[] }>(
    `/api/agents/${agentId}/skills`,
  );
  return res.bindings;
}

export async function agentSkillCheckin(
  agentId: string,
  skills: SkillCheckinEntry[],
): Promise<{ results: SkillCheckinResult[] }> {
  return apiRequest<{ results: SkillCheckinResult[] }>(
    `/api/agents/${agentId}/skills/checkin`,
    {
      method: 'POST',
      body: JSON.stringify({ skills }),
    },
  );
}

// --- Admin endpoints ---

export async function adminCreateSkill(
  data: CreateSkillData,
): Promise<SkillCatalogEntry> {
  return apiRequest<SkillCatalogEntry>('/api/admin/skills', {
    method: 'POST',
    body: JSON.stringify(data),
  });
}

export async function adminUpdateSkill(
  id: string,
  data: Partial<CreateSkillData>,
): Promise<{ updated: boolean }> {
  return apiRequest<{ updated: boolean }>(`/api/admin/skills/${id}`, {
    method: 'PUT',
    body: JSON.stringify(data),
  });
}

export async function adminDeleteSkill(
  id: string,
): Promise<{ deleted: boolean }> {
  return apiRequest<{ deleted: boolean }>(`/api/admin/skills/${id}`, {
    method: 'DELETE',
  });
}

export async function adminReviewSkill(
  id: string,
  action: ReviewAction,
  notes?: string,
): Promise<{ status: string }> {
  return apiRequest<{ status: string }>(`/api/admin/skills/${id}/review`, {
    method: 'POST',
    body: JSON.stringify({ action, notes }),
  });
}

export async function adminListPendingSkills(): Promise<SkillCatalogEntry[]> {
  const res = await apiRequest<{ skills: SkillCatalogEntry[] }>(
    '/api/admin/skills/pending',
  );
  return res.skills;
}

export async function adminListPendingBindings(): Promise<
  AgentSkillBinding[]
> {
  const res = await apiRequest<{ bindings: AgentSkillBinding[] }>(
    '/api/admin/skills/bindings/pending',
  );
  return res.bindings;
}

export async function adminReviewBinding(
  id: string,
  action: ReviewAction,
  notes?: string,
): Promise<{ status: string }> {
  return apiRequest<{ status: string }>(
    `/api/admin/skills/bindings/${id}/review`,
    {
      method: 'POST',
      body: JSON.stringify({ action, notes }),
    },
  );
}
