import { useEffect, useState } from 'react';
import { useNavigate, useParams } from 'react-router-dom';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { getAgent, listAgentEvents, updateAgent } from '../api/agents';
import { AgentLlmFields, buildAgentLlmSelection } from '../components/AgentLlmFields';

const PAGE_SIZE = 25;

function eventColor(type: string): string {
  switch (type) {
    case 'auditAction':
      return 'text-blue-300';
    case 'memoryEvent':
      return 'text-purple-300';
    case 'identitySync':
      return 'text-emerald-300';
    default:
      return 'text-gray-300';
  }
}

export default function AgentEventsPage() {
  const { id } = useParams<{ id: string }>();
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const [offset, setOffset] = useState(0);
  const [selection, setSelection] = useState({
    default_provider: '',
    default_id_model: '',
    default_ego_model: '',
  });
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState('');
  const [saved, setSaved] = useState(false);

  const { data: agent } = useQuery({
    queryKey: ['agent', id],
    queryFn: () => getAgent(id!),
    enabled: !!id,
  });

  const { data: events, isLoading } = useQuery({
    queryKey: ['agent-events', id, offset],
    queryFn: () => listAgentEvents(id!, PAGE_SIZE, offset),
    enabled: !!id,
    refetchInterval: 5000,
  });

  useEffect(() => {
    if (!agent?.available_llm_providers || agent.available_llm_providers.length === 0) {
      return;
    }

    setSelection(
      buildAgentLlmSelection(
        agent.available_llm_providers,
        agent.default_provider ?? '',
        agent.default_id_model ?? '',
        agent.default_ego_model ?? '',
      ),
    );
  }, [
    agent?.available_llm_providers,
    agent?.default_ego_model,
    agent?.default_id_model,
    agent?.default_provider,
  ]);

  if (!id) return null;

  const handleSave = async () => {
    setSaveError('');
    setSaved(false);
    setSaving(true);
    try {
      const updated = await updateAgent(id, selection);
      queryClient.setQueryData(['agent', id], updated);
      queryClient.invalidateQueries({ queryKey: ['agents'] });
      setSaved(true);
    } catch (err) {
      setSaveError(err instanceof Error ? err.message : 'Failed to update LLM defaults');
    } finally {
      setSaving(false);
    }
  };

  return (
    <div>
      <div className="flex items-center justify-between mb-4">
        <div>
          <button
            onClick={() => navigate('/agents')}
            className="text-xs text-gray-400 hover:text-white mb-2"
          >
            ← Agents
          </button>
          <h1 className="text-2xl font-bold text-white">
            Events — {agent ? agent.agent_id.slice(0, 8) : id.slice(0, 8)}
          </h1>
          {agent?.last_heartbeat && (
            <p className="text-xs text-gray-400 mt-1">
              Last heartbeat: {new Date(agent.last_heartbeat).toLocaleString()}
            </p>
          )}
        </div>
      </div>

      <div className="mb-6 rounded-xl border border-gray-700 bg-gray-800 p-5">
        <div className="mb-3">
          <h2 className="text-lg font-semibold text-white">Runtime LLM Selection</h2>
          <p className="text-xs text-gray-500 mt-1">
            Switch the agent to any enabled provider and approved model combination that SAO
            currently exposes.
          </p>
        </div>

        <AgentLlmFields
          providers={agent?.available_llm_providers ?? []}
          provider={selection.default_provider}
          idModel={selection.default_id_model}
          egoModel={selection.default_ego_model}
          disabled={saving}
          onChange={setSelection}
        />

        {saveError && (
          <div className="mt-3 rounded border border-red-800 bg-red-900/30 p-2 text-xs text-red-300">
            {saveError}
          </div>
        )}
        {saved && <div className="mt-3 text-xs text-green-400">LLM defaults updated.</div>}

        <div className="mt-4 flex gap-2">
          <button
            onClick={handleSave}
            disabled={saving || !(agent?.available_llm_providers?.length)}
            className="px-4 py-2 bg-blue-600 hover:bg-blue-700 disabled:bg-blue-900 text-white text-sm rounded-lg"
          >
            {saving ? 'Saving...' : 'Save LLM defaults'}
          </button>
          <button
            onClick={() => {
              if (!agent?.available_llm_providers) {
                return;
              }
              setSelection(
                buildAgentLlmSelection(
                  agent.available_llm_providers,
                  agent.default_provider ?? '',
                  agent.default_id_model ?? '',
                  agent.default_ego_model ?? '',
                ),
              );
              setSaved(false);
              setSaveError('');
            }}
            disabled={saving}
            className="px-4 py-2 bg-gray-700 hover:bg-gray-600 text-gray-300 text-sm rounded-lg"
          >
            Reset
          </button>
        </div>
      </div>

      {isLoading ? (
        <div className="text-gray-400">Loading...</div>
      ) : (events ?? []).length === 0 ? (
        <div className="text-center py-16 text-gray-500">
          No egress events yet. Once the entity is installed and chats with you, audit /
          memory / identitySync events will appear here.
        </div>
      ) : (
        <div className="bg-gray-800 rounded-xl border border-gray-700 overflow-hidden">
          <table className="w-full text-sm">
            <thead className="bg-gray-900 text-xs text-gray-400 uppercase">
              <tr>
                <th className="px-4 py-2 text-left">When</th>
                <th className="px-4 py-2 text-left">Type</th>
                <th className="px-4 py-2 text-left">Orion ID</th>
                <th className="px-4 py-2 text-left">Payload</th>
              </tr>
            </thead>
            <tbody>
              {(events ?? []).map((e) => (
                <tr
                  key={e.event_id}
                  className="border-t border-gray-700 hover:bg-gray-700/40"
                >
                  <td className="px-4 py-2 text-gray-300 whitespace-nowrap">
                    {new Date(e.created_at).toLocaleString()}
                  </td>
                  <td className={`px-4 py-2 font-mono ${eventColor(e.event_type)}`}>
                    {e.event_type}
                  </td>
                  <td className="px-4 py-2 font-mono text-xs text-gray-400">
                    {e.orion_id.slice(0, 8)}
                  </td>
                  <td className="px-4 py-2 text-xs text-gray-300">
                    <pre className="whitespace-pre-wrap break-all max-w-xl">
                      {JSON.stringify(e.payload, null, 2)}
                    </pre>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      <div className="flex items-center justify-between mt-4">
        <button
          onClick={() => setOffset(Math.max(0, offset - PAGE_SIZE))}
          disabled={offset === 0}
          className="px-3 py-1.5 text-xs bg-gray-700 hover:bg-gray-600 disabled:bg-gray-800 disabled:text-gray-600 text-gray-200 rounded"
        >
          Previous
        </button>
        <span className="text-xs text-gray-500">
          showing {offset + 1}–{offset + (events?.length ?? 0)}
        </span>
        <button
          onClick={() => setOffset(offset + PAGE_SIZE)}
          disabled={(events?.length ?? 0) < PAGE_SIZE}
          className="px-3 py-1.5 text-xs bg-gray-700 hover:bg-gray-600 disabled:bg-gray-800 disabled:text-gray-600 text-gray-200 rounded"
        >
          Next
        </button>
      </div>
    </div>
  );
}
