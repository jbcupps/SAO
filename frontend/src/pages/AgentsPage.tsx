import { useMemo, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import {
  listAgents,
  createAgent,
  deleteAgent,
  downloadAgentBundle,
} from '../api/agents';
import { listLlmProviders } from '../api/llm-providers';
import type { Agent, LlmProvider } from '../types';

export default function AgentsPage() {
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  const [showRegister, setShowRegister] = useState(false);
  const [newAgentName, setNewAgentName] = useState('');
  const [newProvider, setNewProvider] = useState('');
  const [newIdModel, setNewIdModel] = useState('');
  const [newEgoModel, setNewEgoModel] = useState('');
  const [error, setError] = useState('');
  const [creating, setCreating] = useState(false);
  const [downloadingId, setDownloadingId] = useState<string | null>(null);
  const [deletingId, setDeletingId] = useState<string | null>(null);
  const [confirmDeleteId, setConfirmDeleteId] = useState<string | null>(null);

  const { data: agents, isLoading } = useQuery({
    queryKey: ['agents'],
    queryFn: listAgents,
  });

  const { data: providers } = useQuery({
    queryKey: ['llm-providers'],
    queryFn: listLlmProviders,
    // Non-admins can't read this; treat 403 as "no providers" gracefully.
    retry: false,
  });

  const enabledProviders = useMemo<LlmProvider[]>(
    () => (providers ?? []).filter((p) => p.enabled),
    [providers],
  );

  const selectedProvider = useMemo(
    () => enabledProviders.find((p) => p.provider === newProvider),
    [enabledProviders, newProvider],
  );

  const handleRegister = async () => {
    setError('');
    if (!newAgentName.trim()) {
      setError('Agent name is required');
      return;
    }
    if (enabledProviders.length > 0) {
      if (!newProvider) {
        setError('Choose an LLM provider');
        return;
      }
      if (!newIdModel.trim() || !newEgoModel.trim()) {
        setError('Choose both Id and Ego models');
        return;
      }
    }

    setCreating(true);
    try {
      await createAgent({
        name: newAgentName.trim(),
        default_provider: newProvider || undefined,
        default_id_model: newIdModel.trim() || undefined,
        default_ego_model: newEgoModel.trim() || undefined,
      });
      await queryClient.invalidateQueries({ queryKey: ['agents'] });
      setNewAgentName('');
      setNewProvider('');
      setNewIdModel('');
      setNewEgoModel('');
      setShowRegister(false);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to register agent');
    } finally {
      setCreating(false);
    }
  };

  const handleDownload = async (agent: Agent) => {
    setError('');
    setDownloadingId(agent.id);
    try {
      await downloadAgentBundle(agent.id, agent.name);
    } catch (err) {
      setError(
        err instanceof Error ? err.message : 'Failed to download bundle',
      );
    } finally {
      setDownloadingId(null);
    }
  };

  const handleDelete = async (id: string) => {
    setError('');
    setDeletingId(id);
    try {
      await deleteAgent(id);
      queryClient.setQueryData<Agent[]>(['agents'], (current) =>
        current?.filter((agent) => agent.id !== id) ?? [],
      );
      await queryClient.invalidateQueries({ queryKey: ['agents'] });
      setConfirmDeleteId(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to delete agent');
    } finally {
      setDeletingId(null);
    }
  };

  const stateColor = (state: string) => {
    switch (state) {
      case 'active':
      case 'online':
        return 'bg-green-500';
      case 'inactive':
      case 'offline':
        return 'bg-gray-500';
      case 'error':
        return 'bg-red-500';
      default:
        return 'bg-yellow-500';
    }
  };

  return (
    <div>
      <div className="flex items-center justify-between mb-6">
        <h1 className="text-2xl font-bold text-white">Agents</h1>
        <button
          onClick={() => setShowRegister(true)}
          className="px-4 py-2 bg-blue-600 hover:bg-blue-700 text-white font-medium rounded-lg transition-colors text-sm"
        >
          + Register Agent
        </button>
      </div>

      {error && (
        <div className="mb-6 rounded-lg border border-red-800 bg-red-900/30 p-3">
          <p className="text-sm text-red-300">{error}</p>
        </div>
      )}

      {showRegister && (
        <div className="bg-gray-800 rounded-xl border border-gray-700 p-6 mb-6 space-y-4">
          <h2 className="text-lg font-semibold text-white">
            Register New Agent
          </h2>

          <div>
            <label className="block text-xs text-gray-400 mb-1">Name</label>
            <input
              type="text"
              value={newAgentName}
              onChange={(e) => setNewAgentName(e.target.value)}
              placeholder="abigail-main"
              className="w-full px-3 py-2 bg-gray-700 border border-gray-600 rounded-lg text-white placeholder-gray-500 focus:outline-none focus:border-blue-500"
            />
          </div>

          {enabledProviders.length === 0 ? (
            <div className="rounded-lg border border-yellow-800 bg-yellow-900/20 p-3 text-sm text-yellow-200">
              No LLM providers are enabled. Ask an administrator to configure one
              under <span className="font-mono">/admin/llm-providers</span>.
            </div>
          ) : (
            <>
              <div>
                <label className="block text-xs text-gray-400 mb-1">
                  LLM Provider
                </label>
                <select
                  value={newProvider}
                  onChange={(e) => {
                    setNewProvider(e.target.value);
                    const p = enabledProviders.find(
                      (x) => x.provider === e.target.value,
                    );
                    if (p?.default_model) {
                      setNewIdModel(p.default_model);
                      setNewEgoModel(p.default_model);
                    }
                  }}
                  className="w-full px-3 py-2 bg-gray-700 border border-gray-600 rounded-lg text-white focus:outline-none focus:border-blue-500"
                >
                  <option value="">Choose provider...</option>
                  {enabledProviders.map((p) => (
                    <option key={p.provider} value={p.provider}>
                      {p.provider}
                    </option>
                  ))}
                </select>
              </div>

              <div className="grid grid-cols-2 gap-3">
                <div>
                  <label className="block text-xs text-gray-400 mb-1">
                    Id model
                  </label>
                  <ModelInput
                    value={newIdModel}
                    onChange={setNewIdModel}
                    options={selectedProvider?.approved_models ?? []}
                  />
                </div>
                <div>
                  <label className="block text-xs text-gray-400 mb-1">
                    Ego model
                  </label>
                  <ModelInput
                    value={newEgoModel}
                    onChange={setNewEgoModel}
                    options={selectedProvider?.approved_models ?? []}
                  />
                </div>
              </div>
            </>
          )}

          <div className="flex gap-3">
            <button
              onClick={handleRegister}
              disabled={creating}
              className="px-4 py-2 bg-blue-600 hover:bg-blue-700 disabled:bg-blue-800 text-white font-medium rounded-lg transition-colors"
            >
              {creating ? 'Registering...' : 'Register'}
            </button>
            <button
              onClick={() => {
                setShowRegister(false);
                setNewAgentName('');
                setNewProvider('');
                setNewIdModel('');
                setNewEgoModel('');
                setError('');
              }}
              className="px-4 py-2 bg-gray-700 hover:bg-gray-600 text-gray-300 font-medium rounded-lg transition-colors"
            >
              Cancel
            </button>
          </div>
        </div>
      )}

      {isLoading ? (
        <div className="text-center py-16 text-gray-400">
          Loading agents...
        </div>
      ) : agents && agents.length > 0 ? (
        <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-4">
          {agents.map((agent: Agent) => (
            <div
              key={agent.id}
              className="bg-gray-800 rounded-xl border border-gray-700 p-5"
            >
              <div className="flex items-start justify-between mb-3">
                <div>
                  <h3 className="text-white font-semibold">{agent.name}</h3>
                  <p className="text-xs text-gray-500 font-mono mt-0.5">
                    {agent.id}
                  </p>
                </div>
                <div className="flex items-center gap-1.5">
                  <span
                    className={`w-2.5 h-2.5 rounded-full ${stateColor(agent.state)}`}
                  />
                  <span className="text-xs text-gray-400 capitalize">
                    {agent.state}
                  </span>
                </div>
              </div>

              {agent.default_provider && (
                <div className="mb-3 text-xs text-gray-300">
                  <span className="text-gray-500">LLM:</span>{' '}
                  <span className="font-mono">
                    {agent.default_provider} / {agent.default_id_model} /{' '}
                    {agent.default_ego_model}
                  </span>
                </div>
              )}

              {agent.capabilities.length > 0 && (
                <div className="flex flex-wrap gap-1.5 mb-3">
                  {agent.capabilities.map((cap) => (
                    <span
                      key={cap}
                      className="inline-block px-2 py-0.5 text-xs bg-gray-700 text-gray-300 rounded"
                    >
                      {cap}
                    </span>
                  ))}
                </div>
              )}

              <div className="flex flex-wrap gap-2 mt-3 pt-3 border-t border-gray-700">
                <button
                  onClick={() => handleDownload(agent)}
                  disabled={downloadingId === agent.id}
                  className="text-xs px-3 py-1.5 bg-blue-600 hover:bg-blue-700 disabled:bg-blue-900 text-white rounded transition-colors"
                >
                  {downloadingId === agent.id ? 'Building...' : 'Download bundle'}
                </button>
                <button
                  onClick={() => navigate(`/agents/${agent.id}/events`)}
                  className="text-xs px-3 py-1.5 bg-gray-700 hover:bg-gray-600 text-gray-200 rounded transition-colors"
                >
                  Logs
                </button>
                {confirmDeleteId === agent.id ? (
                  <div className="flex gap-2 ml-auto">
                    <button
                      onClick={() => handleDelete(agent.id)}
                      disabled={deletingId === agent.id}
                      className="text-xs px-3 py-1.5 bg-red-600 hover:bg-red-700 disabled:bg-red-900 text-white rounded transition-colors"
                    >
                      {deletingId === agent.id
                        ? 'Deleting...'
                        : 'Confirm delete'}
                    </button>
                    <button
                      onClick={() => setConfirmDeleteId(null)}
                      disabled={deletingId === agent.id}
                      className="text-xs px-3 py-1.5 bg-gray-700 hover:bg-gray-600 text-gray-300 rounded transition-colors"
                    >
                      Cancel
                    </button>
                  </div>
                ) : (
                  <button
                    onClick={() => {
                      setError('');
                      setConfirmDeleteId(agent.id);
                    }}
                    className="text-xs px-3 py-1.5 text-red-400 hover:text-red-300 ml-auto"
                  >
                    Delete
                  </button>
                )}
              </div>

              <div className="text-xs text-gray-500 mt-2">
                Created {new Date(agent.created_at).toLocaleDateString()}
              </div>
            </div>
          ))}
        </div>
      ) : (
        <div className="text-center py-16">
          <p className="text-gray-500 mb-4">No agents registered yet</p>
          <button
            onClick={() => setShowRegister(true)}
            className="px-4 py-2 bg-blue-600 hover:bg-blue-700 text-white font-medium rounded-lg transition-colors text-sm"
          >
            Register Your First Agent
          </button>
        </div>
      )}
    </div>
  );
}

function ModelInput({
  value,
  onChange,
  options,
}: {
  value: string;
  onChange: (v: string) => void;
  options: string[];
}) {
  const listId = `models-${Math.random().toString(36).slice(2, 9)}`;
  return (
    <>
      <input
        type="text"
        value={value}
        onChange={(e) => onChange(e.target.value)}
        list={options.length > 0 ? listId : undefined}
        placeholder="model name"
        className="w-full px-3 py-2 bg-gray-700 border border-gray-600 rounded-lg text-white placeholder-gray-500 focus:outline-none focus:border-blue-500"
      />
      {options.length > 0 && (
        <datalist id={listId}>
          {options.map((m) => (
            <option key={m} value={m} />
          ))}
        </datalist>
      )}
    </>
  );
}
