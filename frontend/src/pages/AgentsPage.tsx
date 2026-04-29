import { useEffect, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import {
  createAgent,
  deleteAgent,
  downloadAgentBundle,
  updateAgent,
} from '../api/agents';
import { listAvailableLlmProviders } from '../api/llm-providers';
import { AgentLlmFields, buildAgentLlmSelection } from '../components/AgentLlmFields';
import { listAgents } from '../api/agents';
import type {
  Agent,
  AgentLlmProviderOption,
  AgentStatusResponse,
} from '../types';

type AgentLlmSelection = ReturnType<typeof buildAgentLlmSelection>;

const EMPTY_SELECTION: AgentLlmSelection = {
  default_provider: '',
  default_id_model: '',
  default_ego_model: '',
};

export default function AgentsPage() {
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  const [showRegister, setShowRegister] = useState(false);
  const [newAgentName, setNewAgentName] = useState('');
  const [newSelection, setNewSelection] = useState<AgentLlmSelection>(EMPTY_SELECTION);
  const [error, setError] = useState('');
  const [creating, setCreating] = useState(false);
  const [downloadingId, setDownloadingId] = useState<string | null>(null);
  const [deletingId, setDeletingId] = useState<string | null>(null);
  const [confirmDeleteId, setConfirmDeleteId] = useState<string | null>(null);

  const { data: agents, isLoading } = useQuery({
    queryKey: ['agents'],
    queryFn: listAgents,
  });

  const { data: providers, isLoading: providersLoading } = useQuery({
    queryKey: ['available-llm-providers'],
    queryFn: listAvailableLlmProviders,
    retry: false,
  });

  const availableProviders = providers ?? [];

  useEffect(() => {
    if (!showRegister || availableProviders.length === 0 || newSelection.default_provider) {
      return;
    }

    setNewSelection(buildAgentLlmSelection(availableProviders, '', '', ''));
  }, [availableProviders, newSelection.default_provider, showRegister]);

  const handleRegister = async () => {
    setError('');
    if (!newAgentName.trim()) {
      setError('Agent name is required');
      return;
    }
    if (availableProviders.length > 0) {
      if (!newSelection.default_provider) {
        setError('Choose an LLM provider');
        return;
      }
      if (!newSelection.default_id_model || !newSelection.default_ego_model) {
        setError('Choose both Id and Ego models');
        return;
      }
    }

    setCreating(true);
    try {
      await createAgent({
        name: newAgentName.trim(),
        default_provider: newSelection.default_provider || undefined,
        default_id_model: newSelection.default_id_model || undefined,
        default_ego_model: newSelection.default_ego_model || undefined,
      });
      await queryClient.invalidateQueries({ queryKey: ['agents'] });
      setNewAgentName('');
      setNewSelection(EMPTY_SELECTION);
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
      setError(err instanceof Error ? err.message : 'Failed to download bundle');
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

  const handleAgentUpdated = (agentId: string, updated: AgentStatusResponse) => {
    queryClient.setQueryData<Agent[]>(['agents'], (current) =>
      current?.map((agent) =>
        agent.id === agentId
          ? {
              ...agent,
              default_provider: updated.default_provider ?? null,
              default_id_model: updated.default_id_model ?? null,
              default_ego_model: updated.default_ego_model ?? null,
            }
          : agent,
      ) ?? [],
    );
    queryClient.setQueryData(['agent', agentId], updated);
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
          <h2 className="text-lg font-semibold text-white">Register New Agent</h2>

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

          {providersLoading ? (
            <div className="text-sm text-gray-400">Loading LLM provider options...</div>
          ) : availableProviders.length === 0 ? (
            <div className="rounded-lg border border-yellow-800 bg-yellow-900/20 p-3 text-sm text-yellow-200">
              No enabled LLM providers are available. Ask an administrator to configure one
              under <span className="font-mono">/admin/llm-providers</span>.
            </div>
          ) : (
            <div className="rounded-xl border border-gray-700 bg-gray-900/40 p-4">
              <div className="mb-3">
                <h3 className="text-sm font-medium text-white">LLM Runtime Defaults</h3>
                <p className="text-xs text-gray-500 mt-1">
                  Every enabled provider stays available to the agent, and this form shows
                  the approved models for each one. Pick the starting provider and the
                  default Id and Ego models now, then switch them later from the agent
                  controls.
                </p>
              </div>

              <AgentLlmFields
                providers={availableProviders}
                provider={newSelection.default_provider}
                idModel={newSelection.default_id_model}
                egoModel={newSelection.default_ego_model}
                disabled={creating}
                onChange={setNewSelection}
              />
            </div>
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
                setNewSelection(EMPTY_SELECTION);
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
        <div className="text-center py-16 text-gray-400">Loading agents...</div>
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
                  <p className="text-xs text-gray-500 font-mono mt-0.5">{agent.id}</p>
                </div>
                <div className="flex items-center gap-1.5">
                  <span className={`w-2.5 h-2.5 rounded-full ${stateColor(agent.state)}`} />
                  <span className="text-xs text-gray-400 capitalize">{agent.state}</span>
                </div>
              </div>

              <AgentLlmEditorCard
                agent={agent}
                providers={availableProviders}
                onUpdated={(updated) => handleAgentUpdated(agent.id, updated)}
              />

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
                      {deletingId === agent.id ? 'Deleting...' : 'Confirm delete'}
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

function AgentLlmEditorCard({
  agent,
  providers,
  onUpdated,
}: {
  agent: Agent;
  providers: AgentLlmProviderOption[];
  onUpdated: (agent: AgentStatusResponse) => void;
}) {
  const [editing, setEditing] = useState(false);
  const [selection, setSelection] = useState<AgentLlmSelection>(EMPTY_SELECTION);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState('');
  const [saved, setSaved] = useState(false);

  useEffect(() => {
    if (providers.length === 0) {
      setSelection(EMPTY_SELECTION);
      return;
    }

    setSelection(
      buildAgentLlmSelection(
        providers,
        agent.default_provider ?? '',
        agent.default_id_model ?? '',
        agent.default_ego_model ?? '',
      ),
    );
  }, [
    agent.default_ego_model,
    agent.default_id_model,
    agent.default_provider,
    providers,
  ]);

  const handleSave = async () => {
    setError('');
    setSaved(false);
    setSaving(true);
    try {
      const updated = await updateAgent(agent.id, selection);
      onUpdated(updated);
      setSaved(true);
      setEditing(false);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to update LLM settings');
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="mb-4 rounded-xl border border-gray-700 bg-gray-900/40 p-4">
      <div className="flex items-start justify-between gap-3">
        <div>
          <div className="text-xs uppercase tracking-wide text-gray-500">LLM Runtime</div>
          {agent.default_provider ? (
            <div className="mt-1 text-sm text-gray-200">
              <span className="text-gray-500">Active:</span>{' '}
              <span className="font-mono">
                {agent.default_provider} / {agent.default_id_model} / {agent.default_ego_model}
              </span>
            </div>
          ) : (
            <div className="mt-1 text-sm text-yellow-200">No default LLM configured yet.</div>
          )}
        </div>
        <button
          onClick={() => {
            setEditing((current) => !current);
            setSaved(false);
            setError('');
          }}
          className="text-xs px-3 py-1.5 bg-gray-700 hover:bg-gray-600 text-gray-200 rounded transition-colors"
        >
          {editing ? 'Close' : 'Switch LLM'}
        </button>
      </div>

      <p className="mt-2 text-xs text-gray-500">
        Provider and model defaults can be changed without rebuilding the agent record.
      </p>

      {editing && (
        <div className="mt-4 space-y-3">
          <AgentLlmFields
            providers={providers}
            provider={selection.default_provider}
            idModel={selection.default_id_model}
            egoModel={selection.default_ego_model}
            disabled={saving}
            onChange={setSelection}
          />

          {error && (
            <div className="rounded border border-red-800 bg-red-900/30 p-2 text-xs text-red-300">
              {error}
            </div>
          )}
          {saved && <div className="text-xs text-green-400">LLM defaults updated.</div>}

          <div className="flex gap-2">
            <button
              onClick={handleSave}
              disabled={saving || providers.length === 0}
              className="px-3 py-1.5 bg-blue-600 hover:bg-blue-700 disabled:bg-blue-900 text-white text-xs rounded-lg"
            >
              {saving ? 'Saving...' : 'Save LLM defaults'}
            </button>
            <button
              onClick={() => {
                setSelection(
                  buildAgentLlmSelection(
                    providers,
                    agent.default_provider ?? '',
                    agent.default_id_model ?? '',
                    agent.default_ego_model ?? '',
                  ),
                );
                setEditing(false);
                setSaved(false);
                setError('');
              }}
              disabled={saving}
              className="px-3 py-1.5 bg-gray-700 hover:bg-gray-600 text-gray-300 text-xs rounded-lg"
            >
              Cancel
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
