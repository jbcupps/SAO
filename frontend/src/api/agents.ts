import { apiRequest } from './client';
import type { Agent } from '../types';

export async function listAgents(): Promise<Agent[]> {
  return apiRequest<Agent[]>('/api/agents');
}

export async function createAgent(name: string): Promise<Agent> {
  return apiRequest<Agent>('/api/agents', {
    method: 'POST',
    body: JSON.stringify({ name }),
  });
}

export async function getAgent(id: string): Promise<Agent> {
  return apiRequest<Agent>(`/api/agents/${id}`);
}

export async function deleteAgent(id: string): Promise<void> {
  return apiRequest<void>(`/api/agents/${id}`, {
    method: 'DELETE',
  });
}
