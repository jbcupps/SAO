import { useState } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import {
  listOidcProviders,
  createOidcProvider,
  updateOidcProvider,
  deleteOidcProvider,
} from '../api/admin';
import type { OidcProvider, CreateOidcProviderData } from '../types';

const emptyForm: CreateOidcProviderData = {
  name: '',
  issuer_url: '',
  client_id: '',
  client_secret: '',
  scopes: 'openid profile email',
};

export default function AdminSsoPage() {
  const queryClient = useQueryClient();
  const [showAdd, setShowAdd] = useState(false);
  const [form, setForm] = useState<CreateOidcProviderData>({ ...emptyForm });
  const [error, setError] = useState('');
  const [saving, setSaving] = useState(false);
  const [togglingId, setTogglingId] = useState<string | null>(null);
  const [confirmDeleteId, setConfirmDeleteId] = useState<string | null>(null);
  const [deletingId, setDeletingId] = useState<string | null>(null);

  const { data: providers, isLoading } = useQuery({
    queryKey: ['oidc-providers'],
    queryFn: listOidcProviders,
  });

  const handleAdd = async () => {
    setError('');
    const clientSecret = (form.client_secret ?? '').trim();
    if (!form.name.trim()) {
      setError('Provider name is required');
      return;
    }
    if (!form.issuer_url.trim()) {
      setError('Issuer URL is required');
      return;
    }
    if (!form.client_id.trim()) {
      setError('Client ID is required');
      return;
    }
    if (!clientSecret) {
      setError('Client Secret is required');
      return;
    }

    setSaving(true);
    try {
      await createOidcProvider({
        ...form,
        client_secret: clientSecret,
      });
      await queryClient.invalidateQueries({ queryKey: ['oidc-providers'] });
      setForm({ ...emptyForm });
      setShowAdd(false);
    } catch (err) {
      setError(
        err instanceof Error ? err.message : 'Failed to add provider',
      );
    } finally {
      setSaving(false);
    }
  };

  const handleToggle = async (provider: OidcProvider) => {
    setTogglingId(provider.id);
    try {
      await updateOidcProvider(provider.id, { enabled: !provider.enabled });
      await queryClient.invalidateQueries({ queryKey: ['oidc-providers'] });
    } catch (err) {
      setError(
        err instanceof Error
          ? err.message
          : 'Failed to update provider',
      );
    } finally {
      setTogglingId(null);
    }
  };

  const handleDelete = async (id: string) => {
    setDeletingId(id);
    try {
      await deleteOidcProvider(id);
      await queryClient.invalidateQueries({ queryKey: ['oidc-providers'] });
      setConfirmDeleteId(null);
    } catch (err) {
      setError(
        err instanceof Error
          ? err.message
          : 'Failed to delete provider',
      );
    } finally {
      setDeletingId(null);
    }
  };

  return (
    <div>
      <div className="flex items-center justify-between mb-6">
        <div>
          <h1 className="text-2xl font-bold text-white">SSO Configuration</h1>
          <p className="text-sm text-gray-400 mt-1">
            Manage OIDC identity providers for single sign-on
          </p>
        </div>
        <button
          onClick={() => setShowAdd(true)}
          className="px-4 py-2 bg-blue-600 hover:bg-blue-700 text-white font-medium rounded-lg transition-colors text-sm"
        >
          + Add Provider
        </button>
      </div>

      {error && (
        <div className="p-3 mb-4 bg-red-900/30 border border-red-800 rounded-lg">
          <p className="text-red-400 text-sm">{error}</p>
        </div>
      )}

      {/* Add Provider Form */}
      {showAdd && (
        <div className="bg-gray-800 rounded-xl border border-gray-700 p-6 mb-6">
          <h2 className="text-lg font-semibold text-white mb-4">
            Add OIDC Provider
          </h2>
          <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
            <div>
              <label className="block text-sm font-medium text-gray-300 mb-1">
                Provider Name
              </label>
              <input
                type="text"
                value={form.name}
                onChange={(e) => setForm({ ...form, name: e.target.value })}
                placeholder="e.g., Microsoft Entra ID"
                className="w-full px-3 py-2 bg-gray-700 border border-gray-600 rounded-lg text-white placeholder-gray-500 focus:outline-none focus:border-blue-500"
              />
            </div>
            <div>
              <label className="block text-sm font-medium text-gray-300 mb-1">
                Issuer URL
              </label>
              <input
                type="url"
                value={form.issuer_url}
                onChange={(e) =>
                  setForm({ ...form, issuer_url: e.target.value })
                }
                placeholder="https://login.microsoftonline.com/{tenant}/v2.0"
                className="w-full px-3 py-2 bg-gray-700 border border-gray-600 rounded-lg text-white placeholder-gray-500 focus:outline-none focus:border-blue-500"
              />
            </div>
            <div>
              <label className="block text-sm font-medium text-gray-300 mb-1">
                Client ID
              </label>
              <input
                type="text"
                value={form.client_id}
                onChange={(e) =>
                  setForm({ ...form, client_id: e.target.value })
                }
                placeholder="Application (client) ID"
                className="w-full px-3 py-2 bg-gray-700 border border-gray-600 rounded-lg text-white placeholder-gray-500 focus:outline-none focus:border-blue-500"
              />
            </div>
            <div>
              <label className="block text-sm font-medium text-gray-300 mb-1">
                Client Secret
              </label>
              <input
                type="password"
                value={form.client_secret ?? ''}
                onChange={(e) =>
                  setForm({ ...form, client_secret: e.target.value })
                }
                placeholder="Client secret value"
                className="w-full px-3 py-2 bg-gray-700 border border-gray-600 rounded-lg text-white placeholder-gray-500 focus:outline-none focus:border-blue-500"
              />
            </div>
            <div className="md:col-span-2">
              <label className="block text-sm font-medium text-gray-300 mb-1">
                Scopes
              </label>
              <input
                type="text"
                value={form.scopes}
                onChange={(e) =>
                  setForm({ ...form, scopes: e.target.value })
                }
                placeholder="openid profile email"
                className="w-full px-3 py-2 bg-gray-700 border border-gray-600 rounded-lg text-white placeholder-gray-500 focus:outline-none focus:border-blue-500"
              />
            </div>
          </div>

          <div className="flex gap-3 mt-4">
            <button
              onClick={() => {
                setShowAdd(false);
                setForm({ ...emptyForm });
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
              {saving ? 'Saving...' : 'Add Provider'}
            </button>
          </div>
        </div>
      )}

      {/* Provider List */}
      {isLoading ? (
        <div className="text-center py-16 text-gray-400">
          Loading providers...
        </div>
      ) : providers && providers.length > 0 ? (
        <div className="space-y-4">
          {providers.map((provider: OidcProvider) => (
            <div
              key={provider.id}
              className="bg-gray-800 rounded-xl border border-gray-700 p-5"
            >
              <div className="flex items-start justify-between">
                <div>
                  <div className="flex items-center gap-3">
                    <h3 className="text-white font-semibold">
                      {provider.name}
                    </h3>
                    <span
                      className={`inline-block px-2 py-0.5 text-xs font-medium rounded ${
                        provider.enabled
                          ? 'bg-green-900/50 text-green-400 border border-green-800'
                          : 'bg-gray-700 text-gray-500'
                      }`}
                    >
                      {provider.enabled ? 'Enabled' : 'Disabled'}
                    </span>
                  </div>
                  {provider.issuer_url && (
                    <p className="text-xs text-gray-500 mt-1 font-mono">
                      {provider.issuer_url}
                    </p>
                  )}
                  {provider.client_id && (
                    <p className="text-xs text-gray-500 mt-0.5">
                      Client ID: {provider.client_id}
                    </p>
                  )}
                  {provider.scopes && (
                    <p className="text-xs text-gray-500 mt-0.5">
                      Scopes: {provider.scopes}
                    </p>
                  )}
                </div>

                <div className="flex items-center gap-2">
                  {/* Enable/Disable Toggle */}
                  <button
                    onClick={() => handleToggle(provider)}
                    disabled={togglingId === provider.id}
                    className={`relative inline-flex h-6 w-11 items-center rounded-full transition-colors ${
                      provider.enabled ? 'bg-blue-600' : 'bg-gray-600'
                    } ${togglingId === provider.id ? 'opacity-50' : ''}`}
                  >
                    <span
                      className={`inline-block h-4 w-4 transform rounded-full bg-white transition-transform ${
                        provider.enabled
                          ? 'translate-x-6'
                          : 'translate-x-1'
                      }`}
                    />
                  </button>

                  {/* Delete */}
                  {confirmDeleteId === provider.id ? (
                    <div className="flex gap-1">
                      <button
                        onClick={() => handleDelete(provider.id)}
                        disabled={deletingId === provider.id}
                        className="text-xs px-2 py-1 bg-red-600 hover:bg-red-700 text-white rounded transition-colors"
                      >
                        {deletingId === provider.id ? '...' : 'Confirm'}
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
                      onClick={() => setConfirmDeleteId(provider.id)}
                      className="text-xs text-red-400 hover:text-red-300 transition-colors px-2 py-1"
                    >
                      Delete
                    </button>
                  )}
                </div>
              </div>
            </div>
          ))}
        </div>
      ) : (
        <div className="text-center py-16">
          <p className="text-gray-500 mb-4">
            No SSO providers configured
          </p>
          <button
            onClick={() => setShowAdd(true)}
            className="px-4 py-2 bg-blue-600 hover:bg-blue-700 text-white font-medium rounded-lg transition-colors text-sm"
          >
            Add Your First Provider
          </button>
        </div>
      )}
    </div>
  );
}
