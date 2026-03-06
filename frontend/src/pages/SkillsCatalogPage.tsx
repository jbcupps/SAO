import { useState } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { listSkills, adminCreateSkill } from '../api/skills';
import { useAuth } from '../hooks/useAuth';
import type { SkillCatalogEntry, CreateSkillData } from '../types';

const riskBadge = (level: string) => {
  switch (level) {
    case 'low':
      return 'text-green-400 bg-green-900/30 border-green-800';
    case 'medium':
      return 'text-yellow-400 bg-yellow-900/30 border-yellow-800';
    case 'high':
      return 'text-orange-400 bg-orange-900/30 border-orange-800';
    case 'critical':
      return 'text-red-400 bg-red-900/30 border-red-800';
    default:
      return 'text-gray-400 bg-gray-700 border-gray-600';
  }
};

const statusBadge = (status: string) => {
  switch (status) {
    case 'approved':
      return 'text-green-400 bg-green-900/30 border-green-800';
    case 'pending_review':
      return 'text-yellow-400 bg-yellow-900/30 border-yellow-800';
    case 'rejected':
      return 'text-red-400 bg-red-900/30 border-red-800';
    case 'deprecated':
      return 'text-gray-400 bg-gray-700 border-gray-600';
    default:
      return 'text-gray-400 bg-gray-700 border-gray-600';
  }
};

export default function SkillsCatalogPage() {
  const { isAdmin } = useAuth();
  const queryClient = useQueryClient();
  const [statusFilter, setStatusFilter] = useState('');
  const [categoryFilter, setCategoryFilter] = useState('');
  const [expandedId, setExpandedId] = useState<string | null>(null);
  const [showAdd, setShowAdd] = useState(false);
  const [formData, setFormData] = useState<CreateSkillData>({
    name: '',
    version: '1.0.0',
    description: '',
    author: '',
    category: '',
  });
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState('');

  const { data: skills, isLoading } = useQuery({
    queryKey: ['skills', statusFilter, categoryFilter],
    queryFn: () =>
      listSkills({
        status: statusFilter || undefined,
        category: categoryFilter || undefined,
      }),
  });

  const handleAdd = async () => {
    setError('');
    if (!formData.name.trim()) {
      setError('Name is required');
      return;
    }
    setSaving(true);
    try {
      await adminCreateSkill(formData);
      await queryClient.invalidateQueries({ queryKey: ['skills'] });
      setShowAdd(false);
      setFormData({
        name: '',
        version: '1.0.0',
        description: '',
        author: '',
        category: '',
      });
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to create skill');
    } finally {
      setSaving(false);
    }
  };

  return (
    <div>
      <div className="flex items-center justify-between mb-6">
        <h1 className="text-2xl font-bold text-white">Skills Catalog</h1>
        {isAdmin && (
          <button
            onClick={() => setShowAdd(true)}
            className="px-4 py-2 bg-blue-600 hover:bg-blue-700 text-white font-medium rounded-lg transition-colors text-sm"
          >
            + Add Skill
          </button>
        )}
      </div>

      {/* Add Skill Form */}
      {showAdd && (
        <div className="bg-gray-800 rounded-xl border border-gray-700 p-6 mb-6">
          <h2 className="text-lg font-semibold text-white mb-4">
            Add New Skill
          </h2>
          <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
            <div>
              <label className="block text-sm font-medium text-gray-300 mb-1">
                Name
              </label>
              <input
                type="text"
                value={formData.name}
                onChange={(e) =>
                  setFormData({ ...formData, name: e.target.value })
                }
                placeholder="e.g., text-formatter"
                className="w-full px-3 py-2 bg-gray-700 border border-gray-600 rounded-lg text-white placeholder-gray-500 focus:outline-none focus:border-blue-500"
              />
            </div>
            <div>
              <label className="block text-sm font-medium text-gray-300 mb-1">
                Version
              </label>
              <input
                type="text"
                value={formData.version || ''}
                onChange={(e) =>
                  setFormData({ ...formData, version: e.target.value })
                }
                placeholder="1.0.0"
                className="w-full px-3 py-2 bg-gray-700 border border-gray-600 rounded-lg text-white placeholder-gray-500 focus:outline-none focus:border-blue-500"
              />
            </div>
            <div className="md:col-span-2">
              <label className="block text-sm font-medium text-gray-300 mb-1">
                Description
              </label>
              <textarea
                value={formData.description || ''}
                onChange={(e) =>
                  setFormData({ ...formData, description: e.target.value })
                }
                placeholder="What does this skill do?"
                rows={2}
                className="w-full px-3 py-2 bg-gray-700 border border-gray-600 rounded-lg text-white placeholder-gray-500 focus:outline-none focus:border-blue-500"
              />
            </div>
            <div>
              <label className="block text-sm font-medium text-gray-300 mb-1">
                Author
              </label>
              <input
                type="text"
                value={formData.author || ''}
                onChange={(e) =>
                  setFormData({ ...formData, author: e.target.value })
                }
                placeholder="Author name"
                className="w-full px-3 py-2 bg-gray-700 border border-gray-600 rounded-lg text-white placeholder-gray-500 focus:outline-none focus:border-blue-500"
              />
            </div>
            <div>
              <label className="block text-sm font-medium text-gray-300 mb-1">
                Category
              </label>
              <input
                type="text"
                value={formData.category || ''}
                onChange={(e) =>
                  setFormData({ ...formData, category: e.target.value })
                }
                placeholder="e.g., utility, network, system"
                className="w-full px-3 py-2 bg-gray-700 border border-gray-600 rounded-lg text-white placeholder-gray-500 focus:outline-none focus:border-blue-500"
              />
            </div>
          </div>
          {error && <p className="text-red-400 text-sm mt-3">{error}</p>}
          <div className="flex gap-3 mt-4">
            <button
              onClick={() => {
                setShowAdd(false);
                setError('');
              }}
              className="px-4 py-2 bg-gray-700 hover:bg-gray-600 text-gray-300 font-medium rounded-lg transition-colors"
            >
              Cancel
            </button>
            <button
              onClick={handleAdd}
              disabled={saving}
              className="px-4 py-2 bg-blue-600 hover:bg-blue-700 disabled:bg-blue-800 text-white font-medium rounded-lg transition-colors"
            >
              {saving ? 'Creating...' : 'Create Skill'}
            </button>
          </div>
        </div>
      )}

      {/* Filters */}
      <div className="bg-gray-800 rounded-xl border border-gray-700 p-4 mb-6">
        <div className="flex items-end gap-3">
          <div className="flex-1 max-w-xs">
            <label className="block text-xs font-medium text-gray-400 mb-1">
              Status
            </label>
            <select
              value={statusFilter}
              onChange={(e) => setStatusFilter(e.target.value)}
              className="w-full px-3 py-1.5 text-sm bg-gray-700 border border-gray-600 rounded-lg text-white focus:outline-none focus:border-blue-500"
            >
              <option value="">All</option>
              <option value="approved">Approved</option>
              <option value="pending_review">Pending Review</option>
              <option value="rejected">Rejected</option>
              <option value="deprecated">Deprecated</option>
            </select>
          </div>
          <div className="flex-1 max-w-xs">
            <label className="block text-xs font-medium text-gray-400 mb-1">
              Category
            </label>
            <input
              type="text"
              value={categoryFilter}
              onChange={(e) => setCategoryFilter(e.target.value)}
              placeholder="Filter by category"
              className="w-full px-3 py-1.5 text-sm bg-gray-700 border border-gray-600 rounded-lg text-white placeholder-gray-500 focus:outline-none focus:border-blue-500"
            />
          </div>
        </div>
      </div>

      {/* Skills List */}
      {isLoading ? (
        <div className="text-center py-16 text-gray-400">
          Loading skills...
        </div>
      ) : skills && skills.length > 0 ? (
        <div className="space-y-3">
          {skills.map((skill: SkillCatalogEntry) => (
            <div
              key={skill.id}
              className="bg-gray-800 rounded-xl border border-gray-700"
            >
              <div
                className="p-5 cursor-pointer"
                onClick={() =>
                  setExpandedId(expandedId === skill.id ? null : skill.id)
                }
              >
                <div className="flex items-start justify-between">
                  <div className="flex-1 min-w-0">
                    <div className="flex items-center gap-2 mb-1">
                      <h3 className="text-white font-semibold">
                        {skill.name}
                      </h3>
                      <span className="text-xs text-gray-500">
                        v{skill.version}
                      </span>
                    </div>
                    {skill.description && (
                      <p className="text-sm text-gray-400 truncate">
                        {skill.description}
                      </p>
                    )}
                    <div className="flex items-center gap-2 mt-2">
                      {skill.category && (
                        <span className="text-xs px-2 py-0.5 bg-gray-700 text-gray-300 rounded">
                          {skill.category}
                        </span>
                      )}
                      {skill.author && (
                        <span className="text-xs text-gray-500">
                          by {skill.author}
                        </span>
                      )}
                    </div>
                  </div>
                  <div className="flex items-center gap-2 ml-4">
                    <span
                      className={`inline-block px-2 py-0.5 text-xs font-medium rounded border ${riskBadge(skill.risk_level)}`}
                    >
                      {skill.risk_level}
                    </span>
                    <span
                      className={`inline-block px-2 py-0.5 text-xs font-medium rounded border ${statusBadge(skill.status)}`}
                    >
                      {skill.status.replace('_', ' ')}
                    </span>
                    {skill.policy_score !== null && (
                      <span className="text-xs text-gray-500">
                        Score: {skill.policy_score}
                      </span>
                    )}
                  </div>
                </div>
              </div>

              {/* Expanded details */}
              {expandedId === skill.id && (
                <div className="px-5 pb-5 border-t border-gray-700 pt-4">
                  <div className="grid grid-cols-1 md:grid-cols-2 gap-4 text-sm">
                    {skill.permissions.length > 0 && (
                      <div>
                        <p className="text-xs font-medium text-gray-400 mb-1">
                          Permissions
                        </p>
                        <div className="flex flex-wrap gap-1">
                          {skill.permissions.map((p) => (
                            <span
                              key={p}
                              className="px-2 py-0.5 text-xs bg-gray-700 text-gray-300 rounded"
                            >
                              {p}
                            </span>
                          ))}
                        </div>
                      </div>
                    )}
                    {skill.api_endpoints.length > 0 && (
                      <div>
                        <p className="text-xs font-medium text-gray-400 mb-1">
                          API Endpoints
                        </p>
                        <div className="space-y-1">
                          {skill.api_endpoints.map((ep) => (
                            <p
                              key={ep}
                              className="text-xs text-gray-400 font-mono truncate"
                            >
                              {ep}
                            </p>
                          ))}
                        </div>
                      </div>
                    )}
                    {skill.tags.length > 0 && (
                      <div>
                        <p className="text-xs font-medium text-gray-400 mb-1">
                          Tags
                        </p>
                        <div className="flex flex-wrap gap-1">
                          {skill.tags.map((t) => (
                            <span
                              key={t}
                              className="px-2 py-0.5 text-xs bg-blue-900/30 text-blue-400 border border-blue-800 rounded"
                            >
                              {t}
                            </span>
                          ))}
                        </div>
                      </div>
                    )}
                    {skill.policy_details && (
                      <div className="md:col-span-2">
                        <p className="text-xs font-medium text-gray-400 mb-1">
                          Policy Checks
                        </p>
                        <div className="space-y-1">
                          {skill.policy_details.map((check) => (
                            <div
                              key={check.name}
                              className="flex items-center gap-2 text-xs"
                            >
                              <span
                                className={
                                  check.passed
                                    ? 'text-green-400'
                                    : 'text-red-400'
                                }
                              >
                                {check.passed ? '[PASS]' : '[FAIL]'}
                              </span>
                              <span className="text-gray-400">
                                {check.message}
                              </span>
                              {check.weight > 0 && (
                                <span className="text-gray-500">
                                  (+{check.weight})
                                </span>
                              )}
                            </div>
                          ))}
                        </div>
                      </div>
                    )}
                  </div>
                  <div className="flex items-center gap-4 mt-4 pt-3 border-t border-gray-700 text-xs text-gray-500">
                    <span>ID: {skill.id.slice(0, 8)}...</span>
                    <span>
                      Created{' '}
                      {new Date(skill.created_at).toLocaleDateString()}
                    </span>
                    {skill.reviewed_at && (
                      <span>
                        Reviewed{' '}
                        {new Date(skill.reviewed_at).toLocaleDateString()}
                      </span>
                    )}
                  </div>
                </div>
              )}
            </div>
          ))}
        </div>
      ) : (
        <div className="text-center py-16">
          <p className="text-gray-500 mb-4">No skills in catalog yet</p>
          {isAdmin && (
            <button
              onClick={() => setShowAdd(true)}
              className="px-4 py-2 bg-blue-600 hover:bg-blue-700 text-white font-medium rounded-lg transition-colors text-sm"
            >
              Add Your First Skill
            </button>
          )}
        </div>
      )}
    </div>
  );
}
