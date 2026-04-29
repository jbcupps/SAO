import { useEffect, useMemo, useState } from 'react';
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
import {
  buildModelCatalog,
  deriveManualModels,
  normalizeModelList,
  prepareModelSelection,
} from './adminLlmProviderModels';

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
    blurb:
      'Frontier GPT-5.x, reasoning, and GPT-4.1 family models for general chat, coding, and tool use.',
    needsKey: true,
    needsBaseUrl: false,
    keyHint: 'sk-... — create at platform.openai.com/api-keys',
    consoleUrl: 'https://platform.openai.com/api-keys',
    presetModels: [
      'gpt-5.5',
      'gpt-5.4',
      'gpt-5.4-mini',
      'gpt-5.4-nano',
      'gpt-5',
      'gpt-5-mini',
      'gpt-5-nano',
      'o3',
      'o4-mini',
      'gpt-4.1',
      'gpt-4.1-mini',
      'gpt-4.1-nano',
      'gpt-4o',
      'gpt-4o-mini',
    ],
    defaultModel: 'gpt-5.4-mini',
  },
  {
    name: 'anthropic',
    label: 'Anthropic Claude',
    blurb:
      'Claude frontier and balanced text models, including Opus 4.1 and Sonnet 4 aliases.',
    needsKey: true,
    needsBaseUrl: false,
    keyHint: 'sk-ant-... — create at console.anthropic.com/settings/keys',
    consoleUrl: 'https://console.anthropic.com/settings/keys',
    presetModels: [
      'claude-opus-4-1',
      'claude-opus-4-1-20250805',
      'claude-opus-4-0',
      'claude-sonnet-4-0',
      'claude-sonnet-4-20250514',
      'claude-3-7-sonnet-latest',
      'claude-3-5-haiku-latest',
    ],
    defaultModel: 'claude-sonnet-4-0',
  },
  {
    name: 'grok',
    label: 'xAI Grok',
    blurb: 'Grok 4.20 family from xAI, including reasoning and multi-agent variants.',
    needsKey: true,
    needsBaseUrl: false,
    keyHint: 'xai-... — create at console.x.ai',
    consoleUrl: 'https://console.x.ai/',
    presetModels: [
      'grok-4.20',
      'grok-4.20-reasoning',
      'grok-4.20-multi-agent',
      'grok-4-1-fast',
      'grok-3',
    ],
    defaultModel: 'grok-4.20-reasoning',
  },
  {
    name: 'gemini',
    label: 'Google Gemini',
    blurb:
      'Gemini frontier preview and stable text models exposed through the Generative Language API.',
    needsKey: true,
    needsBaseUrl: false,
    keyHint: 'AIza... — create at aistudio.google.com/apikey',
    consoleUrl: 'https://aistudio.google.com/apikey',
    presetModels: [
      'gemini-3-pro-preview',
      'gemini-3-flash-preview',
      'gemini-2.5-pro',
      'gemini-2.5-flash',
      'gemini-2.5-flash-lite',
      'gemini-2.0-flash',
      'gemini-2.0-flash-lite',
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
  const initialSelection = prepareModelSelection(
    row?.approved_models ?? [],
    row?.default_model,
  );
  const [enabled, setEnabled] = useState(row?.enabled ?? false);
  const [baseUrl, setBaseUrl] = useState(row?.base_url ?? '');
  const [defaultModel, setDefaultModel] = useState(initialSelection.defaultModel);
  const [approvedModels, setApprovedModels] = useState<string[]>(
    initialSelection.approvedModels,
  );
  const [manualModels, setManualModels] = useState<string[]>(
    deriveManualModels(
      entry.presetModels,
      initialSelection.approvedModels,
      initialSelection.defaultModel,
    ),
  );
  const [apiKey, setApiKey] = useState('');
  const [customModelInput, setCustomModelInput] = useState('');
  const [discovered, setDiscovered] = useState<string[] | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [saved, setSaved] = useState(false);
  const [testResult, setTestResult] = useState<LlmProviderTestResult | null>(null);

  const modelCatalog = useMemo(
    () =>
      buildModelCatalog({
        presetModels: entry.presetModels,
        discoveredModels: discovered ?? [],
        manualModels,
        approvedModels,
        defaultModel,
      }),
    [approvedModels, defaultModel, discovered, entry.presetModels, manualModels],
  );

  useEffect(() => {
    const nextSelection = prepareModelSelection(
      row?.approved_models ?? [],
      row?.default_model,
    );
    setEnabled(row?.enabled ?? false);
    setBaseUrl(row?.base_url ?? '');
    setApprovedModels(nextSelection.approvedModels);
    setDefaultModel(nextSelection.defaultModel);
    setManualModels(
      deriveManualModels(
        entry.presetModels,
        nextSelection.approvedModels,
        nextSelection.defaultModel,
      ),
    );
  }, [entry.presetModels, row]);

  const toggleApprovedModel = (model: string) => {
    const isApproved = approvedModels.includes(model);
    const nextApprovedModels = isApproved
      ? approvedModels.filter((current) => current !== model)
      : normalizeModelList([...approvedModels, model]);

    setApprovedModels(nextApprovedModels);

    if (isApproved && defaultModel === model) {
      setDefaultModel(nextApprovedModels[0] ?? '');
      return;
    }

    if (!isApproved && !defaultModel) {
      setDefaultModel(model);
    }
  };

  const selectDefaultModel = (model: string) => {
    if (!approvedModels.includes(model)) {
      setApprovedModels(normalizeModelList([...approvedModels, model]));
    }
    setDefaultModel(model);
  };

  const removeCustomModel = (model: string) => {
    const nextManualModels = manualModels.filter((current) => current !== model);
    const nextApprovedModels = approvedModels.filter((current) => current !== model);

    setManualModels(nextManualModels);
    setApprovedModels(nextApprovedModels);

    if (defaultModel === model) {
      setDefaultModel(nextApprovedModels[0] ?? '');
    }
  };

  const addCustomModels = () => {
    const nextCustomModels = normalizeModelList(customModelInput.split(','));

    if (nextCustomModels.length === 0) {
      return;
    }

    setManualModels((current) => normalizeModelList([...current, ...nextCustomModels]));
    setApprovedModels((current) => normalizeModelList([...current, ...nextCustomModels]));
    if (!defaultModel) {
      setDefaultModel(nextCustomModels[0]);
    }
    setCustomModelInput('');
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
      setDiscovered(normalizeModelList(models));
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
    const preparedSelection = prepareModelSelection(approvedModels, defaultModel);

    if (enabled && preparedSelection.approvedModels.length === 0) {
      setError(
        'Select at least one approved model before enabling this provider.',
      );
      return;
    }

    if (enabled && !preparedSelection.defaultModel) {
      setError('Choose a default model before enabling this provider.');
      return;
    }

    setBusy(true);
    try {
      await updateLlmProvider(entry.name, {
        enabled,
        base_url: entry.needsBaseUrl ? baseUrl.trim() : null,
        approved_models: preparedSelection.approvedModels,
        default_model: preparedSelection.defaultModel || null,
        api_key: apiKey.trim() ? apiKey.trim() : undefined,
      });
      setApprovedModels(preparedSelection.approvedModels);
      setDefaultModel(preparedSelection.defaultModel);
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
      const modelToTest = defaultModel.trim() || approvedModels[0]?.trim() || undefined;
      const result = await testLlmProvider(entry.name, {
        model: modelToTest,
        api_key: entry.needsKey && apiKey.trim() ? apiKey.trim() : undefined,
        base_url: entry.needsBaseUrl && baseUrl.trim() ? baseUrl.trim() : undefined,
      });
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
          <p className="text-xs text-gray-500 mt-1">
            Test connection uses the value currently typed here. Save stores it in the vault.
          </p>
        </div>
      )}

      <div className="rounded-xl border border-gray-700 bg-gray-900/40 p-4 space-y-4">
        <label className="block text-xs text-gray-400 mb-1">
          Approved models — entities can only use models on this list
        </label>
        <div className="flex flex-wrap gap-2">
          <span className="rounded-full bg-gray-800 px-3 py-1 text-xs text-gray-300">
            {approvedModels.length} approved
          </span>
          <span className="rounded-full bg-gray-800 px-3 py-1 text-xs text-gray-300">
            Default: <span className="font-mono">{defaultModel || 'not selected'}</span>
          </span>
          <span className="rounded-full bg-gray-800 px-3 py-1 text-xs text-gray-300">
            Suggested default: <span className="font-mono">{entry.defaultModel}</span>
          </span>
          {entry.needsBaseUrl && discovered && (
            <span className="rounded-full bg-gray-800 px-3 py-1 text-xs text-gray-300">
              Discovered: {discovered.length}
            </span>
          )}
        </div>

        <div className="rounded-lg border border-gray-700 bg-gray-800/50 p-3">
          <label className="block text-xs text-gray-400 mb-2">Add custom models</label>
          <div className="flex gap-2">
            <input
              type="text"
              value={customModelInput}
              onChange={(e) => setCustomModelInput(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'Enter') {
                  e.preventDefault();
                  addCustomModels();
                }
              }}
              placeholder="comma-separated fine-tunes, dated revisions, or local tags"
              className="flex-1 px-3 py-2 bg-gray-700 border border-gray-600 rounded-lg text-white placeholder-gray-500 focus:outline-none focus:border-blue-500 text-sm font-mono"
            />
            <button
              onClick={addCustomModels}
              disabled={busy || !customModelInput.trim()}
              className="px-4 py-2 bg-gray-700 hover:bg-gray-600 disabled:bg-gray-800 disabled:text-gray-500 text-white text-sm rounded-lg"
            >
              Add
            </button>
          </div>
          <p className="mt-2 text-xs text-gray-500">
            Approve multiple models per provider, then mark one of the approved models as
            the default for tests and new agent defaults.
          </p>
        </div>

        {modelCatalog.allModels.length === 0 ? (
          <div className="rounded-lg border border-dashed border-gray-700 p-4 text-sm text-gray-500">
            No models are listed yet. Add custom models or, for Ollama, probe the endpoint to
            discover them.
          </div>
        ) : (
          <div className="space-y-4">
            {modelCatalog.presetModels.length > 0 && (
              <ModelSection
                providerName={entry.name}
                heading="Provider catalog"
                caption="Curated frontier and production-ready models for this provider"
                models={modelCatalog.presetModels}
                approvedModels={approvedModels}
                defaultModel={defaultModel}
                suggestedDefault={entry.defaultModel}
                onToggleApproved={toggleApprovedModel}
                onSelectDefault={selectDefaultModel}
              />
            )}
            {modelCatalog.discoveredModels.length > 0 && (
              <ModelSection
                providerName={entry.name}
                heading="Discovered models"
                caption="Returned by the provider endpoint right now"
                models={modelCatalog.discoveredModels}
                approvedModels={approvedModels}
                defaultModel={defaultModel}
                suggestedDefault={entry.defaultModel}
                onToggleApproved={toggleApprovedModel}
                onSelectDefault={selectDefaultModel}
              />
            )}
            {modelCatalog.customModels.length > 0 && (
              <ModelSection
                providerName={entry.name}
                heading="Custom models"
                caption="Manual additions such as fine-tunes, dated revisions, or local tags"
                models={modelCatalog.customModels}
                approvedModels={approvedModels}
                defaultModel={defaultModel}
                suggestedDefault={entry.defaultModel}
                onToggleApproved={toggleApprovedModel}
                onSelectDefault={selectDefaultModel}
                onRemoveModel={removeCustomModel}
              />
            )}
          </div>
        )}

        {approvedModels.length === 0 && (
          <div className="rounded-lg border border-yellow-800 bg-yellow-900/20 p-3 text-xs text-yellow-100">
            No approved models are selected yet. Enabled providers must have at least one approved
            model.
          </div>
        )}

        {approvedModels.length > 0 && !defaultModel && (
          <div className="rounded-lg border border-blue-800 bg-blue-900/20 p-3 text-xs text-blue-100">
            Choose one approved model as the default to make connection tests and new agent
            defaults more predictable.
          </div>
        )}
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

function ModelSection({
  providerName,
  heading,
  caption,
  models,
  approvedModels,
  defaultModel,
  suggestedDefault,
  onToggleApproved,
  onSelectDefault,
  onRemoveModel,
}: {
  providerName: string;
  heading: string;
  caption: string;
  models: string[];
  approvedModels: string[];
  defaultModel: string;
  suggestedDefault: string;
  onToggleApproved: (model: string) => void;
  onSelectDefault: (model: string) => void;
  onRemoveModel?: (model: string) => void;
}) {
  return (
    <div>
      <div className="mb-2">
        <p className="text-sm font-medium text-white">{heading}</p>
        <p className="text-xs text-gray-500">{caption}</p>
      </div>
      <div className="grid gap-3 md:grid-cols-2 xl:grid-cols-3">
        {models.map((model) => {
          const isApproved = approvedModels.includes(model);
          const isDefault = defaultModel === model;
          const isSuggestedDefault = suggestedDefault === model;

          return (
            <div
              key={model}
              className={`rounded-lg border p-3 ${
                isApproved
                  ? 'border-blue-700 bg-blue-900/10'
                  : 'border-gray-700 bg-gray-800/60'
              }`}
            >
              <div className="flex items-start justify-between gap-3">
                <div className="min-w-0">
                  <div className="font-mono text-sm text-white break-all">{model}</div>
                  <div className="mt-2 flex flex-wrap gap-1">
                    {isDefault && (
                      <span className="rounded-full bg-blue-500/20 px-2 py-0.5 text-[11px] text-blue-200">
                        Default
                      </span>
                    )}
                    {isSuggestedDefault && !isDefault && (
                      <span className="rounded-full bg-gray-700 px-2 py-0.5 text-[11px] text-gray-300">
                        Suggested
                      </span>
                    )}
                    <span
                      className={`rounded-full px-2 py-0.5 text-[11px] ${
                        isApproved
                          ? 'bg-green-500/20 text-green-200'
                          : 'bg-gray-700 text-gray-400'
                      }`}
                    >
                      {isApproved ? 'Approved' : 'Not approved'}
                    </span>
                  </div>
                </div>
                {onRemoveModel && (
                  <button
                    onClick={() => onRemoveModel(model)}
                    className="text-xs text-gray-400 hover:text-red-300"
                    title="Remove custom model"
                  >
                    Remove
                  </button>
                )}
              </div>

              <div className="mt-4 flex flex-wrap gap-4">
                <label className="flex items-center gap-2 text-xs text-gray-200">
                  <input
                    type="checkbox"
                    checked={isApproved}
                    onChange={() => onToggleApproved(model)}
                  />
                  Approved
                </label>
                <label
                  className={`flex items-center gap-2 text-xs ${
                    isApproved ? 'text-gray-200' : 'text-gray-500'
                  }`}
                >
                  <input
                    type="radio"
                    name={`${providerName}-default-model`}
                    checked={isDefault}
                    disabled={!isApproved}
                    onChange={() => onSelectDefault(model)}
                  />
                  Default
                </label>
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}
