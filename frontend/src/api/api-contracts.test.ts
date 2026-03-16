import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  listUsers,
  listOidcProviders,
  queryAuditLog,
} from './admin';
import { listSecrets } from './vault';
import { webauthnLoginStart } from './auth';

type MockResponse = {
  ok: boolean;
  status: number;
  json: () => Promise<unknown>;
  text: () => Promise<string>;
};

function makeResponse(body: unknown, status = 200): MockResponse {
  return {
    ok: status >= 200 && status < 300,
    status,
    json: async () => body,
    text: async () => JSON.stringify(body),
  };
}

describe('frontend api contract adapters', () => {
  const fetchMock = vi.fn();

  beforeEach(() => {
    fetchMock.mockReset();
    vi.stubGlobal('fetch', fetchMock);
    const storage = new Map<string, string>();
    vi.stubGlobal('localStorage', {
      getItem: (key: string) => storage.get(key) ?? null,
      setItem: (key: string, value: string) => storage.set(key, value),
      removeItem: (key: string) => storage.delete(key),
      clear: () => storage.clear(),
      key: (index: number) => Array.from(storage.keys())[index] ?? null,
      get length() {
        return storage.size;
      },
    });
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('unwraps admin users envelope', async () => {
    fetchMock.mockResolvedValueOnce(
      makeResponse({ users: [{ id: 'u1', username: 'alice', role: 'admin' }] }),
    );

    const users = await listUsers();
    expect(users).toHaveLength(1);
    expect(users[0].username).toBe('alice');
    expect(fetchMock).toHaveBeenCalledWith(
      '/api/admin/users',
      expect.objectContaining({ headers: expect.any(Object) }),
    );
  });

  it('uses vault secrets endpoint and unwraps list', async () => {
    fetchMock.mockResolvedValueOnce(
      makeResponse({ secrets: [{ id: 's1', secret_type: 'api_key', label: 'k1' }] }),
    );

    const secrets = await listSecrets();
    expect(secrets).toHaveLength(1);
    expect(fetchMock).toHaveBeenCalledWith(
      '/api/vault/secrets',
      expect.objectContaining({ headers: expect.any(Object) }),
    );
  });

  it('maps webauthn login start challenge to options', async () => {
    fetchMock.mockResolvedValueOnce(
      makeResponse({
        challenge_id: 'c1',
        challenge: { challenge: 'abc', rpId: 'localhost' },
      }),
    );

    const result = await webauthnLoginStart('alice');
    expect(result.challenge_id).toBe('c1');
    expect(result.options).toEqual({ challenge: 'abc', rpId: 'localhost' });
    expect(fetchMock).toHaveBeenCalledWith(
      '/api/auth/webauthn/login/start',
      expect.objectContaining({ method: 'POST' }),
    );
  });

  it('allows username-less webauthn login start requests', async () => {
    fetchMock.mockResolvedValueOnce(
      makeResponse({
        challenge_id: 'c2',
        challenge: { challenge: 'def', rpId: 'localhost' },
      }),
    );

    await webauthnLoginStart();
    expect(fetchMock).toHaveBeenCalledWith(
      '/api/auth/webauthn/login/start',
      expect.objectContaining({
        method: 'POST',
        body: JSON.stringify({ username: undefined }),
      }),
    );
  });

  it('uses admin oidc provider namespace', async () => {
    fetchMock.mockResolvedValueOnce(
      makeResponse({ providers: [{ id: 'p1', name: 'Entra', enabled: true }] }),
    );

    const providers = await listOidcProviders();
    expect(providers).toHaveLength(1);
    expect(fetchMock).toHaveBeenCalledWith(
      '/api/admin/oidc/providers',
      expect.objectContaining({ headers: expect.any(Object) }),
    );
  });

  it('unwraps admin audit log envelope', async () => {
    fetchMock.mockResolvedValueOnce(
      makeResponse({ audit_log: [{ id: 'a1', action: 'auth.login' }] }),
    );

    const audit = await queryAuditLog({ limit: 5, offset: 0 });
    expect(audit).toHaveLength(1);
    expect(fetchMock).toHaveBeenCalledWith(
      '/api/admin/audit?offset=0&limit=5',
      expect.objectContaining({ headers: expect.any(Object) }),
    );
  });
});
