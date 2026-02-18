const BASE_URL = import.meta.env.VITE_API_BASE_URL || '';

let isRefreshing = false;
let refreshSubscribers: Array<(token: string) => void> = [];

function onRefreshed(newToken: string) {
  refreshSubscribers.forEach((cb) => cb(newToken));
  refreshSubscribers = [];
}

function addRefreshSubscriber(cb: (token: string) => void) {
  refreshSubscribers.push(cb);
}

async function attemptTokenRefresh(): Promise<string | null> {
  const refreshToken = localStorage.getItem('sao_refresh_token');
  if (!refreshToken) return null;

  try {
    const res = await fetch(`${BASE_URL}/api/auth/refresh`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ refresh_token: refreshToken }),
    });

    if (!res.ok) return null;

    const data = await res.json();
    localStorage.setItem('sao_access_token', data.access_token);
    if (data.refresh_token) {
      localStorage.setItem('sao_refresh_token', data.refresh_token);
    }
    return data.access_token;
  } catch {
    return null;
  }
}

export async function apiRequest<T>(
  path: string,
  options: RequestInit = {},
): Promise<T> {
  const token = localStorage.getItem('sao_access_token');

  const headers: Record<string, string> = {
    'Content-Type': 'application/json',
    ...(options.headers as Record<string, string>),
  };

  if (token) {
    headers['Authorization'] = `Bearer ${token}`;
  }

  const res = await fetch(`${BASE_URL}${path}`, {
    ...options,
    headers,
  });

  if (res.status === 401 && token) {
    if (!isRefreshing) {
      isRefreshing = true;
      const newToken = await attemptTokenRefresh();
      isRefreshing = false;

      if (newToken) {
        onRefreshed(newToken);
        headers['Authorization'] = `Bearer ${newToken}`;
        const retryRes = await fetch(`${BASE_URL}${path}`, {
          ...options,
          headers,
        });
        if (!retryRes.ok) {
          const errorBody = await retryRes.text();
          throw new ApiError(retryRes.status, errorBody);
        }
        return retryRes.json();
      } else {
        localStorage.removeItem('sao_access_token');
        localStorage.removeItem('sao_refresh_token');
        window.location.href = '/login';
        throw new ApiError(401, 'Session expired');
      }
    } else {
      return new Promise<T>((resolve, reject) => {
        addRefreshSubscriber(async (newToken: string) => {
          try {
            headers['Authorization'] = `Bearer ${newToken}`;
            const retryRes = await fetch(`${BASE_URL}${path}`, {
              ...options,
              headers,
            });
            if (!retryRes.ok) {
              const errorBody = await retryRes.text();
              reject(new ApiError(retryRes.status, errorBody));
              return;
            }
            resolve(retryRes.json());
          } catch (err) {
            reject(err);
          }
        });
      });
    }
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
