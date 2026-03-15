import { useState, useCallback } from 'react';
import { useQuery } from '@tanstack/react-query';
import { queryAuditLog } from '../api/admin';
import type { AuditLogEntry } from '../types';

const PAGE_SIZE = 25;

export default function AuditLogPage() {
  const [filterUserId, setFilterUserId] = useState('');
  const [offset, setOffset] = useState(0);

  const { data: entries, isLoading } = useQuery({
    queryKey: ['audit-log', filterUserId, offset],
    queryFn: () =>
      queryAuditLog({
        user_id: filterUserId || undefined,
        offset,
        limit: PAGE_SIZE,
      }),
  });

  const handleFilter = useCallback(() => {
    setOffset(0);
  }, []);

  const handleClearFilter = useCallback(() => {
    setFilterUserId('');
    setOffset(0);
  }, []);

  const formatTime = (ts: string) => {
    const d = new Date(ts);
    return d.toLocaleString();
  };

  const formatDetails = (details: unknown) => {
    if (details === null || details === undefined) return '--';
    if (typeof details === 'string') return details;
    try {
      return JSON.stringify(details);
    } catch {
      return String(details);
    }
  };

  const actionColor = (action: string) => {
    if (action.startsWith('create') || action.startsWith('register'))
      return 'text-green-400 bg-green-900/30 border-green-800';
    if (action.startsWith('delete') || action.startsWith('revoke'))
      return 'text-red-400 bg-red-900/30 border-red-800';
    if (action.startsWith('update') || action.startsWith('rotate'))
      return 'text-yellow-400 bg-yellow-900/30 border-yellow-800';
    if (action.startsWith('login') || action.startsWith('auth'))
      return 'text-blue-400 bg-blue-900/30 border-blue-800';
    return 'text-gray-400 bg-gray-700 border-gray-600';
  };

  return (
    <div>
      <h1 className="text-2xl font-bold text-white mb-6">Audit Log</h1>

      {/* Filters */}
      <div className="bg-gray-800 rounded-xl border border-gray-700 p-4 mb-6">
        <div className="flex items-end gap-3">
          <div className="flex-1 max-w-xs">
            <label className="block text-xs font-medium text-gray-400 mb-1">
              Filter by User ID
            </label>
            <input
              type="text"
              value={filterUserId}
              onChange={(e) => setFilterUserId(e.target.value)}
              placeholder="User ID (optional)"
              className="w-full px-3 py-1.5 text-sm bg-gray-700 border border-gray-600 rounded-lg text-white placeholder-gray-500 focus:outline-none focus:border-blue-500"
            />
          </div>
          <button
            onClick={handleFilter}
            className="px-3 py-1.5 text-sm bg-blue-600 hover:bg-blue-700 text-white font-medium rounded-lg transition-colors"
          >
            Filter
          </button>
          {filterUserId && (
            <button
              onClick={handleClearFilter}
              className="px-3 py-1.5 text-sm bg-gray-700 hover:bg-gray-600 text-gray-300 font-medium rounded-lg transition-colors"
            >
              Clear
            </button>
          )}
        </div>
      </div>

      {/* Table */}
      {isLoading ? (
        <div className="text-center py-16 text-gray-400">
          Loading audit log...
        </div>
      ) : entries && entries.length > 0 ? (
        <>
          <div className="bg-gray-800 rounded-xl border border-gray-700 overflow-hidden">
            <div className="overflow-x-auto">
              <table className="w-full">
                <thead>
                  <tr className="text-left text-xs text-gray-500 uppercase border-b border-gray-700">
                    <th className="px-6 py-3">Action</th>
                    <th className="px-6 py-3">Resource</th>
                    <th className="px-6 py-3">User / Agent</th>
                    <th className="px-6 py-3">Details</th>
                    <th className="px-6 py-3">Time</th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-gray-700">
                  {entries.map((entry: AuditLogEntry) => (
                    <tr key={entry.id} className="text-sm">
                      <td className="px-6 py-3">
                        <span
                          className={`inline-block px-2 py-0.5 text-xs font-medium rounded border ${actionColor(entry.action)}`}
                        >
                          {entry.action}
                        </span>
                      </td>
                      <td className="px-6 py-3 text-gray-300">
                        {entry.resource}
                      </td>
                      <td className="px-6 py-3 text-gray-400 font-mono text-xs">
                        {entry.user_id
                          ? `user:${entry.user_id.slice(0, 8)}...`
                          : entry.agent_id
                            ? `agent:${entry.agent_id.slice(0, 8)}...`
                            : '--'}
                      </td>
                      <td className="px-6 py-3 text-gray-500 truncate max-w-xs">
                        {formatDetails(entry.details)}
                      </td>
                      <td className="px-6 py-3 text-gray-500 whitespace-nowrap">
                        {formatTime(entry.created_at)}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </div>

          {/* Pagination */}
          <div className="flex items-center justify-between mt-4">
            <div className="text-sm text-gray-500">
              Showing {offset + 1} - {offset + entries.length}
            </div>
            <div className="flex gap-2">
              <button
                onClick={() => setOffset(Math.max(0, offset - PAGE_SIZE))}
                disabled={offset === 0}
                className="px-3 py-1.5 text-sm bg-gray-700 hover:bg-gray-600 disabled:bg-gray-800 disabled:text-gray-600 text-gray-300 font-medium rounded-lg transition-colors"
              >
                Previous
              </button>
              <button
                onClick={() => setOffset(offset + PAGE_SIZE)}
                disabled={entries.length < PAGE_SIZE}
                className="px-3 py-1.5 text-sm bg-gray-700 hover:bg-gray-600 disabled:bg-gray-800 disabled:text-gray-600 text-gray-300 font-medium rounded-lg transition-colors"
              >
                Load More
              </button>
            </div>
          </div>
        </>
      ) : (
        <div className="text-center py-16 text-gray-500">
          {filterUserId
            ? 'No audit log entries found for this filter'
            : 'No audit log entries yet'}
        </div>
      )}
    </div>
  );
}
