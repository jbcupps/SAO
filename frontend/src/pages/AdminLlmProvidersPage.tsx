import { useEffect, useState } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import {
  listLlmProviders,
  probeOllamaModels,
  testLlmProvider,
  updateLlmProvider,
} from '../api/llm-providers';
import type {
  LlmProvider,
  LlmProviderName,
  LlmProviderTestResult,
} from '../types';

interface ProviderCatalogEntry {
  name: LlmProviderName;
  label: string;
  blurb: string;
  needsKey: boolean;
  needsBaseUrl: boolean;
  keyHint?: string;
  consoleUrl?: string;
  presetModels: string[];
  defaultModel: string;
}

const CATALOG: ProviderCatalogEntry[] = [
  {
    name: 'openai',
    label: 'OpenAI',
    blurb: 'GPT-4o family. Used by entities that need general-purpose chat or tool-use.',
    needsKey: true,
    needsBaseUrl: false,
    keyHint: 'sk-... — create at platform.openai.com/api-keys',
    consoleUrl: 'https://platform.openai.com/api-keys',
    presetModels: ['gpt-4o', 'gpt-4o-mini', 'gpt-4.1', 'gpt-4.1-mini'],
    defaultModel: 'gpt-4o-mini',
  },
  {
    name: 'anthropic',
    label: 'Anthropic Claude',
    blurb: 'Claude 4.x family. Strong reasoning, large context, computer-use compatible.',
    needsKey: true,
    needsBaseUrl: false,
    keyHint: 'sk-ant-... — create at console.anthropic.com/settings/keys',
    consoleUrl: 'https://console.anthropic.com/settings/keys',
    presetModels: [
      'claude-opus-4-7',
      'claude-sonnet-4-6',
      'claude-haiku-4-5-20251001',
    ],
    defaultModel: 'claude-haiku-4-5-20251001',
  },
  {
    name: 'grok',
    label: 'xAI Grok',
    blurb: 'Grok 4 family from xAI. OpenAI-compatible chat completions.',
    needsKey: true,
    needsBaseUrl: false,
    keyHint: 'xai-... — create at console.x.ai',
    consoleUrl: 'https://console.x.ai/',
    presetModels: ['grok-4-latest', 'grok-4-fast', 'grok-3'],
    defaultModel: 'grok-4-fast',
  },
  {
    name: 'gemini',
    label: 'Google Gemini',
    blurb: 'Gemini 2.x via the Generative Language API.',
    needsKey: true,
    needsBaseUrl: false,
    keyHint: 'AIza... — create at aistudio.google.com/apikey',
    consoleUrl: 'https://aistudio.google.com/apikey',
    presetModels: [
      'gemini-2.5-pro',
      'gemini-2.5-flash',
      'gemini-2.0-flash',
    ],
    defaultModel: 'gemini-2.5-flash',
  },
  {
    name: 'ollama',
    label: 'Ollama (local)',
    blurb:
      'Self-hosted models. SAO probes /api/tags on the URL you provide and proxies generations to it.',
    needsKey: false,
    needsBaseUrl: true,
    presetModels: ['llama3.2', 'qwen3:8b', 'mistral'],
    defaultModel: 'llama3.2',
  },
];

export default function AdminLlmProvidersPage() {
  const { data: providers, isLoading } = useQuery({
    queryKey: ['llm-providers'],
    queryFn: listLlmProviders,
  });

  return (
    <div>
      <h1 className="text-2xl font-bold text-white mb-2">LLM Providers</h1>
      <p className="text-sm text-gray-400 mb-6 max-w-3xl">
        Provider keys live in the SAO vault and never leave the server. Every OrionII entity
        proxies through SAO via{' '}
        <span className="font-mono text-gray-300">POST /api/llm/generate</span>; switching
        providers, rotating a key, or revoking a token applies instantly without redeploying any
        entity.
      </p>

      {isLoading ? (
        <div className="text-gray-400">Loading...</div>
      ) : (
        <div className="space-y-6">
          {CATALOG.map((entry) => (
            <ProviderCard
              key={entry.name}
              entry={entry}
              row={providers?.find((p) => p.provider === entry.name)}
            />
          ))}
        </div>
      )}
    </div>
  );
}

function ProviderCard({
  entry,
  row,
}: {
  entry: ProviderCatalogEntry;
  row: LlmProvider | undefined;
}) {
  const queryClient = useQueryClient();
  const [enabled, setEnabled] = useState(row?.enabled ?? false);
  const [baseUrl, setBaseUrl] = useState(row?.base_url ?? '');
  const [defaultModel, setDefaultModel] = useState(
    row?.default_model ?? entry.defaultModel,
  );
  const [approvedModels, setApprovedModels] = useState<string[]>(
    row?.approved_models?.length
      ? row.approved_models
      : entry.needsKey || entry.needsBaseUrl
        ? []
        : entry.presetModels,
  );
  const [apiKey, setApiKey] = useState('');
  const [discovered, setDiscovered] = useState<string[] | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [saved, setSaved] = useState(false);
  const [testResult, setTestResult] = useState<LlmProviderTestResult | null>(null);

  useEffect(() => {
    if (!row) return;
    setEnabled(row.enabled);
    setBaseUrl(row.base_url ?? '');
    setDefaultModel(row.default_model ?? entry.defaultModel);
    if (row.approved_models?.length) {
      setApprovedModels(row.approved_models);
    }
  }, [row, entry.defaultModel]);

  const togglePreset = (model: string) => {
    setApprovedModels((current) =>
      current.includes(model)
        ? current.filter((m) => m !== model)
        : [...current, model],
    );
  };

  const handleProbe = async () => {
    setError('');
    if (!baseUrl.trim()) {
      setError('Base URL is required');
      return;
    }
    setBusy(true);
    try {
      const models = await probeOllamaModels(baseUrl.trim());
      setDiscovered(models);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Probe failed');
    } finally {
      setBusy(false);
    }
  };

  const handleSave = async () => {
    setError('');
    setSaved(false);
    setTestResult(null);
    setBusy(true);
    try {
      await updateLlmProvider(entry.name, {
        enabled,
        base_url: entry.needsBaseUrl ? baseUrl.trim() : null,
        approved_models: approvedModels,
        default_model: defaultModel.trim() || null,
        api_key: apiKey.trim() ? apiKey.trim() : undefined,
      });
      setApiKey('');
      setSaved(true);
      await queryClient.invalidateQueries({ queryKey: ['llm-providers'] });
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Save failed');
    } finally {
      setBusy(false);
    }
  };

  const handleTest = async () => {
    setError('');
    setSaved(false);
    setTestResult(null);
    setBusy(true);
    try {
      const result = await testLlmProvider(entry.name, defaultModel.trim() || undefined);
      setTestResult(result);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Test failed');
    } finally {
      setBusy(false);
    }
  };

  const stored = row?.has_api_key;

  return (
    <div className="bg-gray-800 rounded-xl border border-gray-700 p-6 space-y-4">
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-lg font-semibold text-white">{entry.label}</h2>
          <p className="text-xs text-gray-400 mt-0.5 max-w-2xl">{entry.blurb}</p>
        </div>
        <label className="flex items-center gap-2 text-sm text-gray-300">
          <input
            type="checkbox"
            checked={enabled}
            onChange={(e) => setEnabled(e.target.checked)}
          />
          Enabled
        </label>
      </div>

      {entry.needsBaseUrl && (
        <div>
          <label className="block text-xs text-gray-400 mb-1">Base URL</label>
          <div className="flex gap-2">
            <input
              type="text"
              value={baseUrl}
              onChange={(e) => setBaseUrl(e.target.value)}
              placeholder="http://host.docker.internal:11434"
              className="flex-1 px-3 py-2 bg-gray-700 border border-gray-600 rounded-lg text-white placeholder-gray-500 focus:outline-none focus:border-blue-500"
            />
            <button
              onClick={handleProbe}
              disabled={busy}
              className="px-4 py-2 bg-gray-700 hover:bg-gray-600 text-white text-sm rounded-lg"
            >
              Refresh models
            </button>
          </div>
        </div>
      )}

      {entry.needsKey && (
        <div>
          <label className="block text-xs text-gray-400 mb-1">
            API key{' '}
            {stored ? (
              <span className="text-green-400">(stored — leave blank to keep)</span>
            ) : (
              <span className="text-yellow-400">(required)</span>
            )}
          </label>
          <input
            type="password"
            value={apiKey}
            onChange={(e) => setApiKey(e.target.value)}
            placeholder={entry.keyHint ?? 'paste API key'}
            autoComplete="off"
            className="w-full px-3 py-2 bg-gray-700 border border-gray-600 rounded-lg text-white placeholder-gray-500 focus:outline-none focus:border-blue-500"
          />
          {entry.consoleUrl && (
            <p className="text-xs text-gray-500 mt-1">
              Get a key:{' '}
              <a
                href={entry.consoleUrl}
                target="_blank"
                rel="noreferrer"
                className="text-blue-400 hover:underline"
              >
                {entry.consoleUrl}
              </a>
            </p>
          )}
        </div>
      )}

      <div>
        <label className="block text-xs text-gray-400 mb-1">
          Approved models — entities can only use models on this list
        </label>
        <div className="flex flex-wrap gap-2 mb-2">
          {entry.presetModels.map((m) => (
            <label
              key={m}
              className="flex items-center gap-2 px-2 py-1 bg-gray-700 rounded text-xs text-gray-200"
            >
              <input
                type="checkbox"
                checked={approvedModels.includes(m)}
                onChange={() => togglePreset(m)}
              />
              <span className="font-mono">{m}</span>
            </label>
          ))}
        </div>
        {discovered && (
          <div className="mb-2">
            <p className="text-xs text-gray-500 mb-1">Discovered on Ollama:</p>
            <div className="flex flex-wrap gap-2">
              {discovered.length === 0 && (
                <span className="text-xs text-gray-500">Ollama returned no models</span>
              )}
              {discovered.map((m) => (
                <label
                  key={m}
                  className="flex items-center gap-2 px-2 py-1 bg-gray-700 rounded text-xs text-gray-200"
                >
                  <input
                    type="checkbox"
                    checked={approvedModels.includes(m)}
                    onChange={() => togglePreset(m)}
                  />
                  <span className="font-mono">{m}</span>
                </label>
              ))}
            </div>
          </div>
        )}
        <input
          type="text"
          value={approvedModels.join(', ')}
          onChange={(e) =>
            setApprovedModels(
              e.target.value
                .split(',')
                .map((s) => s.trim())
                .filter(Boolean),
            )
          }
          placeholder="comma-separated override (e.g., custom fine-tunes)"
          className="w-full px-3 py-2 bg-gray-700 border border-gray-600 rounded-lg text-white placeholder-gray-500 focus:outline-none focus:border-blue-500 text-xs font-mono"
        />
      </div>

      <div>
        <label className="block text-xs text-gray-400 mb-1">Default model</label>
        <input
          type="text"
          value={defaultModel}
          onChange={(e) => setDefaultModel(e.target.value)}
          className="w-full px-3 py-2 bg-gray-700 border border-gray-600 rounded-lg text-white placeholder-gray-500 focus:outline-none focus:border-blue-500 font-mono"
        />
      </div>

      {error && (
        <div className="rounded border border-red-800 bg-red-900/30 p-2 text-xs text-red-300">
          {error}
        </div>
      )}
      {saved && <div className="text-xs text-green-400">Saved.</div>}
      {testResult && (
        <div
          className={`rounded border p-2 text-xs ${
            testResult.ok
              ? 'border-green-800 bg-green-900/30 text-green-200'
              : 'border-red-800 bg-red-900/30 text-red-200'
          }`}
        >
          {testResult.ok ? (
            <>
              <div>
                ✓ {entry.label} replied in {testResult.latency_ms}ms via{' '}
                <span className="font-mono">{testResult.model}</span>
              </div>
              {testResult.preview && (
                <div className="mt-1 text-gray-300">
                  preview: <span className="italic">{testResult.preview}</span>
                </div>
              )}
            </>
          ) : (
            <>
              ✗ Test failed ({testResult.latency_ms}ms): {testResult.error}
            </>
          )}
        </div>
      )}

      <div className="flex justify-end gap-2">
        <button
          onClick={handleTest}
          disabled={busy || (entry.needsKey && !stored && !apiKey.trim())}
          title={
            entry.needsKey && !stored && !apiKey.trim()
              ? 'Save an API key first, then test.'
              : 'Send a tiny ping prompt to verify connectivity.'
          }
          className="px-4 py-2 bg-gray-700 hover:bg-gray-600 disabled:bg-gray-800 disabled:text-gray-500 text-white text-sm rounded-lg"
        >
          {busy ? 'Working...' : 'Test connection'}
        </button>
        <button
          onClick={handleSave}
          disabled={busy}
          className="px-4 py-2 bg-blue-600 hover:bg-blue-700 disabled:bg-blue-900 text-white text-sm rounded-lg"
        >
          {busy ? 'Saving...' : 'Save'}
        </button>
      </div>
    </div>
  );
}
