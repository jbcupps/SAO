import type { SetupStatus } from '../types';

interface BootstrapRequiredPageProps {
  status?: SetupStatus;
}

export default function BootstrapRequiredPage({
  status,
}: BootstrapRequiredPageProps) {
  const command =
    status?.recommended_installer?.command ||
    'docker build -f installer/Dockerfile -t sao-installer installer && docker run --rm -it -e ANTHROPIC_API_KEY=<your-key> sao-installer';

  return (
    <div className="min-h-screen bg-slate-950 text-slate-100 flex items-center justify-center px-4 py-10">
      <div className="w-full max-w-4xl rounded-3xl border border-slate-800 bg-[radial-gradient(circle_at_top,_rgba(56,189,248,0.15),_transparent_45%),linear-gradient(180deg,_rgba(15,23,42,0.98),_rgba(2,6,23,1))] shadow-2xl overflow-hidden">
        <div className="px-8 py-8 border-b border-slate-800">
          <p className="text-xs uppercase tracking-[0.35em] text-cyan-300">
            Bootstrap Required
          </p>
          <h1 className="mt-3 text-3xl font-semibold text-white">
            SAO now bootstraps through the governed installer, not an in-app setup wizard.
          </h1>
          <p className="mt-3 max-w-2xl text-sm text-slate-300">
            This environment is not fully initialized yet. Run the standalone
            conversational installer and come back once bootstrap completes.
          </p>
        </div>

        <div className="grid gap-0 lg:grid-cols-[1.4fr_1fr]">
          <div className="px-8 py-8 border-b lg:border-b-0 lg:border-r border-slate-800">
            <h2 className="text-sm font-semibold uppercase tracking-[0.2em] text-slate-400">
              One-command start
            </h2>
            <pre className="mt-4 rounded-2xl border border-slate-800 bg-slate-950/80 p-5 text-sm text-cyan-100 overflow-x-auto whitespace-pre-wrap">
              <code>{command}</code>
            </pre>
            <p className="mt-4 text-sm text-slate-400">
              The installer handles Azure sign-in, subscription targeting,
              governed deployment steps, and post-deploy verification in a
              traceable conversation.
            </p>
          </div>

          <div className="px-8 py-8">
            <h2 className="text-sm font-semibold uppercase tracking-[0.2em] text-slate-400">
              Current state
            </h2>
            <div className="mt-4 space-y-3">
              <StateRow label="Initialized" value={status?.initialized ? 'Yes' : 'No'} />
              <StateRow label="Users present" value={status?.has_users ? 'Yes' : 'No'} />
              <StateRow
                label="Bootstrap mode"
                value={status?.bootstrap_mode || 'installer_required'}
              />
            </div>
            <div className="mt-8 rounded-2xl border border-slate-800 bg-slate-900/70 p-5">
              <p className="text-sm font-medium text-white">
                What changed
              </p>
              <p className="mt-2 text-sm text-slate-400">
                Local browser setup was removed so SAO can stay aligned with the
                Entra-first, zero-trust Azure bootstrap model.
              </p>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

function StateRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex items-center justify-between rounded-xl border border-slate-800 bg-slate-900/40 px-4 py-3">
      <span className="text-sm text-slate-400">{label}</span>
      <span className="text-sm font-medium text-white">{value}</span>
    </div>
  );
}
