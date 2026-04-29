import { useQuery } from '@tanstack/react-query';
import { listEntityArchives } from '../api/admin';
import type { EntityArchive } from '../types';

export default function AdminEntityArchivesPage() {
  const { data: archives, isLoading, error } = useQuery({
    queryKey: ['entity-archives'],
    queryFn: listEntityArchives,
  });

  return (
    <div>
      <div className="mb-6">
        <h1 className="text-2xl font-bold text-white">Entity Archives</h1>
        <p className="mt-1 text-sm text-gray-400">
          Deleted entities are archived with identity documents, Orion egress, and memory events.
        </p>
      </div>

      {error && (
        <div className="mb-4 rounded-lg border border-red-800 bg-red-900/30 p-3">
          <p className="text-sm text-red-300">
            {error instanceof Error ? error.message : 'Failed to load archives'}
          </p>
        </div>
      )}

      {isLoading ? (
        <div className="py-16 text-center text-gray-400">Loading archives...</div>
      ) : archives && archives.length > 0 ? (
        <div className="overflow-hidden rounded-xl border border-gray-700 bg-gray-800">
          <div className="overflow-x-auto">
            <table className="w-full">
              <thead>
                <tr className="border-b border-gray-700 text-left text-xs uppercase text-gray-500">
                  <th className="px-6 py-3">Entity</th>
                  <th className="px-6 py-3">Memories</th>
                  <th className="px-6 py-3">Events</th>
                  <th className="px-6 py-3">Archived</th>
                  <th className="px-6 py-3">Storage</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-gray-700">
                {archives.map((archive: EntityArchive) => (
                  <tr key={archive.id} className="text-sm">
                    <td className="px-6 py-3">
                      <div className="font-medium text-gray-200">{archive.agent_name}</div>
                      <div className="mt-0.5 font-mono text-xs text-gray-500">
                        {archive.agent_id}
                      </div>
                    </td>
                    <td className="px-6 py-3 text-gray-300">{archive.memory_event_count}</td>
                    <td className="px-6 py-3 text-gray-300">{archive.egress_event_count}</td>
                    <td className="whitespace-nowrap px-6 py-3 text-gray-400">
                      {new Date(archive.created_at).toLocaleString()}
                    </td>
                    <td className="px-6 py-3">
                      <code className="block max-w-md truncate text-xs text-gray-500">
                        {archive.archive_path}
                      </code>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      ) : (
        <div className="rounded-xl border border-gray-700 bg-gray-800 py-16 text-center text-gray-500">
          No entity archives yet
        </div>
      )}
    </div>
  );
}
