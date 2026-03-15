import { useState } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { listUsers, updateUserRole, deleteUser } from '../api/admin';
import { useAuth } from '../hooks/useAuth';
import type { User } from '../types';

export default function AdminUsersPage() {
  const { user: currentUser } = useAuth();
  const queryClient = useQueryClient();
  const [error, setError] = useState('');
  const [updatingId, setUpdatingId] = useState<string | null>(null);
  const [deletingId, setDeletingId] = useState<string | null>(null);
  const [confirmDeleteId, setConfirmDeleteId] = useState<string | null>(null);

  const { data: users, isLoading } = useQuery({
    queryKey: ['users'],
    queryFn: listUsers,
  });

  const handleRoleChange = async (userId: string, role: 'user' | 'admin') => {
    setError('');
    setUpdatingId(userId);
    try {
      await updateUserRole(userId, role);
      await queryClient.invalidateQueries({ queryKey: ['users'] });
    } catch (err) {
      setError(
        err instanceof Error ? err.message : 'Failed to update role',
      );
    } finally {
      setUpdatingId(null);
    }
  };

  const handleDelete = async (userId: string) => {
    setError('');
    setDeletingId(userId);
    try {
      await deleteUser(userId);
      await queryClient.invalidateQueries({ queryKey: ['users'] });
      setConfirmDeleteId(null);
    } catch (err) {
      setError(
        err instanceof Error ? err.message : 'Failed to delete user',
      );
    } finally {
      setDeletingId(null);
    }
  };

  return (
    <div>
      <h1 className="text-2xl font-bold text-white mb-6">User Management</h1>

      {error && (
        <div className="p-3 mb-4 bg-red-900/30 border border-red-800 rounded-lg">
          <p className="text-red-400 text-sm">{error}</p>
        </div>
      )}

      {isLoading ? (
        <div className="text-center py-16 text-gray-400">
          Loading users...
        </div>
      ) : users && users.length > 0 ? (
        <div className="bg-gray-800 rounded-xl border border-gray-700 overflow-hidden">
          <div className="overflow-x-auto">
            <table className="w-full">
              <thead>
                <tr className="text-left text-xs text-gray-500 uppercase border-b border-gray-700">
                  <th className="px-6 py-3">Username</th>
                  <th className="px-6 py-3">Display Name</th>
                  <th className="px-6 py-3">Role</th>
                  <th className="px-6 py-3">Created</th>
                  <th className="px-6 py-3">Actions</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-gray-700">
                {users.map((user: User) => {
                  const isSelf = user.id === currentUser?.id;
                  return (
                    <tr key={user.id} className="text-sm">
                      <td className="px-6 py-3 text-gray-200 font-medium">
                        {user.username}
                        {isSelf && (
                          <span className="ml-2 text-xs text-blue-400">
                            (you)
                          </span>
                        )}
                      </td>
                      <td className="px-6 py-3 text-gray-400">
                        {user.display_name}
                      </td>
                      <td className="px-6 py-3">
                        <select
                          value={user.role}
                          onChange={(e) =>
                            handleRoleChange(
                              user.id,
                              e.target.value as 'user' | 'admin',
                            )
                          }
                          disabled={isSelf || updatingId === user.id}
                          className="px-2 py-1 text-xs bg-gray-700 border border-gray-600 rounded text-gray-300 focus:outline-none focus:border-blue-500 disabled:opacity-50"
                        >
                          <option value="user">User</option>
                          <option value="admin">Admin</option>
                        </select>
                      </td>
                      <td className="px-6 py-3 text-gray-500 whitespace-nowrap">
                        {user.created_at
                          ? new Date(user.created_at).toLocaleDateString()
                          : '--'}
                      </td>
                      <td className="px-6 py-3">
                        {isSelf ? (
                          <span className="text-xs text-gray-600">--</span>
                        ) : confirmDeleteId === user.id ? (
                          <div className="flex gap-2">
                            <button
                              onClick={() => handleDelete(user.id)}
                              disabled={deletingId === user.id}
                              className="text-xs px-2 py-1 bg-red-600 hover:bg-red-700 text-white rounded transition-colors"
                            >
                              {deletingId === user.id
                                ? '...'
                                : 'Confirm'}
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
                            onClick={() => setConfirmDeleteId(user.id)}
                            className="text-xs text-red-400 hover:text-red-300 transition-colors"
                          >
                            Delete
                          </button>
                        )}
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        </div>
      ) : (
        <div className="text-center py-16 text-gray-500">
          No users found
        </div>
      )}
    </div>
  );
}
