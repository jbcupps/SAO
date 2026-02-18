import { useState } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { listAgents, createAgent, deleteAgent } from '../api/agents';
import type { Agent } from '../types';

export default function AgentsPage() {
  const queryClient = useQueryClient();
  const [showRegister, setShowRegister] = useState(false);
  const [newAgentName, setNewAgentName] = useState('');
  const [error, setError] = useState('');
  const [creating, setCreating] = useState(false);
  const [deletingId, setDeletingId] = useState<string | null>(null);
  const [confirmDeleteId, setConfirmDeleteId] = useState<string | null>(null);

  const { data: agents, isLoading } = useQuery({
    queryKey: ['agents'],
    queryFn: listAgents,
  });

  const handleRegister = async () => {
    setError('');
    if (!newAgentName.trim()) {
      setError('Agent name is required');
      return;
    }

    setCreating(true);
    try {
      await createAgent(newAgentName.trim());
      await queryClient.invalidateQueries({ queryKey: ['agents'] });
      setNewAgentName('');
      setShowRegister(false);
    } catch (err) {
      setError(
        err instanceof Error ? err.message : 'Failed to register agent',
      );
    } finally {
      setCreating(false);
    }
  };

  const handleDelete = async (id: string) => {
    setDeletingId(id);
    try {
      await deleteAgent(id);
      await queryClient.invalidateQueries({ queryKey: ['agents'] });
      setConfirmDeleteId(null);
    } catch (err) {
      setError(
        err instanceof Error ? err.message : 'Failed to delete agent',
      );
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

      {/* Register Agent Form */}
      {showRegister && (
        <div className="bg-gray-800 rounded-xl border border-gray-700 p-6 mb-6">
          <h2 className="text-lg font-semibold text-white mb-4">
            Register New Agent
          </h2>
          <div className="flex gap-3">
            <input
              type="text"
              value={newAgentName}
              onChange={(e) => setNewAgentName(e.target.value)}
              placeholder="Agent name (e.g., abigail-main)"
              onKeyDown={(e) => e.key === 'Enter' && handleRegister()}
              className="flex-1 px-3 py-2 bg-gray-700 border border-gray-600 rounded-lg text-white placeholder-gray-500 focus:outline-none focus:border-blue-500"
            />
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
                setError('');
              }}
              className="px-4 py-2 bg-gray-700 hover:bg-gray-600 text-gray-300 font-medium rounded-lg transition-colors"
            >
              Cancel
            </button>
          </div>
          {error && <p className="text-red-400 text-sm mt-2">{error}</p>}
        </div>
      )}

      {/* Agent List */}
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

              {agent.public_key && (
                <div className="mb-3">
                  <p className="text-xs text-gray-500">Public Key</p>
                  <p className="text-xs text-gray-400 font-mono truncate">
                    {agent.public_key}
                  </p>
                </div>
              )}

              <div className="flex items-center justify-between mt-3 pt-3 border-t border-gray-700">
                <span className="text-xs text-gray-500">
                  Created {new Date(agent.created_at).toLocaleDateString()}
                </span>
                {confirmDeleteId === agent.id ? (
                  <div className="flex gap-2">
                    <button
                      onClick={() => handleDelete(agent.id)}
                      disabled={deletingId === agent.id}
                      className="text-xs px-2 py-1 bg-red-600 hover:bg-red-700 text-white rounded transition-colors"
                    >
                      {deletingId === agent.id ? '...' : 'Confirm'}
                    </button>
                    <button
                      onClick={() => setConfirmDeleteId(null)}
                      className="text-xs px-2 py-1 bg-gray-700 hover:bg-gray-600 text-gray-300 rounded transition-colors"
                    >
                      Cancel
                    </button>
                  </div>
                ) : (
                  <button
                    onClick={() => setConfirmDeleteId(agent.id)}
                    className="text-xs text-red-400 hover:text-red-300 transition-colors"
                  >
                    Delete
                  </button>
                )}
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
