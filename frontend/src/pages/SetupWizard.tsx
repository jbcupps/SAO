import { useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { useQueryClient } from '@tanstack/react-query';
import { initializeVault } from '../api/auth';
import { ApiError } from '../api/client';
import type {
  BootstrapModelConfig,
  FrontierProvider,
  SetupInitializationResult,
} from '../types';

type Step =
  | 'welcome'
  | 'model'
  | 'passphrase'
  | 'initializing'
  | 'complete';

const PROVIDERS: Array<{ value: FrontierProvider; label: string }> = [
  { value: 'openai', label: 'OpenAI' },
  { value: 'anthropic', label: 'Anthropic' },
  { value: 'google', label: 'Google' },
];

const PROGRESS_INDEX: Record<Step, number> = {
  welcome: 0,
  model: 1,
  passphrase: 2,
  initializing: 2,
  complete: 3,
};

const DEFAULT_LOCAL_ADMIN_USERNAME = 'local-admin';
const DEFAULT_LOCAL_ADMIN_DISPLAY_NAME = 'Local Administrator';

function formatSetupError(error: unknown): string {
  if (error instanceof ApiError) {
    try {
      const parsed = JSON.parse(error.body) as { error?: string };
      if (parsed.error) {
        return parsed.error;
      }
    } catch {
      return error.message;
    }
  }

  return error instanceof Error ? error.message : 'Initialization failed';
}

export default function SetupWizard() {
  const navigate = useNavigate();
  const queryClient = useQueryClient();

  const [step, setStep] = useState<Step>('welcome');
  const [provider, setProvider] = useState<FrontierProvider>('openai');
  const [model, setModel] = useState('');
  const [apiKey, setApiKey] = useState('');
  const [entityName, setEntityName] = useState('sao-admin-entity');
  const [passphrase, setPassphrase] = useState('');
  const [passphraseConfirm, setPassphraseConfirm] = useState('');
  const [result, setResult] = useState<SetupInitializationResult | null>(null);
  const [error, setError] = useState('');

  const handleModelNext = () => {
    setError('');
    if (!model.trim()) {
      setError('Model identifier is required');
      return;
    }
    if (!apiKey.trim()) {
      setError('API key is required');
      return;
    }
    setStep('passphrase');
  };

  const handlePassphraseNext = () => {
    setError('');
    if (passphrase.length < 8) {
      setError('Passphrase must be at least 8 characters');
      return;
    }
    if (passphrase !== passphraseConfirm) {
      setError('Passphrases do not match');
      return;
    }
    void handleInitialize();
  };

  const handleInitialize = async () => {
    setError('');

    const bootstrapModel: BootstrapModelConfig = {
      provider,
      model: model.trim(),
      api_key: apiKey.trim(),
      entity_name: entityName.trim() || 'sao-admin-entity',
    };

    setStep('initializing');

    try {
      const response = await initializeVault(
        passphrase,
        DEFAULT_LOCAL_ADMIN_USERNAME,
        bootstrapModel,
        DEFAULT_LOCAL_ADMIN_DISPLAY_NAME,
      );
      setResult(response);
      await queryClient.invalidateQueries({ queryKey: ['setup-status'] });
      setStep('complete');
    } catch (err) {
      setStep('passphrase');
      setError(formatSetupError(err));
    }
  };

  const handleGoToLogin = () => {
    navigate('/login');
  };

  return (
    <div className="min-h-screen bg-gray-900 flex items-center justify-center p-4">
      <div className="w-full max-w-xl">
        <div className="bg-gray-800 rounded-xl shadow-2xl border border-gray-700 overflow-hidden">
          <div className="px-8 py-6 border-b border-gray-700 bg-gray-800">
            <h1 className="text-2xl font-bold text-white">SAO Setup</h1>
            <p className="text-sm text-gray-400 mt-1">
              Secure Agent Orchestrator - First Run Configuration
            </p>
          </div>

          <div className="px-8 py-6">
            {step === 'welcome' && (
              <div className="space-y-4">
                <p className="text-gray-300">
                  Welcome to the Secure Agent Orchestrator. This wizard will
                  guide you through the initial setup:
                </p>
                <ul className="text-gray-400 text-sm space-y-2 list-disc list-inside">
                  <li>Provision the SAO admin entity and connect its frontier model</li>
                  <li>Set a vault passphrase for encrypting all secrets</li>
                  <li>Create the internal local administrator identity automatically</li>
                  <li>Seed tracked bootstrap work items for Azure delivery</li>
                </ul>
                <p className="text-yellow-400 text-sm">
                  The model credential is validated during setup, encrypted in
                  the vault, and bound to the SAO admin entity that owns the
                  initial work queue.
                </p>
                <button
                  onClick={() => setStep('model')}
                  className="w-full mt-4 px-4 py-2.5 bg-blue-600 hover:bg-blue-700 text-white font-medium rounded-lg transition-colors"
                >
                  Begin Setup
                </button>
              </div>
            )}

            {step === 'model' && (
              <div className="space-y-4">
                <div>
                  <label className="block text-sm font-medium text-gray-300 mb-1">
                    Frontier Provider
                  </label>
                  <select
                    value={provider}
                    onChange={(e) =>
                      setProvider(e.target.value as FrontierProvider)
                    }
                    className="w-full px-3 py-2 bg-gray-700 border border-gray-600 rounded-lg text-white focus:outline-none focus:border-blue-500 focus:ring-1 focus:ring-blue-500"
                  >
                    {PROVIDERS.map((option) => (
                      <option key={option.value} value={option.value}>
                        {option.label}
                      </option>
                    ))}
                  </select>
                </div>
                <div>
                  <label className="block text-sm font-medium text-gray-300 mb-1">
                    Model Identifier
                  </label>
                  <input
                    type="text"
                    value={model}
                    onChange={(e) => setModel(e.target.value)}
                    placeholder="Provider model ID"
                    className="w-full px-3 py-2 bg-gray-700 border border-gray-600 rounded-lg text-white placeholder-gray-500 focus:outline-none focus:border-blue-500 focus:ring-1 focus:ring-blue-500"
                  />
                </div>
                <div>
                  <label className="block text-sm font-medium text-gray-300 mb-1">
                    API Key
                  </label>
                  <input
                    type="password"
                    value={apiKey}
                    onChange={(e) => setApiKey(e.target.value)}
                    placeholder="Credential used to validate and seed the SAO admin entity"
                    className="w-full px-3 py-2 bg-gray-700 border border-gray-600 rounded-lg text-white placeholder-gray-500 focus:outline-none focus:border-blue-500 focus:ring-1 focus:ring-blue-500"
                  />
                </div>
                <div>
                  <label className="block text-sm font-medium text-gray-300 mb-1">
                    SAO Admin Entity Name
                  </label>
                  <input
                    type="text"
                    value={entityName}
                    onChange={(e) => setEntityName(e.target.value)}
                    placeholder="sao-admin-entity"
                    className="w-full px-3 py-2 bg-gray-700 border border-gray-600 rounded-lg text-white placeholder-gray-500 focus:outline-none focus:border-blue-500 focus:ring-1 focus:ring-blue-500"
                  />
                  <p className="text-xs text-gray-500 mt-1">
                    This entity is created during first-run bootstrap and keeps
                    track of the initial work required to get SAO operational.
                  </p>
                </div>
                {error && <p className="text-red-400 text-sm">{error}</p>}
                <div className="flex gap-3">
                  <button
                    onClick={() => {
                      setError('');
                      setStep('welcome');
                    }}
                    className="px-4 py-2.5 bg-gray-700 hover:bg-gray-600 text-gray-300 font-medium rounded-lg transition-colors"
                  >
                    Back
                  </button>
                  <button
                    onClick={handleModelNext}
                    className="flex-1 px-4 py-2.5 bg-blue-600 hover:bg-blue-700 text-white font-medium rounded-lg transition-colors"
                  >
                    Next
                  </button>
                </div>
              </div>
            )}

            {step === 'passphrase' && (
              <div className="space-y-4">
                <div>
                  <label className="block text-sm font-medium text-gray-300 mb-1">
                    Vault Passphrase
                  </label>
                  <input
                    type="password"
                    value={passphrase}
                    onChange={(e) => setPassphrase(e.target.value)}
                    placeholder="Minimum 8 characters"
                    className="w-full px-3 py-2 bg-gray-700 border border-gray-600 rounded-lg text-white placeholder-gray-500 focus:outline-none focus:border-blue-500 focus:ring-1 focus:ring-blue-500"
                  />
                </div>
                <div>
                  <label className="block text-sm font-medium text-gray-300 mb-1">
                    Confirm Passphrase
                  </label>
                  <input
                    type="password"
                    value={passphraseConfirm}
                    onChange={(e) => setPassphraseConfirm(e.target.value)}
                    placeholder="Re-enter passphrase"
                    className="w-full px-3 py-2 bg-gray-700 border border-gray-600 rounded-lg text-white placeholder-gray-500 focus:outline-none focus:border-blue-500 focus:ring-1 focus:ring-blue-500"
                  />
                </div>
                <div className="rounded-lg border border-gray-700 bg-gray-800/70 p-4">
                  <p className="text-sm text-gray-300">
                    SAO admin entity: <span className="text-white">{entityName.trim() || 'sao-admin-entity'}</span>
                  </p>
                  <p className="text-xs text-gray-500 mt-1">
                    {provider} / {model.trim() || 'model required'}
                  </p>
                  <p className="text-xs text-gray-500 mt-2">
                    SAO will create the internal local administrator account automatically, so
                    you will not need to pick a separate username during setup.
                  </p>
                </div>
                {error && <p className="text-red-400 text-sm">{error}</p>}
                <div className="flex gap-3">
                  <button
                    onClick={() => {
                      setError('');
                      setStep('model');
                    }}
                    className="px-4 py-2.5 bg-gray-700 hover:bg-gray-600 text-gray-300 font-medium rounded-lg transition-colors"
                  >
                    Back
                  </button>
                  <button
                    onClick={handlePassphraseNext}
                    className="flex-1 px-4 py-2.5 bg-blue-600 hover:bg-blue-700 text-white font-medium rounded-lg transition-colors"
                  >
                    Initialize SAO
                  </button>
                </div>
              </div>
            )}

            {step === 'initializing' && (
              <div className="text-center py-8">
                <div className="inline-block w-10 h-10 border-4 border-blue-600 border-t-transparent rounded-full animate-spin mb-4"></div>
                <p className="text-gray-300">Initializing SAO...</p>
                <p className="text-gray-500 text-sm mt-1">
                  Validating the frontier model, encrypting the credential, and
                  provisioning the SAO admin entity and its initial work queue
                </p>
              </div>
            )}

            {step === 'complete' && (
              <div className="space-y-4 text-center py-4">
                <div className="text-green-400 text-4xl mb-2">[OK]</div>
                <h2 className="text-xl font-semibold text-white">
                  Setup Complete
                </h2>
                <p className="text-gray-400">
                  SAO has been initialized. The SAO admin entity is provisioned,
                  its credential is sealed in the vault, and the first tracked
                  work items are ready.
                </p>
                {result && (
                  <div className="rounded-lg border border-gray-700 bg-gray-800/70 p-4 text-left space-y-3">
                    <p className="text-sm text-gray-300">
                      Entity: <span className="text-white">{result.admin_entity.name}</span>
                    </p>
                    <p className="text-xs text-gray-500 mt-1">
                      {result.admin_entity.provider} / {result.admin_entity.model}
                    </p>
                    <div className="border-t border-gray-700 pt-3">
                      <p className="text-xs font-semibold uppercase tracking-wide text-gray-400">
                        Initial Work Queue
                      </p>
                      <div className="mt-2 space-y-2">
                        {result.work_items.slice(0, 4).map((item) => (
                          <div
                            key={item.id}
                            className="rounded-lg border border-gray-700 bg-gray-900/40 px-3 py-2"
                          >
                            <p className="text-sm text-white">{item.title}</p>
                            <p className="text-xs text-gray-500 mt-1">
                              {item.description}
                            </p>
                          </div>
                        ))}
                      </div>
                    </div>
                  </div>
                )}
                <button
                  onClick={handleGoToLogin}
                  className="w-full mt-4 px-4 py-2.5 bg-blue-600 hover:bg-blue-700 text-white font-medium rounded-lg transition-colors"
                >
                  Go to Login
                </button>
              </div>
            )}
          </div>

          <div className="px-8 py-4 border-t border-gray-700 bg-gray-800/50">
            <div className="flex gap-2">
              {['welcome', 'model', 'passphrase', 'complete'].map(
                (progressStep, index) => (
                  <div
                    key={progressStep}
                    className={`h-1.5 flex-1 rounded-full ${
                      PROGRESS_INDEX[step] >= index
                        ? 'bg-blue-600'
                        : 'bg-gray-700'
                    }`}
                  />
                ),
              )}
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
