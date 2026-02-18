import { useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { useQueryClient } from '@tanstack/react-query';
import { initializeVault } from '../api/auth';

type Step = 'welcome' | 'passphrase' | 'admin' | 'initializing' | 'complete';

export default function SetupWizard() {
  const navigate = useNavigate();
  const queryClient = useQueryClient();

  const [step, setStep] = useState<Step>('welcome');
  const [passphrase, setPassphrase] = useState('');
  const [passphraseConfirm, setPassphraseConfirm] = useState('');
  const [adminUsername, setAdminUsername] = useState('');
  const [adminDisplayName, setAdminDisplayName] = useState('');
  const [error, setError] = useState('');

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
    setStep('admin');
  };

  const handleInitialize = async () => {
    setError('');
    if (!adminUsername.trim()) {
      setError('Username is required');
      return;
    }

    setStep('initializing');

    try {
      await initializeVault(
        passphrase,
        adminUsername.trim(),
        adminDisplayName.trim() || undefined,
      );
      await queryClient.invalidateQueries({ queryKey: ['setup-status'] });
      setStep('complete');
    } catch (err) {
      setStep('admin');
      setError(
        err instanceof Error ? err.message : 'Initialization failed',
      );
    }
  };

  const handleGoToLogin = () => {
    navigate('/login');
  };

  return (
    <div className="min-h-screen bg-gray-900 flex items-center justify-center p-4">
      <div className="w-full max-w-lg">
        {/* Card */}
        <div className="bg-gray-800 rounded-xl shadow-2xl border border-gray-700 overflow-hidden">
          {/* Header */}
          <div className="px-8 py-6 border-b border-gray-700 bg-gray-800">
            <h1 className="text-2xl font-bold text-white">SAO Setup</h1>
            <p className="text-sm text-gray-400 mt-1">
              Secure Agent Orchestrator - First Run Configuration
            </p>
          </div>

          {/* Content */}
          <div className="px-8 py-6">
            {/* Step: Welcome */}
            {step === 'welcome' && (
              <div className="space-y-4">
                <p className="text-gray-300">
                  Welcome to the Secure Agent Orchestrator. This wizard will
                  guide you through the initial setup:
                </p>
                <ul className="text-gray-400 text-sm space-y-2 list-disc list-inside">
                  <li>Set a vault passphrase for encrypting all secrets</li>
                  <li>Create the initial administrator account</li>
                  <li>Initialize the master signing key</li>
                </ul>
                <p className="text-yellow-400 text-sm">
                  Keep the vault passphrase safe. It is required to unseal the
                  vault after restarts.
                </p>
                <button
                  onClick={() => setStep('passphrase')}
                  className="w-full mt-4 px-4 py-2.5 bg-blue-600 hover:bg-blue-700 text-white font-medium rounded-lg transition-colors"
                >
                  Begin Setup
                </button>
              </div>
            )}

            {/* Step: Passphrase */}
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
                {error && (
                  <p className="text-red-400 text-sm">{error}</p>
                )}
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
                    onClick={handlePassphraseNext}
                    className="flex-1 px-4 py-2.5 bg-blue-600 hover:bg-blue-700 text-white font-medium rounded-lg transition-colors"
                  >
                    Next
                  </button>
                </div>
              </div>
            )}

            {/* Step: Admin Account */}
            {step === 'admin' && (
              <div className="space-y-4">
                <div>
                  <label className="block text-sm font-medium text-gray-300 mb-1">
                    Admin Username
                  </label>
                  <input
                    type="text"
                    value={adminUsername}
                    onChange={(e) => setAdminUsername(e.target.value)}
                    placeholder="admin"
                    className="w-full px-3 py-2 bg-gray-700 border border-gray-600 rounded-lg text-white placeholder-gray-500 focus:outline-none focus:border-blue-500 focus:ring-1 focus:ring-blue-500"
                  />
                </div>
                <div>
                  <label className="block text-sm font-medium text-gray-300 mb-1">
                    Display Name (optional)
                  </label>
                  <input
                    type="text"
                    value={adminDisplayName}
                    onChange={(e) => setAdminDisplayName(e.target.value)}
                    placeholder="Administrator"
                    className="w-full px-3 py-2 bg-gray-700 border border-gray-600 rounded-lg text-white placeholder-gray-500 focus:outline-none focus:border-blue-500 focus:ring-1 focus:ring-blue-500"
                  />
                </div>
                {error && (
                  <p className="text-red-400 text-sm">{error}</p>
                )}
                <div className="flex gap-3">
                  <button
                    onClick={() => {
                      setError('');
                      setStep('passphrase');
                    }}
                    className="px-4 py-2.5 bg-gray-700 hover:bg-gray-600 text-gray-300 font-medium rounded-lg transition-colors"
                  >
                    Back
                  </button>
                  <button
                    onClick={handleInitialize}
                    className="flex-1 px-4 py-2.5 bg-blue-600 hover:bg-blue-700 text-white font-medium rounded-lg transition-colors"
                  >
                    Initialize SAO
                  </button>
                </div>
              </div>
            )}

            {/* Step: Initializing */}
            {step === 'initializing' && (
              <div className="text-center py-8">
                <div className="inline-block w-10 h-10 border-4 border-blue-600 border-t-transparent rounded-full animate-spin mb-4"></div>
                <p className="text-gray-300">Initializing vault...</p>
                <p className="text-gray-500 text-sm mt-1">
                  Generating master key and encrypting vault
                </p>
              </div>
            )}

            {/* Step: Complete */}
            {step === 'complete' && (
              <div className="space-y-4 text-center py-4">
                <div className="text-green-400 text-4xl mb-2">[OK]</div>
                <h2 className="text-xl font-semibold text-white">
                  Setup Complete
                </h2>
                <p className="text-gray-400">
                  SAO has been initialized. You can now log in with your
                  admin account.
                </p>
                <button
                  onClick={handleGoToLogin}
                  className="w-full mt-4 px-4 py-2.5 bg-blue-600 hover:bg-blue-700 text-white font-medium rounded-lg transition-colors"
                >
                  Go to Login
                </button>
              </div>
            )}
          </div>

          {/* Progress indicator */}
          <div className="px-8 py-4 border-t border-gray-700 bg-gray-800/50">
            <div className="flex gap-2">
              {['welcome', 'passphrase', 'admin', 'complete'].map(
                (s, i) => (
                  <div
                    key={s}
                    className={`h-1.5 flex-1 rounded-full ${
                      ['welcome', 'passphrase', 'admin', 'initializing', 'complete'].indexOf(step) >= i
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
