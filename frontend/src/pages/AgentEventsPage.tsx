import { useState } from 'react';
import { useNavigate, useParams } from 'react-router-dom';
import { useQuery } from '@tanstack/react-query';
import { getAgent, listAgentEvents } from '../api/agents';

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
  const [offset, setOffset] = useState(0);

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

  if (!id) return null;

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

      {isLoading ? (
        <div className="text-gray-400">Loading...</div>
      ) : (events ?? []).length === 0 ? (
        <div className="text-center py-16 text-gray-500">
          No egress events yet. Once the entity is installed and chats with
          you, audit / memory / identitySync events will appear here.
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
                  <td
                    className={`px-4 py-2 font-mono ${eventColor(e.event_type)}`}
                  >
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
