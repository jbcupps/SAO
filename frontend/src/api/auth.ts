import { apiRequest } from './client';
import type {
  OidcProvider,
  SetupStatus,
  User,
} from '../types';
import type {
  PublicKeyCredentialCreationOptionsJSON,
  PublicKeyCredentialRequestOptionsJSON,
} from '@simplewebauthn/types';

export async function setupStatus(): Promise<SetupStatus> {
  return apiRequest<SetupStatus>('/api/setup/status');
}

export async function webauthnRegisterStart(
  username: string,
): Promise<{ challenge_id: string; options: PublicKeyCredentialCreationOptionsJSON | { publicKey: PublicKeyCredentialCreationOptionsJSON } }> {
  const res = await apiRequest<{
    challenge_id: string;
    challenge: PublicKeyCredentialCreationOptionsJSON | { publicKey: PublicKeyCredentialCreationOptionsJSON };
  }>('/api/auth/webauthn/register/start', {
    method: 'POST',
    body: JSON.stringify({ username }),
  });
  return { challenge_id: res.challenge_id, options: res.challenge };
}

export async function webauthnRegisterFinish(
  challenge_id: string,
  credential: unknown,
): Promise<{ status: string; credential_id: string }> {
  return apiRequest<{ status: string; credential_id: string }>(
    '/api/auth/webauthn/register/finish',
    {
      method: 'POST',
      body: JSON.stringify({ challenge_id, credential }),
    },
  );
}

export async function localWebauthnRegisterStart(
  username?: string,
): Promise<{ challenge_id: string; options: PublicKeyCredentialCreationOptionsJSON | { publicKey: PublicKeyCredentialCreationOptionsJSON } }> {
  const res = await apiRequest<{
    challenge_id: string;
    challenge: PublicKeyCredentialCreationOptionsJSON | { publicKey: PublicKeyCredentialCreationOptionsJSON };
  }>('/api/auth/webauthn/local/register/start', {
    method: 'POST',
    body: JSON.stringify({ username }),
  });
  return { challenge_id: res.challenge_id, options: res.challenge };
}

export async function localWebauthnRegisterFinish(
  challenge_id: string,
  credential: unknown,
): Promise<{ status: string; credential_id: string }> {
  return apiRequest<{ status: string; credential_id: string }>(
    '/api/auth/webauthn/local/register/finish',
    {
      method: 'POST',
      body: JSON.stringify({ challenge_id, credential }),
    },
  );
}

export async function webauthnLoginStart(
  username?: string,
): Promise<{ challenge_id: string; options: PublicKeyCredentialRequestOptionsJSON | { publicKey: PublicKeyCredentialRequestOptionsJSON } }> {
  const res = await apiRequest<{
    challenge_id: string;
    challenge: PublicKeyCredentialRequestOptionsJSON | { publicKey: PublicKeyCredentialRequestOptionsJSON };
  }>('/api/auth/webauthn/login/start', {
    method: 'POST',
    body: JSON.stringify({ username }),
  });
  return { challenge_id: res.challenge_id, options: res.challenge };
}

export async function webauthnLoginFinish(
  challenge_id: string,
  credential: unknown,
): Promise<{ authenticated: boolean; user: User }> {
  return apiRequest<{ authenticated: boolean; user: User }>(
    '/api/auth/webauthn/login/finish',
    {
      method: 'POST',
      body: JSON.stringify({ challenge_id, credential }),
    },
  );
}

export async function logout(): Promise<void> {
  return apiRequest<void>('/api/auth/logout', {
    method: 'POST',
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
