import { useState, useEffect } from 'react';
import { useNavigate, useLocation } from 'react-router-dom';
import { useAuth } from '../hooks/useAuth';
import { webauthnLoginStart, webauthnLoginFinish } from '../api/auth';
import { listOidcProviders } from '../api/admin';
import { beginAuthentication } from '../lib/webauthn';
import type { OidcProvider } from '../types';

export default function Login() {
  const navigate = useNavigate();
  const location = useLocation();
  const { login } = useAuth();

  const [username, setUsername] = useState('');
  const [error, setError] = useState('');
  const [loading, setLoading] = useState(false);
  const [oidcProviders, setOidcProviders] = useState<OidcProvider[]>([]);

  const from = (location.state as { from?: { pathname: string } })?.from?.pathname || '/';

  useEffect(() => {
    listOidcProviders()
      .then((providers) =>
        setOidcProviders(providers.filter((p) => p.enabled)),
      )
      .catch(() => {
        // OIDC providers not available, that is fine
      });
  }, []);

  const handleWebAuthnLogin = async () => {
    setError('');
    if (!username.trim()) {
      setError('Please enter your username');
      return;
    }

    setLoading(true);
    try {
      const { challenge_id, options } = await webauthnLoginStart(
        username.trim(),
      );
      const credential = await beginAuthentication(options as never);
      const tokens = await webauthnLoginFinish(challenge_id, credential);
      await login(tokens);
      navigate(from, { replace: true });
    } catch (err) {
      setError(
        err instanceof Error
          ? err.message
          : 'Authentication failed. Please try again.',
      );
    } finally {
      setLoading(false);
    }
  };

  const handleOidcLogin = (providerId: string) => {
    window.location.href = `/api/auth/oidc/${providerId}/login`;
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter') {
      handleWebAuthnLogin();
    }
  };

  return (
    <div className="min-h-screen bg-gray-900 flex items-center justify-center p-4">
      <div className="w-full max-w-md">
        <div className="bg-gray-800 rounded-xl shadow-2xl border border-gray-700 overflow-hidden">
          {/* Header */}
          <div className="px-8 py-6 border-b border-gray-700">
            <h1 className="text-2xl font-bold text-white">SAO</h1>
            <p className="text-sm text-gray-400 mt-1">
              Sign in to Secure Agent Orchestrator
            </p>
          </div>

          {/* Content */}
          <div className="px-8 py-6 space-y-5">
            {/* Username */}
            <div>
              <label className="block text-sm font-medium text-gray-300 mb-1">
                Username
              </label>
              <input
                type="text"
                value={username}
                onChange={(e) => setUsername(e.target.value)}
                onKeyDown={handleKeyDown}
                placeholder="Enter your username"
                autoFocus
                className="w-full px-3 py-2 bg-gray-700 border border-gray-600 rounded-lg text-white placeholder-gray-500 focus:outline-none focus:border-blue-500 focus:ring-1 focus:ring-blue-500"
              />
            </div>

            {/* WebAuthn Login */}
            <button
              onClick={handleWebAuthnLogin}
              disabled={loading}
              className="w-full px-4 py-2.5 bg-blue-600 hover:bg-blue-700 disabled:bg-blue-800 disabled:cursor-not-allowed text-white font-medium rounded-lg transition-colors flex items-center justify-center gap-2"
            >
              {loading ? (
                <>
                  <div className="w-4 h-4 border-2 border-white border-t-transparent rounded-full animate-spin"></div>
                  Authenticating...
                </>
              ) : (
                'Login with Windows Hello'
              )}
            </button>

            {error && (
              <div className="p-3 bg-red-900/30 border border-red-800 rounded-lg">
                <p className="text-red-400 text-sm">{error}</p>
              </div>
            )}

            {/* OIDC Providers */}
            {oidcProviders.length > 0 && (
              <>
                <div className="flex items-center gap-3">
                  <div className="flex-1 h-px bg-gray-700"></div>
                  <span className="text-xs text-gray-500 uppercase">
                    or continue with
                  </span>
                  <div className="flex-1 h-px bg-gray-700"></div>
                </div>

                <div className="space-y-2">
                  {oidcProviders.map((provider) => (
                    <button
                      key={provider.id}
                      onClick={() => handleOidcLogin(provider.id)}
                      className="w-full px-4 py-2.5 bg-gray-700 hover:bg-gray-600 text-gray-200 font-medium rounded-lg transition-colors border border-gray-600"
                    >
                      Sign in with {provider.name}
                    </button>
                  ))}
                </div>
              </>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
