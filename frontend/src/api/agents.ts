import { apiRequest, ApiError } from './client';
import type {
  Agent,
  AgentBirthResponse,
  AgentEgressEvent,
  AgentStatusResponse,
} from '../types';

export interface CreateAgentInput {
  name: string;
  default_provider?: string;
  default_id_model?: string;
  default_ego_model?: string;
}

export async function listAgents(): Promise<Agent[]> {
  const response = await apiRequest<{ agents: Agent[] }>('/api/agents');
  return response.agents;
}

export async function createAgent(
  input: CreateAgentInput,
): Promise<AgentBirthResponse> {
  return apiRequest<AgentBirthResponse>('/api/agents', {
    method: 'POST',
    body: JSON.stringify(input),
  });
}

export async function getAgent(id: string): Promise<AgentStatusResponse> {
  return apiRequest<AgentStatusResponse>(`/api/agents/${id}`);
}

export async function deleteAgent(id: string): Promise<void> {
  return apiRequest<void>(`/api/agents/${id}/delete`, {
    method: 'POST',
  });
}

export async function listAgentEvents(
  id: string,
  limit = 50,
  offset = 0,
): Promise<AgentEgressEvent[]> {
  const response = await apiRequest<{
    events: AgentEgressEvent[];
    limit: number;
    offset: number;
  }>(`/api/agents/${id}/events?limit=${limit}&offset=${offset}`);
  return response.events;
}

export async function downloadAgentBundle(id: string, name: string): Promise<void> {
  const csrf = document.cookie
    .split(';')
    .map((entry) => entry.trim())
    .find((entry) => entry.startsWith('sao_csrf='));
  const csrfToken = csrf
    ? decodeURIComponent(csrf.slice('sao_csrf='.length))
    : '';

  const res = await fetch(`/api/agents/${id}/bundle`, {
    credentials: 'include',
    headers: csrfToken ? { 'X-CSRF-Token': csrfToken } : {},
  });
  if (!res.ok) {
    const text = await res.text();
    throw new ApiError(res.status, text);
  }
  const blob = await res.blob();

  const safeName = name.replace(/[^a-zA-Z0-9]/g, '-');
  const shortId = id.slice(0, 8);
  const filename = `Orion-${safeName}-${shortId}.zip`;

  const url = URL.createObjectURL(blob);
  const link = document.createElement('a');
  link.href = url;
  link.download = filename;
  document.body.appendChild(link);
  link.click();
  document.body.removeChild(link);
  URL.revokeObjectURL(url);
}
