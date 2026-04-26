const BASE_URL = import.meta.env.VITE_API_BASE_URL || '';
const CSRF_COOKIE_NAME = 'sao_csrf';

let isRefreshing = false;
let refreshSubscribers: Array<() => void> = [];
const NO_REFRESH_PATHS = new Set(['/api/auth/me', '/api/auth/refresh']);

function onRefreshed() {
  refreshSubscribers.forEach((cb) => cb());
  refreshSubscribers = [];
}

function addRefreshSubscriber(cb: () => void) {
  refreshSubscribers.push(cb);
}

function getCookie(name: string): string | null {
  if (typeof document === 'undefined') {
    return null;
  }

  const match = document.cookie
    .split(';')
    .map((entry) => entry.trim())
    .find((entry) => entry.startsWith(`${name}=`));

  return match ? decodeURIComponent(match.slice(name.length + 1)) : null;
}

function withCsrfHeader(headers: Record<string, string>, method?: string) {
  const normalizedMethod = method?.toUpperCase() || 'GET';
  if (['GET', 'HEAD', 'OPTIONS'].includes(normalizedMethod)) {
    return headers;
  }

  const csrfToken = getCookie(CSRF_COOKIE_NAME);
  if (csrfToken) {
    headers['X-CSRF-Token'] = csrfToken;
  }
  return headers;
}

async function attemptSessionRefresh(): Promise<boolean> {
  try {
    const headers = withCsrfHeader(
      { 'Content-Type': 'application/json' },
      'POST',
    );
    const res = await fetch(`${BASE_URL}/api/auth/refresh`, {
      method: 'POST',
      credentials: 'include',
      headers,
    });
    return res.ok;
  } catch {
    return false;
  }
}

export async function apiRequest<T>(
  path: string,
  options: RequestInit = {},
): Promise<T> {
  const headers: Record<string, string> = {
    ...(options.body ? { 'Content-Type': 'application/json' } : {}),
    ...(options.headers as Record<string, string>),
  };

  withCsrfHeader(headers, options.method);

  const res = await fetch(`${BASE_URL}${path}`, {
    ...options,
    credentials: 'include',
    headers,
  });

  if (res.status === 401 && !NO_REFRESH_PATHS.has(path)) {
    if (!isRefreshing) {
      isRefreshing = true;
      const refreshed = await attemptSessionRefresh();
      isRefreshing = false;

      if (refreshed) {
        onRefreshed();
        const retryHeaders = { ...headers };
        withCsrfHeader(retryHeaders, options.method);
        const retryRes = await fetch(`${BASE_URL}${path}`, {
          ...options,
          credentials: 'include',
          headers: retryHeaders,
        });
        if (!retryRes.ok) {
          const errorBody = await retryRes.text();
          throw new ApiError(retryRes.status, errorBody);
        }
        if (retryRes.status === 204) {
          return undefined as T;
        }
        return retryRes.json();
      }

      window.location.href = '/login';
      throw new ApiError(401, 'Session expired');
    }

    return new Promise<T>((resolve, reject) => {
      addRefreshSubscriber(async () => {
        try {
          const retryHeaders = { ...headers };
          withCsrfHeader(retryHeaders, options.method);
          const retryRes = await fetch(`${BASE_URL}${path}`, {
            ...options,
            credentials: 'include',
            headers: retryHeaders,
          });
          if (!retryRes.ok) {
            const errorBody = await retryRes.text();
            reject(new ApiError(retryRes.status, errorBody));
            return;
          }
          if (retryRes.status === 204) {
            resolve(undefined as T);
            return;
          }
          resolve(retryRes.json());
        } catch (err) {
          reject(err);
        }
      });
    });
  }

  if (!res.ok) {
    const errorBody = await res.text();
    throw new ApiError(res.status, errorBody);
  }

  if (res.status === 204) {
    return undefined as T;
  }

  return res.json();
}

export class ApiError extends Error {
  status: number;
  body: string;

  constructor(status: number, body: string) {
    super(`API error ${status}: ${body}`);
    this.name = 'ApiError';
    this.status = status;
    this.body = body;
  }
}
