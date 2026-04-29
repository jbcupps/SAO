import type { AgentLlmProviderOption, LlmProviderName } from '../types';

const PROVIDER_LABELS: Record<LlmProviderName, string> = {
  openai: 'OpenAI',
  anthropic: 'Anthropic',
  ollama: 'Ollama',
  grok: 'xAI Grok',
  gemini: 'Google Gemini',
};

export interface AgentLlmSelection {
  default_provider: string;
  default_id_model: string;
  default_ego_model: string;
}

function findProvider(
  providers: AgentLlmProviderOption[],
  provider: string,
): AgentLlmProviderOption | undefined {
  return providers.find((entry) => entry.provider === provider);
}

function chooseModel(
  provider: AgentLlmProviderOption | undefined,
  preferred: string,
): string {
  if (!provider) {
    return '';
  }

  if (preferred) {
    return preferred;
  }

  if (provider.default_model && provider.approved_models.includes(provider.default_model)) {
    return provider.default_model;
  }

  return provider.approved_models[0] ?? '';
}

export function buildAgentLlmSelection(
  providers: AgentLlmProviderOption[],
  provider: string,
  idModel: string,
  egoModel: string,
): AgentLlmSelection {
  const resolvedProvider =
    provider && findProvider(providers, provider)
      ? provider
      : providers[0]?.provider ?? '';
  const providerEntry = findProvider(providers, resolvedProvider);

  return {
    default_provider: resolvedProvider,
    default_id_model: chooseModel(providerEntry, idModel),
    default_ego_model: chooseModel(providerEntry, egoModel),
  };
}

export function AgentLlmFields({
  providers,
  provider,
  idModel,
  egoModel,
  disabled = false,
  onChange,
}: {
  providers: AgentLlmProviderOption[];
  provider: string;
  idModel: string;
  egoModel: string;
  disabled?: boolean;
  onChange: (selection: AgentLlmSelection) => void;
}) {
  const selectedProvider = findProvider(providers, provider);
  const modelOptions = selectedProvider?.approved_models ?? [];
  const idModelUnapproved =
    !!selectedProvider && !!idModel && !modelOptions.includes(idModel);
  const egoModelUnapproved =
    !!selectedProvider && !!egoModel && !modelOptions.includes(egoModel);

  if (providers.length === 0) {
    return (
      <div className="rounded-lg border border-yellow-800 bg-yellow-900/20 p-3 text-sm text-yellow-200">
        No enabled LLM providers are currently available for this agent.
      </div>
    );
  }

  return (
    <div className="space-y-4">
      <div className="flex flex-wrap gap-2">
        {providers.map((entry) => (
          <span
            key={entry.provider}
            className={`rounded-full px-3 py-1 text-xs ${
              entry.provider === provider
                ? 'bg-blue-500/20 text-blue-200'
                : 'bg-gray-700 text-gray-300'
            }`}
          >
            {PROVIDER_LABELS[entry.provider] ?? entry.provider} ·{' '}
            {entry.approved_models.length} model
            {entry.approved_models.length === 1 ? '' : 's'}
          </span>
        ))}
      </div>

      <div>
        <label className="block text-xs text-gray-400 mb-1">Provider</label>
        <select
          value={provider}
          disabled={disabled}
          onChange={(e) => {
            const nextSelection = buildAgentLlmSelection(
              providers,
              e.target.value,
              '',
              '',
            );
            onChange(nextSelection);
          }}
          className="w-full px-3 py-2 bg-gray-700 border border-gray-600 rounded-lg text-white focus:outline-none focus:border-blue-500 disabled:bg-gray-800 disabled:text-gray-500"
        >
          {providers.map((entry) => (
            <option key={entry.provider} value={entry.provider}>
              {PROVIDER_LABELS[entry.provider] ?? entry.provider}
            </option>
          ))}
        </select>
      </div>

      <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
        <div>
          <label className="block text-xs text-gray-400 mb-1">Id model</label>
          <select
            value={idModel}
            disabled={disabled || modelOptions.length === 0}
            onChange={(e) =>
              onChange({
                default_provider: provider,
                default_id_model: e.target.value,
                default_ego_model: egoModel,
              })
            }
            className="w-full px-3 py-2 bg-gray-700 border border-gray-600 rounded-lg text-white focus:outline-none focus:border-blue-500 disabled:bg-gray-800 disabled:text-gray-500"
          >
            {modelOptions.map((model) => (
              <option key={model} value={model}>
                {model}
              </option>
            ))}
          </select>
        </div>

        <div>
          <label className="block text-xs text-gray-400 mb-1">Ego model</label>
          <select
            value={egoModel}
            disabled={disabled || modelOptions.length === 0}
            onChange={(e) =>
              onChange({
                default_provider: provider,
                default_id_model: idModel,
                default_ego_model: e.target.value,
              })
            }
            className="w-full px-3 py-2 bg-gray-700 border border-gray-600 rounded-lg text-white focus:outline-none focus:border-blue-500 disabled:bg-gray-800 disabled:text-gray-500"
          >
            {modelOptions.map((model) => (
              <option key={model} value={model}>
                {model}
              </option>
            ))}
          </select>
        </div>
      </div>

      {selectedProvider && modelOptions.length === 0 && (
        <div className="rounded-lg border border-red-800 bg-red-900/20 p-3 text-sm text-red-200">
          This provider has no approved models. Ask an administrator to approve at least one model
          before saving this agent.
        </div>
      )}

      {(idModelUnapproved || egoModelUnapproved) && (
        <div className="rounded-lg border border-red-800 bg-red-900/20 p-3 text-sm text-red-200">
          The selected Id or Ego model is no longer approved for this provider. Choose an approved
          model before saving.
        </div>
      )}

      {selectedProvider && (
        <p className="text-xs text-gray-500">
          Active provider: {PROVIDER_LABELS[selectedProvider.provider] ?? selectedProvider.provider}
          . Default provider model: {selectedProvider.default_model ?? 'not set'}.
        </p>
      )}
    </div>
  );
}
