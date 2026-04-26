import { BrowserRouter, Routes, Route, Navigate } from 'react-router-dom';
import { useQuery } from '@tanstack/react-query';
import { setupStatus } from './api/auth';
import { useAuth } from './hooks/useAuth';
import Layout from './components/Layout';
import ProtectedRoute from './components/ProtectedRoute';
import BootstrapRequiredPage from './pages/BootstrapRequiredPage';
import Login from './pages/Login';
import Dashboard from './pages/Dashboard';
import VaultPage from './pages/VaultPage';
import AgentsPage from './pages/AgentsPage';
import AdminUsersPage from './pages/AdminUsersPage';
import AdminSsoPage from './pages/AdminSsoPage';
import AuditLogPage from './pages/AuditLogPage';
import SkillsCatalogPage from './pages/SkillsCatalogPage';
import AdminSkillReviewPage from './pages/AdminSkillReviewPage';
import AdminLlmProvidersPage from './pages/AdminLlmProvidersPage';
import AgentEventsPage from './pages/AgentEventsPage';

function AppRoutes() {
  const { isAuthenticated, isLoading: authLoading } = useAuth();

  const { data: status, isLoading: setupLoading } = useQuery({
    queryKey: ['setup-status'],
    queryFn: setupStatus,
    staleTime: 60_000,
  });

  if (authLoading || setupLoading) {
    return (
      <div className="flex items-center justify-center min-h-screen bg-gray-900">
        <div className="text-center">
          <div className="inline-block w-8 h-8 border-4 border-blue-600 border-t-transparent rounded-full animate-spin mb-4"></div>
          <p className="text-gray-400 text-sm">Loading SAO...</p>
        </div>
      </div>
    );
  }

  if (status?.needs_setup) {
    return (
      <Routes>
        <Route
          path="*"
          element={<BootstrapRequiredPage status={status} />}
        />
      </Routes>
    );
  }

  return (
    <Routes>
      <Route path="/login" element={
        isAuthenticated ? <Navigate to="/" replace /> : <Login />
      } />
      <Route
        element={
          <ProtectedRoute>
            <Layout />
          </ProtectedRoute>
        }
      >
        <Route path="/" element={<Dashboard />} />
        <Route path="/vault" element={<VaultPage />} />
        <Route path="/agents" element={<AgentsPage />} />
        <Route path="/agents/:id/events" element={<AgentEventsPage />} />
        <Route path="/skills" element={<SkillsCatalogPage />} />
        <Route path="/audit" element={<AuditLogPage />} />
        <Route
          path="/admin/users"
          element={
            <ProtectedRoute admin>
              <AdminUsersPage />
            </ProtectedRoute>
          }
        />
        <Route
          path="/admin/sso"
          element={
            <ProtectedRoute admin>
              <AdminSsoPage />
            </ProtectedRoute>
          }
        />
        <Route
          path="/admin/skills/review"
          element={
            <ProtectedRoute admin>
              <AdminSkillReviewPage />
            </ProtectedRoute>
          }
        />
        <Route
          path="/admin/llm-providers"
          element={
            <ProtectedRoute admin>
              <AdminLlmProvidersPage />
            </ProtectedRoute>
          }
        />
      </Route>
      <Route path="*" element={<Navigate to="/" replace />} />
    </Routes>
  );
}

export default function App() {
  return (
    <BrowserRouter>
      <AppRoutes />
    </BrowserRouter>
  );
}
