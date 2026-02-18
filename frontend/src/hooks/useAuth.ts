import {
  createContext,
  useContext,
  useState,
  useEffect,
  useCallback,
  type ReactNode,
} from 'react';
import { createElement } from 'react';
import type { User, AuthTokens } from '../types';
import { getMe, logout as apiLogout } from '../api/auth';

interface AuthContextValue {
  user: User | null;
  isAuthenticated: boolean;
  isAdmin: boolean;
  isLoading: boolean;
  login: (tokens: AuthTokens) => Promise<void>;
  logout: () => Promise<void>;
  setUser: (user: User | null) => void;
}

const AuthContext = createContext<AuthContextValue | undefined>(undefined);

export function AuthProvider({ children }: { children: ReactNode }) {
  const [user, setUser] = useState<User | null>(null);
  const [isLoading, setIsLoading] = useState(true);

  const isAuthenticated = user !== null;
  const isAdmin = user?.role === 'admin';

  const fetchUser = useCallback(async () => {
    const token = localStorage.getItem('sao_access_token');
    if (!token) {
      setIsLoading(false);
      return;
    }

    try {
      const me = await getMe();
      setUser(me);
    } catch {
      localStorage.removeItem('sao_access_token');
      localStorage.removeItem('sao_refresh_token');
      setUser(null);
    } finally {
      setIsLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchUser();
  }, [fetchUser]);

  const login = useCallback(async (tokens: AuthTokens) => {
    localStorage.setItem('sao_access_token', tokens.access_token);
    localStorage.setItem('sao_refresh_token', tokens.refresh_token);
    try {
      const me = await getMe();
      setUser(me);
    } catch {
      localStorage.removeItem('sao_access_token');
      localStorage.removeItem('sao_refresh_token');
      throw new Error('Failed to fetch user after login');
    }
  }, []);

  const logout = useCallback(async () => {
    const refreshToken = localStorage.getItem('sao_refresh_token');
    if (refreshToken) {
      try {
        await apiLogout(refreshToken);
      } catch {
        // Ignore logout errors
      }
    }
    localStorage.removeItem('sao_access_token');
    localStorage.removeItem('sao_refresh_token');
    setUser(null);
  }, []);

  const value: AuthContextValue = {
    user,
    isAuthenticated,
    isAdmin,
    isLoading,
    login,
    logout,
    setUser,
  };

  return createElement(AuthContext.Provider, { value }, children);
}

export function useAuth(): AuthContextValue {
  const ctx = useContext(AuthContext);
  if (ctx === undefined) {
    throw new Error('useAuth must be used within an AuthProvider');
  }
  return ctx;
}
