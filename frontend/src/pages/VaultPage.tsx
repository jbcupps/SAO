import { useMemo, useState } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { useVault } from '../hooks/useVault';
import { useAuth } from '../hooks/useAuth';
import {
  listSecrets,
  createSecret,
  getSecret,
  updateSecret,
  deleteSecret,
} from '../api/vault';
import type { VaultSecret, CreateSecretData } from '../types';

/** Must match `MIN_VAULT_PASSPHRASE_LEN` in `crates/sao-server/src/routes/vault.rs`. */
const MIN_PASSPHRASE_LEN = 12;

const SECRET_TYPES = [
  { value: 'api_key', label: 'API Key' },
  { value: 'ed25519', label: 'Ed25519 Key' },
  { value: 'gpg', label: 'GPG Key' },
  { value: 'oauth_token', label: 'OAuth Token' },
  { value: 'other', label: 'Other' },
];

interface ModalProps {
  onClose: () => void;
  children: React.ReactNode;
  title: string;
}

function Modal({ onClose, children, title }: ModalProps) {
  return (
    <div className="fixed inset-0 bg-black/60 flex items-center justify-center z-50 p-4">
      <div className="bg-gray-800 rounded-xl border border-gray-700 w-full max-w-lg max-h-[90vh] overflow-y-auto">
        <div className="flex items-center justify-between px-6 py-4 border-b border-gray-700">
          <h2 className="text-lg font-semibold text-white">{title}</h2>
          <button
            onClick={onClose}
            className="text-gray-400 hover:text-white transition-colors text-xl leading-none"
          >
            x
          </button>
        </div>
        <div className="px-6 py-4">{children}</div>
      </div>
    </div>
  );
}

function AddSecretModal({ onClose }: { onClose: () => void }) {
  const queryClient = useQueryClient();
  const [formData, setFormData] = useState<CreateSecretData>({
    secret_type: 'api_key',
    label: '',
    provider: '',
    value: '',
  });
  const [error, setError] = useState('');
  const [saving, setSaving] = useState(false);

  const handleSubmit = async () => {
    setError('');
    if (!formData.label.trim()) {
      setError('Label is required');
      return;
    }
    if (!formData.value.trim()) {
      setError('Value is required');
      return;
    }

    setSaving(true);
    try {
      await createSecret(formData);
      await queryClient.invalidateQueries({ queryKey: ['secrets'] });
      onClose();
    } catch (err) {
      setError(
        err instanceof Error ? err.message : 'Failed to create secret',
      );
    } finally {
      setSaving(false);
    }
  };

  return (
    <Modal title="Add Secret" onClose={onClose}>
      <div className="space-y-4">
        <div>
          <label className="block text-sm font-medium text-gray-300 mb-1">
            Type
          </label>
          <select
            value={formData.secret_type}
            onChange={(e) =>
              setFormData({ ...formData, secret_type: e.target.value })
            }
            className="w-full px-3 py-2 bg-gray-700 border border-gray-600 rounded-lg text-white focus:outline-none focus:border-blue-500"
          >
            {SECRET_TYPES.map((t) => (
              <option key={t.value} value={t.value}>
                {t.label}
              </option>
            ))}
          </select>
        </div>
        <div>
          <label className="block text-sm font-medium text-gray-300 mb-1">
            Label
          </label>
          <input
            type="text"
            value={formData.label}
            onChange={(e) =>
              setFormData({ ...formData, label: e.target.value })
            }
            placeholder="e.g., OpenAI Production Key"
            className="w-full px-3 py-2 bg-gray-700 border border-gray-600 rounded-lg text-white placeholder-gray-500 focus:outline-none focus:border-blue-500"
          />
        </div>
        <div>
          <label className="block text-sm font-medium text-gray-300 mb-1">
            Provider
          </label>
          <input
            type="text"
            value={formData.provider}
            onChange={(e) =>
              setFormData({ ...formData, provider: e.target.value })
            }
            placeholder="e.g., openai, anthropic, github"
            className="w-full px-3 py-2 bg-gray-700 border border-gray-600 rounded-lg text-white placeholder-gray-500 focus:outline-none focus:border-blue-500"
          />
        </div>
        <div>
          <label className="block text-sm font-medium text-gray-300 mb-1">
            Value
          </label>
          <textarea
            value={formData.value}
            onChange={(e) =>
              setFormData({ ...formData, value: e.target.value })
            }
            placeholder="Secret value (key, token, etc.)"
            rows={4}
            className="w-full px-3 py-2 bg-gray-700 border border-gray-600 rounded-lg text-white placeholder-gray-500 focus:outline-none focus:border-blue-500 font-mono text-sm"
          />
        </div>

        {error && <p className="text-red-400 text-sm">{error}</p>}

        <div className="flex gap-3 pt-2">
          <button
            onClick={onClose}
            className="px-4 py-2 bg-gray-700 hover:bg-gray-600 text-gray-300 font-medium rounded-lg transition-colors"
          >
            Cancel
          </button>
          <button
            onClick={handleSubmit}
            disabled={saving}
            className="flex-1 px-4 py-2 bg-blue-600 hover:bg-blue-700 disabled:bg-blue-800 text-white font-medium rounded-lg transition-colors"
          >
            {saving ? 'Saving...' : 'Add Secret'}
          </button>
        </div>
      </div>
    </Modal>
  );
}

function ViewSecretModal({
  secretId,
  onClose,
}: {
  secretId: string;
  onClose: () => void;
}) {
  const queryClient = useQueryClient();
  const [editLabel, setEditLabel] = useState('');
  const [editProvider, setEditProvider] = useState('');
  const [editValue, setEditValue] = useState('');
  const [isEditing, setIsEditing] = useState(false);
  const [saving, setSaving] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [error, setError] = useState('');

  const { data: secret, isLoading } = useQuery({
    queryKey: ['secret', secretId],
    queryFn: () => getSecret(secretId),
  });

  const startEditing = () => {
    if (secret) {
      setEditLabel(secret.label);
      setEditProvider(secret.provider || '');
      setEditValue(secret.value || '');
      setIsEditing(true);
    }
  };

  const handleSave = async () => {
    setError('');
    setSaving(true);
    try {
      await updateSecret(secretId, {
        label: editLabel,
        provider: editProvider,
        value: editValue,
      });
      await queryClient.invalidateQueries({ queryKey: ['secrets'] });
      await queryClient.invalidateQueries({
        queryKey: ['secret', secretId],
      });
      setIsEditing(false);
    } catch (err) {
      setError(
        err instanceof Error ? err.message : 'Failed to update secret',
      );
    } finally {
      setSaving(false);
    }
  };

  const handleDelete = async () => {
    setDeleting(true);
    try {
      await deleteSecret(secretId);
      await queryClient.invalidateQueries({ queryKey: ['secrets'] });
      onClose();
    } catch (err) {
      setError(
        err instanceof Error ? err.message : 'Failed to delete secret',
      );
      setDeleting(false);
    }
  };

  if (isLoading) {
    return (
      <Modal title="View Secret" onClose={onClose}>
        <div className="text-center py-8 text-gray-400">Loading...</div>
      </Modal>
    );
  }

  if (!secret) {
    return (
      <Modal title="View Secret" onClose={onClose}>
        <div className="text-center py-8 text-red-400">
          Secret not found
        </div>
      </Modal>
    );
  }

  return (
    <Modal title={isEditing ? 'Edit Secret' : 'View Secret'} onClose={onClose}>
      <div className="space-y-4">
        <div>
          <label className="block text-xs font-medium text-gray-500 mb-1">
            Type
          </label>
          <p className="text-sm text-gray-300">
            {SECRET_TYPES.find((t) => t.value === secret.secret_type)
              ?.label || secret.secret_type}
          </p>
        </div>

        {isEditing ? (
          <>
            <div>
              <label className="block text-sm font-medium text-gray-300 mb-1">
                Label
              </label>
              <input
                type="text"
                value={editLabel}
                onChange={(e) => setEditLabel(e.target.value)}
                className="w-full px-3 py-2 bg-gray-700 border border-gray-600 rounded-lg text-white focus:outline-none focus:border-blue-500"
              />
            </div>
            <div>
              <label className="block text-sm font-medium text-gray-300 mb-1">
                Provider
              </label>
              <input
                type="text"
                value={editProvider}
                onChange={(e) => setEditProvider(e.target.value)}
                className="w-full px-3 py-2 bg-gray-700 border border-gray-600 rounded-lg text-white focus:outline-none focus:border-blue-500"
              />
            </div>
            <div>
              <label className="block text-sm font-medium text-gray-300 mb-1">
                Value
              </label>
              <textarea
                value={editValue}
                onChange={(e) => setEditValue(e.target.value)}
                rows={4}
                className="w-full px-3 py-2 bg-gray-700 border border-gray-600 rounded-lg text-white focus:outline-none focus:border-blue-500 font-mono text-sm"
              />
            </div>
          </>
        ) : (
          <>
            <div>
              <label className="block text-xs font-medium text-gray-500 mb-1">
                Label
              </label>
              <p className="text-sm text-gray-300">{secret.label}</p>
            </div>
            <div>
              <label className="block text-xs font-medium text-gray-500 mb-1">
                Provider
              </label>
              <p className="text-sm text-gray-300">
                {secret.provider || '--'}
              </p>
            </div>
            <div>
              <label className="block text-xs font-medium text-gray-500 mb-1">
                Value
              </label>
              <pre className="text-sm text-gray-300 bg-gray-700 rounded-lg p-3 font-mono break-all whitespace-pre-wrap">
                {secret.value || '(empty)'}
              </pre>
            </div>
            <div className="flex gap-4 text-xs text-gray-500">
              <span>Created: {new Date(secret.created_at).toLocaleString()}</span>
              <span>Updated: {new Date(secret.updated_at).toLocaleString()}</span>
            </div>
          </>
        )}

        {error && <p className="text-red-400 text-sm">{error}</p>}

        <div className="flex gap-3 pt-2">
          {isEditing ? (
            <>
              <button
                onClick={() => setIsEditing(false)}
                className="px-4 py-2 bg-gray-700 hover:bg-gray-600 text-gray-300 font-medium rounded-lg transition-colors"
              >
                Cancel
              </button>
              <button
                onClick={handleSave}
                disabled={saving}
                className="flex-1 px-4 py-2 bg-blue-600 hover:bg-blue-700 disabled:bg-blue-800 text-white font-medium rounded-lg transition-colors"
              >
                {saving ? 'Saving...' : 'Save Changes'}
              </button>
            </>
          ) : (
            <>
              <button
                onClick={startEditing}
                className="px-4 py-2 bg-gray-700 hover:bg-gray-600 text-gray-300 font-medium rounded-lg transition-colors"
              >
                Edit
              </button>
              {confirmDelete ? (
                <button
                  onClick={handleDelete}
                  disabled={deleting}
                  className="flex-1 px-4 py-2 bg-red-600 hover:bg-red-700 text-white font-medium rounded-lg transition-colors"
                >
                  {deleting ? 'Deleting...' : 'Confirm Delete'}
                </button>
              ) : (
                <button
                  onClick={() => setConfirmDelete(true)}
                  className="px-4 py-2 bg-red-900/50 hover:bg-red-900 text-red-400 hover:text-red-300 font-medium rounded-lg transition-colors border border-red-800"
                >
                  Delete
                </button>
              )}
            </>
          )}
        </div>
      </div>
    </Modal>
  );
}

function passphraseHints(passphrase: string, confirmation: string): string[] {
  const hints: string[] = [];
  if (passphrase.length === 0) {
    hints.push(`At least ${MIN_PASSPHRASE_LEN} characters`);
  } else if (passphrase.length < MIN_PASSPHRASE_LEN) {
    hints.push(
      `${MIN_PASSPHRASE_LEN - passphrase.length} more character${
        MIN_PASSPHRASE_LEN - passphrase.length === 1 ? '' : 's'
      } to reach the ${MIN_PASSPHRASE_LEN}-character minimum`,
    );
  }
  if (confirmation.length > 0 && confirmation !== passphrase) {
    hints.push('Confirmation does not match');
  }
  return hints;
}

function ConfigureVaultCard({ isAdmin }: { isAdmin: boolean }) {
  const { configure } = useVault();
  const [passphrase, setPassphrase] = useState('');
  const [confirmation, setConfirmation] = useState('');
  const [error, setError] = useState('');
  const [saving, setSaving] = useState(false);

  const hints = useMemo(
    () => passphraseHints(passphrase, confirmation),
    [passphrase, confirmation],
  );
  const canSubmit =
    isAdmin &&
    !saving &&
    passphrase.length >= MIN_PASSPHRASE_LEN &&
    passphrase === confirmation;

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setError('');
    setSaving(true);
    try {
      await configure({
        passphrase,
        passphrase_confirmation: confirmation,
      });
      setPassphrase('');
      setConfirmation('');
    } catch (err) {
      setError(
        err instanceof Error
          ? err.message
          : 'Failed to configure vault passphrase',
      );
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="max-w-xl mx-auto mt-12">
      <div className="rounded-2xl border border-slate-800 bg-gradient-to-b from-slate-900 to-slate-950 shadow-xl p-8">
        <p className="text-xs uppercase tracking-[0.3em] text-cyan-300">
          First-time setup
        </p>
        <h2 className="mt-2 text-xl font-semibold text-white">
          Configure the vault passphrase
        </h2>
        <p className="mt-3 text-sm text-slate-400">
          Choose a strong passphrase. SAO derives an Argon2id key from it and
          uses that key to seal the vault master key. The passphrase is never
          stored. Encrypted secrets stay valid forever; only the envelope
          changes when you rotate.
        </p>

        {!isAdmin ? (
          <div className="mt-6 rounded-xl border border-slate-700 bg-slate-900/70 p-4 text-sm text-slate-300">
            The vault has not been configured yet. Ask a SAO administrator to
            sign in and complete this step before you can store secrets.
          </div>
        ) : (
          <form onSubmit={handleSubmit} className="mt-6 space-y-4">
            <div>
              <label className="block text-sm font-medium text-slate-300 mb-1">
                New passphrase
              </label>
              <input
                type="password"
                autoComplete="new-password"
                value={passphrase}
                onChange={(e) => setPassphrase(e.target.value)}
                placeholder={`At least ${MIN_PASSPHRASE_LEN} characters`}
                className="w-full px-3 py-2 bg-slate-800 border border-slate-700 rounded-lg text-white placeholder-slate-500 focus:outline-none focus:border-cyan-500"
                required
              />
            </div>
            <div>
              <label className="block text-sm font-medium text-slate-300 mb-1">
                Confirm passphrase
              </label>
              <input
                type="password"
                autoComplete="new-password"
                value={confirmation}
                onChange={(e) => setConfirmation(e.target.value)}
                placeholder="Repeat the passphrase"
                className="w-full px-3 py-2 bg-slate-800 border border-slate-700 rounded-lg text-white placeholder-slate-500 focus:outline-none focus:border-cyan-500"
                required
              />
            </div>

            {hints.length > 0 && (
              <ul className="space-y-1 text-xs text-amber-300">
                {hints.map((hint) => (
                  <li key={hint}>{hint}</li>
                ))}
              </ul>
            )}
            {error && <p className="text-sm text-red-400">{error}</p>}

            <button
              type="submit"
              disabled={!canSubmit}
              className="w-full px-4 py-2.5 bg-cyan-600 hover:bg-cyan-500 disabled:bg-slate-700 disabled:cursor-not-allowed text-white font-medium rounded-lg transition-colors"
            >
              {saving ? 'Configuring...' : 'Configure vault'}
            </button>
            <p className="text-xs text-slate-500">
              Tip: store this passphrase in your team password manager and in
              the deployment <code>SAO_VAULT_PASSPHRASE</code> secret so the
              vault auto-unseals on every restart.
            </p>
          </form>
        )}
      </div>
    </div>
  );
}

function RotatePassphraseModal({
  onClose,
  autoUnsealEnvPresent,
}: {
  onClose: () => void;
  autoUnsealEnvPresent: boolean;
}) {
  const { rotatePassphrase } = useVault();
  const [current, setCurrent] = useState('');
  const [next, setNext] = useState('');
  const [confirmation, setConfirmation] = useState('');
  const [error, setError] = useState('');
  const [saving, setSaving] = useState(false);
  const [success, setSuccess] = useState(false);
  const [stale, setStale] = useState(false);

  const hints = useMemo(
    () => passphraseHints(next, confirmation),
    [next, confirmation],
  );
  const canSubmit =
    !saving &&
    current.length > 0 &&
    next.length >= MIN_PASSPHRASE_LEN &&
    next === confirmation &&
    next !== current;

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setError('');
    setSaving(true);
    try {
      const result = await rotatePassphrase({
        current_passphrase: current,
        new_passphrase: next,
        new_passphrase_confirmation: confirmation,
      });
      setCurrent('');
      setNext('');
      setConfirmation('');
      setSuccess(true);
      setStale(result.auto_unseal_env_stale === true);
    } catch (err) {
      setError(
        err instanceof Error ? err.message : 'Failed to rotate passphrase',
      );
    } finally {
      setSaving(false);
    }
  };

  return (
    <Modal title="Rotate vault passphrase" onClose={onClose}>
      {success ? (
        <div className="space-y-4">
          <p className="text-sm text-emerald-300">
            Vault passphrase rotated successfully.
          </p>
          {stale && (
            <div className="rounded-lg border border-amber-700 bg-amber-900/20 p-4 text-sm text-amber-200">
              <p className="font-medium">
                Update <code>SAO_VAULT_PASSPHRASE</code> in your deployment.
              </p>
              <p className="mt-1 text-amber-300/90">
                Auto-unseal currently still uses the old value. The next
                container restart will boot the vault sealed unless the
                deployment-side secret is rotated to match.
              </p>
            </div>
          )}
          <div className="pt-2">
            <button
              onClick={onClose}
              className="px-4 py-2 bg-cyan-600 hover:bg-cyan-500 text-white font-medium rounded-lg transition-colors"
            >
              Done
            </button>
          </div>
        </div>
      ) : (
        <form onSubmit={handleSubmit} className="space-y-4">
          <p className="text-sm text-slate-400">
            The current passphrase is required. The vault master key itself
            does not change, so existing encrypted secrets stay valid.
          </p>
          <div>
            <label className="block text-sm font-medium text-slate-300 mb-1">
              Current passphrase
            </label>
            <input
              type="password"
              autoComplete="current-password"
              value={current}
              onChange={(e) => setCurrent(e.target.value)}
              className="w-full px-3 py-2 bg-slate-800 border border-slate-700 rounded-lg text-white placeholder-slate-500 focus:outline-none focus:border-cyan-500"
              required
            />
          </div>
          <div>
            <label className="block text-sm font-medium text-slate-300 mb-1">
              New passphrase
            </label>
            <input
              type="password"
              autoComplete="new-password"
              value={next}
              onChange={(e) => setNext(e.target.value)}
              placeholder={`At least ${MIN_PASSPHRASE_LEN} characters`}
              className="w-full px-3 py-2 bg-slate-800 border border-slate-700 rounded-lg text-white placeholder-slate-500 focus:outline-none focus:border-cyan-500"
              required
            />
          </div>
          <div>
            <label className="block text-sm font-medium text-slate-300 mb-1">
              Confirm new passphrase
            </label>
            <input
              type="password"
              autoComplete="new-password"
              value={confirmation}
              onChange={(e) => setConfirmation(e.target.value)}
              className="w-full px-3 py-2 bg-slate-800 border border-slate-700 rounded-lg text-white placeholder-slate-500 focus:outline-none focus:border-cyan-500"
              required
            />
          </div>

          {next.length > 0 && next === current && (
            <p className="text-xs text-amber-300">
              New passphrase must differ from the current one
            </p>
          )}
          {hints.length > 0 && (
            <ul className="space-y-1 text-xs text-amber-300">
              {hints.map((hint) => (
                <li key={hint}>{hint}</li>
              ))}
            </ul>
          )}
          {autoUnsealEnvPresent && (
            <p className="text-xs text-slate-400">
              <code>SAO_VAULT_PASSPHRASE</code> is currently set in the
              deployment. After rotating here, update that secret to match
              before the next restart.
            </p>
          )}
          {error && <p className="text-sm text-red-400">{error}</p>}

          <div className="flex gap-3 pt-2">
            <button
              type="button"
              onClick={onClose}
              className="px-4 py-2 bg-slate-700 hover:bg-slate-600 text-slate-200 font-medium rounded-lg transition-colors"
            >
              Cancel
            </button>
            <button
              type="submit"
              disabled={!canSubmit}
              className="flex-1 px-4 py-2 bg-cyan-600 hover:bg-cyan-500 disabled:bg-slate-700 disabled:cursor-not-allowed text-white font-medium rounded-lg transition-colors"
            >
              {saving ? 'Rotating...' : 'Rotate passphrase'}
            </button>
          </div>
        </form>
      )}
    </Modal>
  );
}

export default function VaultPage() {
  const {
    vaultStatus,
    isUninitialized,
    isSealed,
    unseal,
    autoUnsealEnvPresent,
  } = useVault();
  const { isAdmin } = useAuth();
  const [passphrase, setPassphrase] = useState('');
  const [unsealError, setUnsealError] = useState('');
  const [unsealLoading, setUnsealLoading] = useState(false);
  const [showAdd, setShowAdd] = useState(false);
  const [viewSecretId, setViewSecretId] = useState<string | null>(null);
  const [showRotate, setShowRotate] = useState(false);

  const { data: secrets, isLoading } = useQuery({
    queryKey: ['secrets'],
    queryFn: listSecrets,
    enabled: vaultStatus?.status === 'unsealed',
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

  const formatType = (type: string) => {
    return SECRET_TYPES.find((t) => t.value === type)?.label || type;
  };

  // Vault never configured — show first-time configure card.
  if (isUninitialized) {
    return (
      <div>
        <h1 className="text-2xl font-bold text-white mb-6">Key Vault</h1>
        <ConfigureVaultCard isAdmin={isAdmin} />
      </div>
    );
  }

  // Vault sealed — admin can unseal here, or rotate the passphrase if they
  // know the current one.
  if (isSealed) {
    return (
      <div>
        <h1 className="text-2xl font-bold text-white mb-6">Key Vault</h1>
        <div className="max-w-md mx-auto mt-16">
          <div className="bg-gray-800 rounded-xl border border-gray-700 p-8 text-center">
            <div className="text-yellow-400 text-4xl mb-4">[LOCKED]</div>
            <h2 className="text-xl font-semibold text-white mb-2">
              Vault is Sealed
            </h2>
            <p className="text-gray-400 text-sm mb-6">
              Enter the vault passphrase to access your secrets
            </p>
            <div className="space-y-3">
              <input
                type="password"
                value={passphrase}
                onChange={(e) => setPassphrase(e.target.value)}
                placeholder="Vault passphrase"
                onKeyDown={(e) => e.key === 'Enter' && handleUnseal()}
                className="w-full px-3 py-2 bg-gray-700 border border-gray-600 rounded-lg text-white placeholder-gray-500 focus:outline-none focus:border-blue-500"
              />
              <button
                onClick={handleUnseal}
                disabled={unsealLoading || !passphrase}
                className="w-full px-4 py-2.5 bg-blue-600 hover:bg-blue-700 disabled:bg-blue-800 disabled:cursor-not-allowed text-white font-medium rounded-lg transition-colors"
              >
                {unsealLoading ? 'Unsealing...' : 'Unseal Vault'}
              </button>
              {unsealError && (
                <p className="text-red-400 text-sm">{unsealError}</p>
              )}
              {isAdmin && (
                <button
                  type="button"
                  onClick={() => setShowRotate(true)}
                  className="text-xs text-slate-400 hover:text-cyan-300 underline-offset-2 hover:underline"
                >
                  Rotate vault passphrase
                </button>
              )}
            </div>
          </div>
        </div>
        {showRotate && (
          <RotatePassphraseModal
            onClose={() => setShowRotate(false)}
            autoUnsealEnvPresent={autoUnsealEnvPresent}
          />
        )}
      </div>
    );
  }

  return (
    <div>
      <div className="flex items-center justify-between mb-6">
        <h1 className="text-2xl font-bold text-white">Key Vault</h1>
        <div className="flex items-center gap-2">
          {isAdmin && (
            <button
              onClick={() => setShowRotate(true)}
              className="px-3 py-2 bg-slate-700 hover:bg-slate-600 text-slate-200 text-sm font-medium rounded-lg transition-colors"
              title="Rotate the passphrase that seals the vault master key"
            >
              Rotate passphrase
            </button>
          )}
          <button
            onClick={() => setShowAdd(true)}
            className="px-4 py-2 bg-blue-600 hover:bg-blue-700 text-white font-medium rounded-lg transition-colors text-sm"
          >
            + Add Secret
          </button>
        </div>
      </div>

      {isLoading ? (
        <div className="text-center py-16 text-gray-400">
          Loading secrets...
        </div>
      ) : secrets && secrets.length > 0 ? (
        <div className="bg-gray-800 rounded-xl border border-gray-700 overflow-hidden">
          <div className="overflow-x-auto">
            <table className="w-full">
              <thead>
                <tr className="text-left text-xs text-gray-500 uppercase border-b border-gray-700">
                  <th className="px-6 py-3">Type</th>
                  <th className="px-6 py-3">Label</th>
                  <th className="px-6 py-3">Provider</th>
                  <th className="px-6 py-3">Created</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-gray-700">
                {secrets.map((secret: VaultSecret) => (
                  <tr
                    key={secret.id}
                    onClick={() => setViewSecretId(secret.id)}
                    className="text-sm cursor-pointer hover:bg-gray-700/50 transition-colors"
                  >
                    <td className="px-6 py-3">
                      <span className="inline-block px-2 py-0.5 text-xs font-medium bg-gray-700 text-gray-300 rounded">
                        {formatType(secret.secret_type)}
                      </span>
                    </td>
                    <td className="px-6 py-3 text-gray-200 font-medium">
                      {secret.label}
                    </td>
                    <td className="px-6 py-3 text-gray-400">
                      {secret.provider || '--'}
                    </td>
                    <td className="px-6 py-3 text-gray-500 whitespace-nowrap">
                      {new Date(secret.created_at).toLocaleDateString()}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      ) : (
        <div className="text-center py-16">
          <p className="text-gray-500 mb-4">No secrets stored yet</p>
          <button
            onClick={() => setShowAdd(true)}
            className="px-4 py-2 bg-blue-600 hover:bg-blue-700 text-white font-medium rounded-lg transition-colors text-sm"
          >
            Add Your First Secret
          </button>
        </div>
      )}

      {showAdd && <AddSecretModal onClose={() => setShowAdd(false)} />}

      {viewSecretId && (
        <ViewSecretModal
          secretId={viewSecretId}
          onClose={() => setViewSecretId(null)}
        />
      )}

      {showRotate && (
        <RotatePassphraseModal
          onClose={() => setShowRotate(false)}
          autoUnsealEnvPresent={autoUnsealEnvPresent}
        />
      )}
    </div>
  );
}
