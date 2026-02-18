import { apiRequest } from './client';
import type { AuthTokens, SetupStatus, User } from '../types';

export async function setupStatus(): Promise<SetupStatus> {
  return apiRequest<SetupStatus>('/api/setup/status');
}

export async function initializeVault(
  passphrase: string,
  admin_username: string,
  admin_display_name?: string,
): Promise<AuthTokens> {
  return apiRequest<AuthTokens>('/api/setup/initialize', {
    method: 'POST',
    body: JSON.stringify({
      passphrase,
      admin_username,
      admin_display_name: admin_display_name || admin_username,
    }),
  });
}

export async function webauthnRegisterStart(
  username: string,
): Promise<{ challenge_id: string; options: PublicKeyCredentialCreationOptions }> {
  return apiRequest('/api/auth/webauthn/register/begin', {
    method: 'POST',
    body: JSON.stringify({ username }),
  });
}

export async function webauthnRegisterFinish(
  challenge_id: string,
  credential: unknown,
): Promise<AuthTokens> {
  return apiRequest<AuthTokens>('/api/auth/webauthn/register/complete', {
    method: 'POST',
    body: JSON.stringify({ challenge_id, credential }),
  });
}

export async function webauthnLoginStart(
  username: string,
): Promise<{ challenge_id: string; options: PublicKeyCredentialRequestOptions }> {
  return apiRequest('/api/auth/webauthn/login/begin', {
    method: 'POST',
    body: JSON.stringify({ username }),
  });
}

export async function webauthnLoginFinish(
  challenge_id: string,
  credential: unknown,
): Promise<AuthTokens> {
  return apiRequest<AuthTokens>('/api/auth/webauthn/login/complete', {
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
