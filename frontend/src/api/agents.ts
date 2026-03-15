import { apiRequest } from './client';
import type { Agent, AgentBirthResponse, AgentStatusResponse } from '../types';

export async function listAgents(): Promise<Agent[]> {
  const response = await apiRequest<{ agents: Agent[] }>('/api/agents');
  return response.agents;
}

export async function createAgent(name: string): Promise<AgentBirthResponse> {
  return apiRequest<AgentBirthResponse>('/api/agents', {
    method: 'POST',
    body: JSON.stringify({ name }),
  });
}

export async function getAgent(id: string): Promise<AgentStatusResponse> {
  return apiRequest<AgentStatusResponse>(`/api/agents/${id}`);
}

export async function deleteAgent(id: string): Promise<void> {
  return apiRequest<void>(`/api/agents/${id}`, {
    method: 'DELETE',
  });
}
