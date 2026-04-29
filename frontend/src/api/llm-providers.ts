import { apiRequest } from './client';
import type {
  AgentLlmProviderOption,
  LlmProvider,
  LlmProviderTestResult,
  UpdateLlmProviderData,
} from '../types';

export async function listLlmProviders(): Promise<LlmProvider[]> {
  const response = await apiRequest<{ providers: LlmProvider[] }>(
    '/api/admin/llm-providers',
  );
  return response.providers;
}

export async function listAvailableLlmProviders(): Promise<AgentLlmProviderOption[]> {
  const response = await apiRequest<{ providers: AgentLlmProviderOption[] }>(
    '/api/llm/providers',
  );
  return response.providers;
}

export async function updateLlmProvider(
  provider: string,
  data: UpdateLlmProviderData,
): Promise<LlmProvider> {
  return apiRequest<LlmProvider>(`/api/admin/llm-providers/${provider}`, {
    method: 'PUT',
    body: JSON.stringify(data),
  });
}

export async function probeOllamaModels(baseUrl: string): Promise<string[]> {
  const response = await apiRequest<{ models: string[] }>(
    '/api/admin/llm-providers/ollama/probe',
    {
      method: 'POST',
      body: JSON.stringify({ base_url: baseUrl }),
    },
  );
  return response.models;
}

export async function testLlmProvider(
  provider: string,
  options: {
    model?: string;
    api_key?: string;
    base_url?: string;
    prompt?: string;
  } = {},
): Promise<LlmProviderTestResult> {
  return apiRequest<LlmProviderTestResult>(
    `/api/admin/llm-providers/${provider}/test`,
    {
      method: 'POST',
      body: JSON.stringify(options),
    },
  );
}
