import { apiRequest } from './client';
import type {
  AuthTokens,
  BootstrapModelConfig,
  OidcProvider,
  SetupInitializationResult,
  SetupStatus,
  User,
} from '../types';

export async function setupStatus(): Promise<SetupStatus> {
  return apiRequest<SetupStatus>('/api/setup/status');
}

export async function initializeVault(
  passphrase: string,
  admin_username: string,
  bootstrap_model: BootstrapModelConfig,
  admin_display_name?: string,
): Promise<SetupInitializationResult> {
  return apiRequest<SetupInitializationResult>('/api/setup/initialize', {
    method: 'POST',
    body: JSON.stringify({
      passphrase,
      admin_username,
      admin_display_name: admin_display_name || admin_username,
      bootstrap_model,
    }),
  });
}

export async function webauthnRegisterStart(
  username: string,
): Promise<{ challenge_id: string; options: PublicKeyCredentialCreationOptions }> {
  const res = await apiRequest<{
    challenge_id: string;
    challenge: PublicKeyCredentialCreationOptions;
  }>('/api/auth/webauthn/register/start', {
    method: 'POST',
    body: JSON.stringify({ username }),
  });
  return { challenge_id: res.challenge_id, options: res.challenge };
}

export async function webauthnRegisterFinish(
  challenge_id: string,
  credential: unknown,
): Promise<AuthTokens> {
  return apiRequest<AuthTokens>('/api/auth/webauthn/register/finish', {
    method: 'POST',
    body: JSON.stringify({ challenge_id, credential }),
  });
}

export async function webauthnLoginStart(
  username: string,
): Promise<{ challenge_id: string; options: PublicKeyCredentialRequestOptions }> {
  const res = await apiRequest<{
    challenge_id: string;
    challenge: PublicKeyCredentialRequestOptions;
  }>('/api/auth/webauthn/login/start', {
    method: 'POST',
    body: JSON.stringify({ username }),
  });
  return { challenge_id: res.challenge_id, options: res.challenge };
}

export async function webauthnLoginFinish(
  challenge_id: string,
  credential: unknown,
): Promise<AuthTokens> {
  return apiRequest<AuthTokens>('/api/auth/webauthn/login/finish', {
    method: 'POST',
    body: JSON.stringify({ challenge_id, credential }),
  });
}

export async function refreshToken(
  refresh_token: string,
): Promise<AuthTokens> {
  return apiRequest<AuthTokens>('/api/auth/refresh', {
    method: 'POST',
    body: JSON.stringify({ refresh_token }),
  });
}

export async function logout(refresh_token: string): Promise<void> {
  return apiRequest<void>('/api/auth/logout', {
    method: 'POST',
    body: JSON.stringify({ refresh_token }),
  });
}

export async function getMe(): Promise<User> {
  return apiRequest<User>('/api/auth/me');
}

export async function listAuthOidcProviders(): Promise<OidcProvider[]> {
  const res = await apiRequest<{ providers: OidcProvider[] }>(
    '/api/auth/oidc/providers',
  );
  return res.providers;
}
