import { useState } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import {
  createInstallerSource,
  deleteInstallerSource,
  listInstallerSources,
  probeInstallerSource,
  setDefaultInstallerSource,
} from '../api/installer-sources';
import type { InstallerSource, ProbeInstallerResult } from '../types';

export default function AdminInstallerSourcesPage() {
  const queryClient = useQueryClient();
  const { data: sources, isLoading } = useQuery({
    queryKey: ['installer-sources'],
    queryFn: listInstallerSources,
  });

  const [showCreate, setShowCreate] = useState(false);

  return (
    <div>
      <div className="flex items-center justify-between mb-2">
        <h1 className="text-2xl font-bold text-white">OrionII Installer Sources</h1>
        <button
          onClick={() => setShowCreate((v) => !v)}
          className="px-4 py-2 bg-blue-600 hover:bg-blue-700 text-white text-sm rounded-lg"
        >
          {showCreate ? 'Cancel' : '+ Register installer source'}
        </button>
      </div>
      <p className="text-sm text-gray-400 mb-6 max-w-3xl">
        SAO downloads each registered MSI on first reference, verifies its sha256, and caches
        it under <span className="font-mono">{`SAO_DATA_DIR/installers/<sha>/`}</span>. When a
        user creates an entity, SAO pins the current default source's coordinates onto that
        agent so re-downloading the bundle yields a byte-identical MSI even after the default
        rolls forward. The convention is to point the default at GitHub Releases'{' '}
        <span className="font-mono">/releases/latest/download/&lt;asset&gt;</span> URL — that
        way "make latest the default" is a one-click action whenever a new OrionII release
        ships.
      </p>

      {showCreate && (
        <CreateForm
          onCreated={async () => {
            setShowCreate(false);
            await queryClient.invalidateQueries({ queryKey: ['installer-sources'] });
          }}
        />
      )}

      {isLoading ? (
        <div className="text-gray-400">Loading...</div>
      ) : (sources ?? []).length === 0 ? (
        <div className="text-center py-16 text-gray-500">
          No installer sources registered yet. Add one to enable self-serve bundle downloads.
        </div>
      ) : (
        <div className="grid grid-cols-1 lg:grid-cols-2 gap-4">
          {(sources ?? []).map((s) => (
            <SourceCard
              key={s.id}
              source={s}
              onChanged={async () => {
                await queryClient.invalidateQueries({ queryKey: ['installer-sources'] });
              }}
            />
          ))}
        </div>
      )}
    </div>
  );
}

// Convention: the default installer source points at the latest tagged release in the
// jbcupps/OrionII GitHub repo. Admins can override the URL to pin a specific tag or
// switch to an internal mirror.
const DEFAULT_RELEASE_URL =
  'https://github.com/jbcupps/OrionII/releases/latest/download/OrionII_0.1.0_x64_en-US.msi';

function CreateForm({ onCreated }: { onCreated: () => Promise<void> | void }) {
  const [url, setUrl] = useState(DEFAULT_RELEASE_URL);
  const [filename, setFilename] = useState('OrionII_0.1.0_x64_en-US.msi');
  const [version, setVersion] = useState('latest');
  const [sha, setSha] = useState('');
  const [isDefault, setIsDefault] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [probe, setProbe] = useState<ProbeInstallerResult | null>(null);

  const handleProbe = async () => {
    setError('');
    setProbe(null);
    if (!url.trim()) {
      setError('URL is required');
      return;
    }
    setBusy(true);
    try {
      const result = await probeInstallerSource(url.trim(), 'orion-msi');
      setProbe(result);
      if (!sha.trim()) setSha(result.sha256);
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Probe failed');
    } finally {
      setBusy(false);
    }
  };

  const handleCreate = async () => {
    setError('');
    setBusy(true);
    try {
      await createInstallerSource({
        url: url.trim(),
        filename: filename.trim(),
        version: version.trim(),
        expected_sha256: sha.trim().toLowerCase(),
        is_default: isDefault,
      });
      await onCreated();
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Create failed');
    } finally {
      setBusy(false);
    }
  };

  const formatLooksWrong = probe !== null && probe.format_ok === false;
  const formatVerified = probe !== null && probe.format_ok === true;
  const canRegister =
    !busy && !!url.trim() && !!sha.trim() && !formatLooksWrong;

  return (
    <div className="bg-gray-800 rounded-xl border border-gray-700 p-5 mb-6 space-y-3">
      <h2 className="text-lg font-semibold text-white">Register installer source</h2>
      <div className="rounded-lg border border-slate-700 bg-slate-900/40 p-3 text-xs text-slate-400">
        <p className="font-medium text-slate-300">Use a built-MSI release asset URL.</p>
        <p className="mt-1">
          GitHub Releases publishes built artifacts at{' '}
          <span className="font-mono text-slate-200">
            /releases/download/&lt;tag&gt;/&lt;asset&gt;.msi
          </span>{' '}
          or the convenience alias{' '}
          <span className="font-mono text-slate-200">
            /releases/latest/download/&lt;asset&gt;.msi
          </span>
          . URLs that look like{' '}
          <span className="font-mono text-amber-300">
            /archive/refs/tags/&lt;tag&gt;.zip
          </span>{' '}
          are GitHub-generated source-code archives, not installers, and SAO will
          refuse to register them.
        </p>
      </div>
      <div>
        <label className="block text-xs text-gray-400 mb-1">Download URL</label>
        <div className="flex gap-2">
          <input
            type="text"
            value={url}
            onChange={(e) => setUrl(e.target.value)}
            placeholder="https://github.com/jbcupps/OrionII/releases/download/v0.1.0/OrionII_0.1.0_x64_en-US.msi"
            className="flex-1 px-3 py-2 bg-gray-700 border border-gray-600 rounded-lg text-white placeholder-gray-500 focus:outline-none focus:border-blue-500 font-mono text-xs"
          />
          <button
            onClick={handleProbe}
            disabled={busy}
            className="px-3 py-2 bg-gray-700 hover:bg-gray-600 disabled:bg-gray-800 text-white text-sm rounded-lg"
          >
            Probe sha256
          </button>
        </div>
        {probe && (
          <div className="mt-2 space-y-1 text-xs font-mono break-all">
            <p className={formatVerified ? 'text-green-400' : 'text-amber-300'}>
              sha256: {probe.sha256}
            </p>
            {typeof probe.size_bytes === 'number' && (
              <p className="text-slate-400">
                size: {probe.size_bytes.toLocaleString()} bytes
              </p>
            )}
            {probe.format_hint && (
              <p
                className={
                  formatVerified
                    ? 'text-green-400 font-sans'
                    : 'text-amber-300 font-sans'
                }
              >
                format: {probe.format_hint}
              </p>
            )}
          </div>
        )}
        {formatLooksWrong && (
          <div className="mt-2 rounded border border-red-800 bg-red-900/30 p-3 text-xs text-red-200">
            <p className="font-semibold">
              This URL did not return a valid Windows Installer (.msi).
            </p>
            <p className="mt-1">
              {probe?.format_error ||
                'The first bytes of the response do not match the OLE2 magic that msiexec needs.'}
            </p>
            <p className="mt-2">
              Likely cause: the URL is a GitHub source-tarball
              (<span className="font-mono">/archive/refs/tags/...zip</span>) or a
              release asset that has not been built/published yet. Use a{' '}
              <span className="font-mono">/releases/download/&lt;tag&gt;/&lt;file&gt;.msi</span>{' '}
              URL instead. Registration is blocked until this resolves.
            </p>
          </div>
        )}
      </div>
      <div className="grid grid-cols-2 gap-3">
        <div>
          <label className="block text-xs text-gray-400 mb-1">Filename (in bundle)</label>
          <input
            type="text"
            value={filename}
            onChange={(e) => setFilename(e.target.value)}
            className="w-full px-3 py-2 bg-gray-700 border border-gray-600 rounded-lg text-white font-mono text-sm"
          />
        </div>
        <div>
          <label className="block text-xs text-gray-400 mb-1">Version label</label>
          <input
            type="text"
            value={version}
            onChange={(e) => setVersion(e.target.value)}
            className="w-full px-3 py-2 bg-gray-700 border border-gray-600 rounded-lg text-white font-mono text-sm"
          />
        </div>
      </div>
      <div>
        <label className="block text-xs text-gray-400 mb-1">Expected sha256 (64 hex)</label>
        <input
          type="text"
          value={sha}
          onChange={(e) => setSha(e.target.value)}
          placeholder="probe above to autofill, or paste a known-good digest"
          className="w-full px-3 py-2 bg-gray-700 border border-gray-600 rounded-lg text-white font-mono text-xs"
        />
      </div>
      <label className="flex items-center gap-2 text-sm text-gray-300">
        <input
          type="checkbox"
          checked={isDefault}
          onChange={(e) => setIsDefault(e.target.checked)}
        />
        Make this the default — new agents will pin to it.
      </label>
      {error && (
        <div className="rounded border border-red-800 bg-red-900/30 p-2 text-xs text-red-300">
          {error}
        </div>
      )}
      <div className="flex justify-end">
        <button
          onClick={handleCreate}
          disabled={!canRegister}
          className="px-4 py-2 bg-blue-600 hover:bg-blue-700 disabled:bg-blue-900 disabled:cursor-not-allowed text-white text-sm rounded-lg"
        >
          {busy
            ? 'Working...'
            : formatLooksWrong
              ? 'Format check failed'
              : 'Register + warm cache'}
        </button>
      </div>
    </div>
  );
}

function SourceCard({
  source,
  onChanged,
}: {
  source: InstallerSource;
  onChanged: () => Promise<void> | void;
}) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');

  const handleSetDefault = async () => {
    setError('');
    setBusy(true);
    try {
      await setDefaultInstallerSource(source.id);
      await onChanged();
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Failed');
    } finally {
      setBusy(false);
    }
  };

  const handleDelete = async () => {
    setError('');
    setBusy(true);
    try {
      await deleteInstallerSource(source.id);
      await onChanged();
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Failed');
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="bg-gray-800 rounded-xl border border-gray-700 p-5">
      <div className="flex items-start justify-between mb-2">
        <div>
          <h3 className="text-white font-semibold font-mono">{source.filename}</h3>
          <p className="text-xs text-gray-500">v{source.version} · {source.kind}</p>
        </div>
        {source.is_default ? (
          <span className="text-xs px-2 py-0.5 bg-green-700 text-green-100 rounded">default</span>
        ) : (
          <button
            onClick={handleSetDefault}
            disabled={busy}
            className="text-xs px-2 py-0.5 text-blue-300 hover:text-blue-200"
          >
            Make default
          </button>
        )}
      </div>
      <p className="text-xs text-gray-400 break-all mb-2">
        <span className="text-gray-500">URL:</span> {source.url}
      </p>
      <p className="text-xs text-gray-400 break-all mb-3 font-mono">
        <span className="text-gray-500 font-sans">sha256:</span> {source.expected_sha256}
      </p>
      {error && (
        <div className="rounded border border-red-800 bg-red-900/30 p-2 text-xs text-red-300 mb-2">
          {error}
        </div>
      )}
      <div className="flex justify-end gap-2 pt-2 border-t border-gray-700">
        <button
          onClick={handleDelete}
          disabled={busy}
          className="text-xs px-3 py-1.5 text-red-400 hover:text-red-300"
        >
          Delete
        </button>
      </div>
    </div>
  );
}
