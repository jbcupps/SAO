import { useState } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import {
  adminListPendingSkills,
  adminListPendingBindings,
  adminReviewSkill,
  adminReviewBinding,
} from '../api/skills';
import type {
  SkillCatalogEntry,
  AgentSkillBinding,
  ReviewAction,
} from '../types';

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

export default function AdminSkillReviewPage() {
  const queryClient = useQueryClient();
  const [tab, setTab] = useState<'catalog' | 'bindings'>('catalog');
  const [reviewingId, setReviewingId] = useState<string | null>(null);
  const [reviewNotes, setReviewNotes] = useState('');
  const [processing, setProcessing] = useState(false);
  const [error, setError] = useState('');

  const { data: pendingSkills, isLoading: loadingSkills } = useQuery({
    queryKey: ['pending-skills'],
    queryFn: adminListPendingSkills,
  });

  const { data: pendingBindings, isLoading: loadingBindings } = useQuery({
    queryKey: ['pending-bindings'],
    queryFn: adminListPendingBindings,
  });

  const handleReviewSkill = async (id: string, action: ReviewAction) => {
    setError('');
    setProcessing(true);
    try {
      await adminReviewSkill(id, action, reviewNotes || undefined);
      await queryClient.invalidateQueries({ queryKey: ['pending-skills'] });
      await queryClient.invalidateQueries({ queryKey: ['skills'] });
      setReviewingId(null);
      setReviewNotes('');
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Review failed');
    } finally {
      setProcessing(false);
    }
  };

  const handleReviewBinding = async (id: string, action: ReviewAction) => {
    setError('');
    setProcessing(true);
    try {
      await adminReviewBinding(id, action, reviewNotes || undefined);
      await queryClient.invalidateQueries({ queryKey: ['pending-bindings'] });
      setReviewingId(null);
      setReviewNotes('');
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Review failed');
    } finally {
      setProcessing(false);
    }
  };

  return (
    <div>
      <h1 className="text-2xl font-bold text-white mb-6">
        Skill Review Queue
      </h1>

      {/* Tabs */}
      <div className="flex gap-1 mb-6 bg-gray-800 rounded-lg p-1 w-fit border border-gray-700">
        <button
          onClick={() => setTab('catalog')}
          className={`px-4 py-2 text-sm font-medium rounded-md transition-colors ${
            tab === 'catalog'
              ? 'bg-blue-600 text-white'
              : 'text-gray-400 hover:text-white'
          }`}
        >
          Pending Skills ({pendingSkills?.length ?? 0})
        </button>
        <button
          onClick={() => setTab('bindings')}
          className={`px-4 py-2 text-sm font-medium rounded-md transition-colors ${
            tab === 'bindings'
              ? 'bg-blue-600 text-white'
              : 'text-gray-400 hover:text-white'
          }`}
        >
          Pending Bindings ({pendingBindings?.length ?? 0})
        </button>
      </div>

      {error && (
        <div className="bg-red-900/20 border border-red-800 rounded-lg p-3 mb-4">
          <p className="text-red-400 text-sm">{error}</p>
        </div>
      )}

      {/* Pending Skills Tab */}
      {tab === 'catalog' && (
        <>
          {loadingSkills ? (
            <div className="text-center py-16 text-gray-400">
              Loading pending skills...
            </div>
          ) : pendingSkills && pendingSkills.length > 0 ? (
            <div className="space-y-4">
              {pendingSkills.map((skill: SkillCatalogEntry) => (
                <div
                  key={skill.id}
                  className="bg-gray-800 rounded-xl border border-gray-700 p-5"
                >
                  <div className="flex items-start justify-between mb-3">
                    <div>
                      <div className="flex items-center gap-2">
                        <h3 className="text-white font-semibold">
                          {skill.name}
                        </h3>
                        <span className="text-xs text-gray-500">
                          v{skill.version}
                        </span>
                        <span
                          className={`inline-block px-2 py-0.5 text-xs font-medium rounded border ${riskBadge(skill.risk_level)}`}
                        >
                          {skill.risk_level}
                        </span>
                      </div>
                      {skill.description && (
                        <p className="text-sm text-gray-400 mt-1">
                          {skill.description}
                        </p>
                      )}
                    </div>
                    {skill.policy_score !== null && (
                      <div className="text-right">
                        <p className="text-2xl font-bold text-white">
                          {skill.policy_score}
                        </p>
                        <p className="text-xs text-gray-500">Risk Score</p>
                      </div>
                    )}
                  </div>

                  {/* Policy check breakdown */}
                  {skill.policy_details && (
                    <div className="bg-gray-900/50 rounded-lg p-3 mb-3">
                      <p className="text-xs font-medium text-gray-400 mb-2">
                        Policy Check Results
                      </p>
                      <div className="space-y-1">
                        {skill.policy_details.map((check) => (
                          <div
                            key={check.name}
                            className="flex items-center justify-between text-xs"
                          >
                            <div className="flex items-center gap-2">
                              <span
                                className={
                                  check.passed
                                    ? 'text-green-400'
                                    : 'text-red-400'
                                }
                              >
                                {check.passed ? '[PASS]' : '[FAIL]'}
                              </span>
                              <span className="text-gray-300">
                                {check.name.replace(/_/g, ' ')}
                              </span>
                            </div>
                            <div className="flex items-center gap-2">
                              <span className="text-gray-500">
                                {check.message}
                              </span>
                              {check.weight > 0 && (
                                <span className="text-red-400 font-medium">
                                  +{check.weight}
                                </span>
                              )}
                            </div>
                          </div>
                        ))}
                      </div>
                    </div>
                  )}

                  {/* Permissions and endpoints */}
                  <div className="flex flex-wrap gap-4 mb-3 text-xs">
                    {skill.permissions.length > 0 && (
                      <div>
                        <span className="text-gray-500">Permissions: </span>
                        {skill.permissions.map((p) => (
                          <span
                            key={p}
                            className="inline-block px-1.5 py-0.5 bg-gray-700 text-gray-300 rounded mr-1"
                          >
                            {p}
                          </span>
                        ))}
                      </div>
                    )}
                    {skill.api_endpoints.length > 0 && (
                      <div>
                        <span className="text-gray-500">Endpoints: </span>
                        {skill.api_endpoints.map((ep) => (
                          <span
                            key={ep}
                            className="inline-block px-1.5 py-0.5 bg-gray-700 text-gray-300 rounded font-mono mr-1"
                          >
                            {ep}
                          </span>
                        ))}
                      </div>
                    )}
                  </div>

                  {/* Review actions */}
                  {reviewingId === skill.id ? (
                    <div className="border-t border-gray-700 pt-3 mt-3">
                      <textarea
                        value={reviewNotes}
                        onChange={(e) => setReviewNotes(e.target.value)}
                        placeholder="Review notes (optional)"
                        rows={2}
                        className="w-full px-3 py-2 text-sm bg-gray-700 border border-gray-600 rounded-lg text-white placeholder-gray-500 focus:outline-none focus:border-blue-500 mb-3"
                      />
                      <div className="flex gap-2">
                        <button
                          onClick={() => handleReviewSkill(skill.id, 'approve')}
                          disabled={processing}
                          className="px-3 py-1.5 text-sm bg-green-600 hover:bg-green-700 disabled:bg-green-800 text-white font-medium rounded-lg transition-colors"
                        >
                          Approve
                        </button>
                        <button
                          onClick={() => handleReviewSkill(skill.id, 'reject')}
                          disabled={processing}
                          className="px-3 py-1.5 text-sm bg-red-600 hover:bg-red-700 disabled:bg-red-800 text-white font-medium rounded-lg transition-colors"
                        >
                          Reject
                        </button>
                        <button
                          onClick={() =>
                            handleReviewSkill(skill.id, 'request_changes')
                          }
                          disabled={processing}
                          className="px-3 py-1.5 text-sm bg-yellow-600 hover:bg-yellow-700 disabled:bg-yellow-800 text-white font-medium rounded-lg transition-colors"
                        >
                          Request Changes
                        </button>
                        <button
                          onClick={() => {
                            setReviewingId(null);
                            setReviewNotes('');
                            setError('');
                          }}
                          className="px-3 py-1.5 text-sm bg-gray-700 hover:bg-gray-600 text-gray-300 font-medium rounded-lg transition-colors"
                        >
                          Cancel
                        </button>
                      </div>
                    </div>
                  ) : (
                    <div className="flex items-center justify-between border-t border-gray-700 pt-3 mt-3">
                      <span className="text-xs text-gray-500">
                        Submitted{' '}
                        {new Date(skill.created_at).toLocaleDateString()}
                        {skill.created_by_agent_id &&
                          ` by agent ${skill.created_by_agent_id.slice(0, 8)}...`}
                      </span>
                      <button
                        onClick={() => setReviewingId(skill.id)}
                        className="px-3 py-1.5 text-sm bg-blue-600 hover:bg-blue-700 text-white font-medium rounded-lg transition-colors"
                      >
                        Review
                      </button>
                    </div>
                  )}
                </div>
              ))}
            </div>
          ) : (
            <div className="text-center py-16 text-gray-500">
              No pending skills to review
            </div>
          )}
        </>
      )}

      {/* Pending Bindings Tab */}
      {tab === 'bindings' && (
        <>
          {loadingBindings ? (
            <div className="text-center py-16 text-gray-400">
              Loading pending bindings...
            </div>
          ) : pendingBindings && pendingBindings.length > 0 ? (
            <div className="space-y-3">
              {pendingBindings.map((binding: AgentSkillBinding) => (
                <div
                  key={binding.id}
                  className="bg-gray-800 rounded-xl border border-gray-700 p-5"
                >
                  <div className="flex items-center justify-between mb-2">
                    <div>
                      <p className="text-sm text-gray-400">
                        Agent{' '}
                        <span className="text-white font-mono">
                          {binding.agent_id.slice(0, 8)}...
                        </span>{' '}
                        wants to use skill{' '}
                        <span className="text-white font-mono">
                          {binding.skill_id.slice(0, 8)}...
                        </span>
                      </p>
                      <p className="text-xs text-gray-500 mt-1">
                        Declared{' '}
                        {new Date(binding.declared_at).toLocaleDateString()}
                      </p>
                    </div>
                  </div>

                  {reviewingId === binding.id ? (
                    <div className="border-t border-gray-700 pt-3 mt-3">
                      <textarea
                        value={reviewNotes}
                        onChange={(e) => setReviewNotes(e.target.value)}
                        placeholder="Review notes (optional)"
                        rows={2}
                        className="w-full px-3 py-2 text-sm bg-gray-700 border border-gray-600 rounded-lg text-white placeholder-gray-500 focus:outline-none focus:border-blue-500 mb-3"
                      />
                      <div className="flex gap-2">
                        <button
                          onClick={() =>
                            handleReviewBinding(binding.id, 'approve')
                          }
                          disabled={processing}
                          className="px-3 py-1.5 text-sm bg-green-600 hover:bg-green-700 disabled:bg-green-800 text-white font-medium rounded-lg transition-colors"
                        >
                          Approve
                        </button>
                        <button
                          onClick={() =>
                            handleReviewBinding(binding.id, 'reject')
                          }
                          disabled={processing}
                          className="px-3 py-1.5 text-sm bg-red-600 hover:bg-red-700 disabled:bg-red-800 text-white font-medium rounded-lg transition-colors"
                        >
                          Reject
                        </button>
                        <button
                          onClick={() => {
                            setReviewingId(null);
                            setReviewNotes('');
                            setError('');
                          }}
                          className="px-3 py-1.5 text-sm bg-gray-700 hover:bg-gray-600 text-gray-300 font-medium rounded-lg transition-colors"
                        >
                          Cancel
                        </button>
                      </div>
                    </div>
                  ) : (
                    <div className="flex justify-end">
                      <button
                        onClick={() => setReviewingId(binding.id)}
                        className="px-3 py-1.5 text-sm bg-blue-600 hover:bg-blue-700 text-white font-medium rounded-lg transition-colors"
                      >
                        Review
                      </button>
                    </div>
                  )}
                </div>
              ))}
            </div>
          ) : (
            <div className="text-center py-16 text-gray-500">
              No pending bindings to review
            </div>
          )}
        </>
      )}
    </div>
  );
}
