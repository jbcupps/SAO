import { useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { useVault } from '../hooks/useVault';
import { listSecrets } from '../api/vault';
import { listAgents } from '../api/agents';
import { getAdminEntityOverview, queryAuditLog } from '../api/admin';
import { listUsers } from '../api/admin';
import { listSkills, adminListPendingSkills, adminListPendingBindings } from '../api/skills';
import { useAuth } from '../hooks/useAuth';
import type { AuditLogEntry } from '../types';

export default function Dashboard() {
  const { isAdmin } = useAuth();
  const { vaultStatus, isSealed, unseal } = useVault();
  const [passphrase, setPassphrase] = useState('');
  const [unsealError, setUnsealError] = useState('');
  const [unsealLoading, setUnsealLoading] = useState(false);

  const { data: secrets } = useQuery({
    queryKey: ['secrets'],
    queryFn: listSecrets,
    enabled: vaultStatus?.status === 'unsealed',
  });

  const { data: agents } = useQuery({
    queryKey: ['agents'],
    queryFn: listAgents,
  });

  const { data: users } = useQuery({
    queryKey: ['users'],
    queryFn: listUsers,
    enabled: isAdmin,
  });

  const { data: recentAudit } = useQuery({
    queryKey: ['audit-recent'],
    queryFn: () => queryAuditLog({ limit: 10 }),
    enabled: isAdmin,
  });

  const { data: adminEntityOverview } = useQuery({
    queryKey: ['admin-entity-overview'],
    queryFn: getAdminEntityOverview,
    enabled: isAdmin,
  });

  const { data: skills } = useQuery({
    queryKey: ['skills'],
    queryFn: () => listSkills(),
  });

  const { data: pendingSkills } = useQuery({
    queryKey: ['pending-skills'],
    queryFn: adminListPendingSkills,
    enabled: isAdmin,
  });

  const { data: pendingBindings } = useQuery({
    queryKey: ['pending-bindings'],
    queryFn: adminListPendingBindings,
    enabled: isAdmin,
  });

  const handleUnseal = async () => {
    setUnsealError('');
    setUnsealLoading(true);
    try {
      await unseal(passphrase);
      setPassphrase('');
    } catch (err) {
      setUnsealError(
        err instanceof Error ? err.message : 'Failed to unseal vault',
      );
    } finally {
      setUnsealLoading(false);
    }
  };

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

  return (
    <div>
      <h1 className="text-2xl font-bold text-white mb-6">Dashboard</h1>

      <div className="grid grid-cols-1 lg:grid-cols-2 xl:grid-cols-4 gap-6 mb-8">
        {/* Vault Status Card */}
        <div className="bg-gray-800 rounded-xl border border-gray-700 p-6">
          <h3 className="text-sm font-medium text-gray-400 mb-2">
            Vault Status
          </h3>
          <div className="flex items-center gap-2 mb-3">
            <span
              className={`inline-block w-3 h-3 rounded-full ${
                vaultStatus?.status === 'unsealed'
                  ? 'bg-green-500'
                  : vaultStatus?.status === 'sealed'
                    ? 'bg-yellow-500'
                    : 'bg-red-500'
              }`}
            />
            <span className="text-lg font-semibold text-white capitalize">
              {vaultStatus?.status || 'Unknown'}
            </span>
          </div>
          {isSealed && (
            <div className="mt-3 space-y-2">
              <input
                type="password"
                value={passphrase}
                onChange={(e) => setPassphrase(e.target.value)}
                placeholder="Vault passphrase"
                onKeyDown={(e) => e.key === 'Enter' && handleUnseal()}
                className="w-full px-3 py-1.5 text-sm bg-gray-700 border border-gray-600 rounded-lg text-white placeholder-gray-500 focus:outline-none focus:border-blue-500"
              />
              <button
                onClick={handleUnseal}
                disabled={unsealLoading || !passphrase}
                className="w-full px-3 py-1.5 text-sm bg-blue-600 hover:bg-blue-700 disabled:bg-blue-800 disabled:cursor-not-allowed text-white font-medium rounded-lg transition-colors"
              >
                {unsealLoading ? 'Unsealing...' : 'Unseal Vault'}
              </button>
              {unsealError && (
                <p className="text-red-400 text-xs">{unsealError}</p>
              )}
            </div>
          )}
        </div>

        {/* Secrets Count */}
        <div className="bg-gray-800 rounded-xl border border-gray-700 p-6">
          <h3 className="text-sm font-medium text-gray-400 mb-2">
            Stored Secrets
          </h3>
          <p className="text-3xl font-bold text-white">
            {vaultStatus?.status === 'unsealed'
              ? (secrets?.length ?? '...')
              : '--'}
          </p>
          <p className="text-xs text-gray-500 mt-1">
            {vaultStatus?.status === 'unsealed'
              ? 'Keys, tokens, and credentials'
              : 'Unseal vault to view'}
          </p>
        </div>

        {/* Agents Count */}
        <div className="bg-gray-800 rounded-xl border border-gray-700 p-6">
          <h3 className="text-sm font-medium text-gray-400 mb-2">
            Registered Agents
          </h3>
          <p className="text-3xl font-bold text-white">
            {agents?.length ?? '...'}
          </p>
          <p className="text-xs text-gray-500 mt-1">
            Connected orchestrated agents
          </p>
        </div>

        {/* Skills Count */}
        <div className="bg-gray-800 rounded-xl border border-gray-700 p-6">
          <h3 className="text-sm font-medium text-gray-400 mb-2">
            Skills
          </h3>
          <p className="text-3xl font-bold text-white">
            {skills?.length ?? '...'}
          </p>
          <p className="text-xs text-gray-500 mt-1">
            Registered skills in catalog
          </p>
        </div>

        {/* Users Count (Admin Only) */}
        {isAdmin && (
          <div className="bg-gray-800 rounded-xl border border-gray-700 p-6">
            <h3 className="text-sm font-medium text-gray-400 mb-2">
              Users
            </h3>
            <p className="text-3xl font-bold text-white">
              {users?.length ?? '...'}
            </p>
            <p className="text-xs text-gray-500 mt-1">
              Registered user accounts
            </p>
          </div>
        )}

        {/* Pending Reviews (Admin Only) */}
        {isAdmin && (
          <div className="bg-gray-800 rounded-xl border border-gray-700 p-6">
            <h3 className="text-sm font-medium text-gray-400 mb-2">
              Pending Reviews
            </h3>
            <p className="text-3xl font-bold text-white">
              {(pendingSkills?.length ?? 0) + (pendingBindings?.length ?? 0)}
            </p>
            <p className="text-xs text-gray-500 mt-1">
              Skills and bindings awaiting review
            </p>
          </div>
        )}
      </div>

      {isAdmin && adminEntityOverview && (
        <div className="bg-gray-800 rounded-xl border border-gray-700 mb-8">
          <div className="px-6 py-4 border-b border-gray-700">
            <div className="flex flex-col gap-2 lg:flex-row lg:items-center lg:justify-between">
              <div>
                <h2 className="text-lg font-semibold text-white">
                  SAO Admin Entity
                </h2>
                <p className="text-sm text-gray-400 mt-1">
                  {adminEntityOverview.admin_entity.name} tracks the bootstrap
                  work required to get SAO operational beyond local Docker.
                </p>
              </div>
              <div className="rounded-lg border border-gray-700 bg-gray-900/40 px-4 py-3">
                <p className="text-xs uppercase tracking-wide text-gray-500">
                  Deployment Target
                </p>
                <p className="text-sm font-medium text-white mt-1">
                  {adminEntityOverview.admin_entity.deployment_target || 'azure_container_apps'}
                </p>
                <p className="text-xs text-gray-500 mt-1">
                  {adminEntityOverview.admin_entity.provider} / {adminEntityOverview.admin_entity.model}
                </p>
              </div>
            </div>
          </div>
          <div className="px-6 py-5">
            <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
              {adminEntityOverview.work_items.map((item) => (
                <div
                  key={item.id}
                  className="rounded-xl border border-gray-700 bg-gray-900/30 p-4"
                >
                  <div className="flex items-start justify-between gap-3">
                    <div>
                      <p className="text-sm font-medium text-white">
                        {item.title}
                      </p>
                      <p className="text-xs text-gray-500 mt-1 uppercase tracking-wide">
                        {item.area.replace(/_/g, ' ')}
                      </p>
                    </div>
                    <span className="inline-flex rounded-full border border-blue-500/30 bg-blue-500/10 px-2.5 py-1 text-[11px] font-medium uppercase tracking-wide text-blue-300">
                      {item.status.replace(/_/g, ' ')}
                    </span>
                  </div>
                  {item.description && (
                    <p className="text-sm text-gray-400 mt-3">
                      {item.description}
                    </p>
                  )}
                </div>
              ))}
            </div>
          </div>
        </div>
      )}

      {/* Recent Audit Log */}
      {isAdmin && recentAudit && recentAudit.length > 0 && (
        <div className="bg-gray-800 rounded-xl border border-gray-700">
          <div className="px-6 py-4 border-b border-gray-700">
            <h2 className="text-lg font-semibold text-white">
              Recent Activity
            </h2>
          </div>
          <div className="overflow-x-auto">
            <table className="w-full">
              <thead>
                <tr className="text-left text-xs text-gray-500 uppercase border-b border-gray-700">
                  <th className="px-6 py-3">Action</th>
                  <th className="px-6 py-3">Resource</th>
                  <th className="px-6 py-3">Details</th>
                  <th className="px-6 py-3">Time</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-gray-700">
                {recentAudit.map((entry: AuditLogEntry) => (
                  <tr key={entry.id} className="text-sm">
                    <td className="px-6 py-3 text-gray-300">
                      <span className="inline-block px-2 py-0.5 text-xs font-medium bg-gray-700 rounded">
                        {entry.action}
                      </span>
                    </td>
                    <td className="px-6 py-3 text-gray-400">
                      {entry.resource}
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
      )}
    </div>
  );
}
