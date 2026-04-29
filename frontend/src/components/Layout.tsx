import { NavLink, Outlet, useNavigate } from 'react-router-dom';
import { useAuth } from '../hooks/useAuth';

function SidebarLink({
  to,
  label,
  icon,
}: {
  to: string;
  label: string;
  icon: string;
}) {
  return (
    <NavLink
      to={to}
      className={({ isActive }) =>
        `flex items-center gap-3 px-4 py-2.5 rounded-lg text-sm font-medium transition-colors ${
          isActive
            ? 'bg-blue-600 text-white'
            : 'text-gray-300 hover:bg-gray-700 hover:text-white'
        }`
      }
    >
      <span className="text-lg">{icon}</span>
      <span>{label}</span>
    </NavLink>
  );
}

export default function Layout() {
  const { user, isAdmin, logout } = useAuth();
  const navigate = useNavigate();

  const handleLogout = async () => {
    await logout();
    navigate('/login');
  };

  return (
    <div className="flex min-h-screen bg-gray-900">
      {/* Sidebar */}
      <aside className="w-64 bg-gray-800 border-r border-gray-700 flex flex-col">
        {/* Logo */}
        <div className="px-6 py-5 border-b border-gray-700">
          <h1 className="text-xl font-bold text-white tracking-wide">
            SAO
          </h1>
          <p className="text-xs text-gray-400 mt-0.5">
            Secure Agent Orchestrator
          </p>
        </div>

        {/* Navigation */}
        <nav className="flex-1 px-3 py-4 space-y-1">
          <SidebarLink to="/" label="Dashboard" icon="[D]" />
          <SidebarLink to="/vault" label="Key Vault" icon="[K]" />
          <SidebarLink to="/agents" label="Agents" icon="[A]" />
          <SidebarLink to="/skills" label="Skills" icon="[T]" />
          <SidebarLink to="/audit" label="Audit Log" icon="[L]" />

          {isAdmin && (
            <>
              <div className="pt-4 pb-2 px-4">
                <p className="text-xs font-semibold text-gray-500 uppercase tracking-wider">
                  Admin
                </p>
              </div>
              <SidebarLink to="/admin/users" label="Users" icon="[U]" />
              <SidebarLink to="/admin/sso" label="SSO" icon="[S]" />
              <SidebarLink to="/admin/skills/review" label="Skill Reviews" icon="[R]" />
              <SidebarLink to="/admin/llm-providers" label="LLM Providers" icon="[M]" />
              <SidebarLink to="/admin/installer-sources" label="Installer Sources" icon="[I]" />
              <SidebarLink to="/admin/entity-archives" label="Entity Archives" icon="[X]" />
            </>
          )}
        </nav>

        {/* User info */}
        <div className="px-4 py-4 border-t border-gray-700">
          <div className="flex items-center justify-between">
            <div className="min-w-0">
              <p className="text-sm font-medium text-white truncate">
                {user?.display_name || user?.username}
              </p>
              <p className="text-xs text-gray-400 truncate">
                {user?.role === 'admin' ? 'Administrator' : 'User'}
              </p>
            </div>
            <button
              onClick={handleLogout}
              className="text-xs text-gray-400 hover:text-white px-2 py-1 rounded hover:bg-gray-700 transition-colors"
            >
              Logout
            </button>
          </div>
        </div>
      </aside>

      {/* Main content */}
      <main className="flex-1 overflow-auto">
        <div className="p-8">
          <Outlet />
        </div>
      </main>
    </div>
  );
}
