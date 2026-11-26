import { Navigate, Route, Routes, BrowserRouter } from 'react-router-dom'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { ConfigProvider, theme as antTheme } from 'antd'
import ruRU from 'antd/locale/ru_RU'

import { ThemeProvider, useTheme } from './context/ThemeContext'
import { AuthProvider, useAuth } from './context/AuthContext'

import Shell from './components/Shell'
import Login from './pages/Login'
import DashboardPage from './pages/DashboardPage'
import AgentsPage from './pages/AgentsPage'
import EventsPage from './pages/EventsPage'
import IncidentsPage from './pages/IncidentsPage'
import AnalyticsPage from './pages/AnalyticsPage'
import ActivityPage from './pages/ActivityPage'
import SettingsPage from './pages/SettingsPage'
import ConfigsPage from './pages/ConfigsPage'
import ReportsPage from './pages/ReportsPage'
import SurveillancePage from './pages/SurveillancePage'
import QuadratorPage from './pages/QuadratorPage'
import EmployeesPage from './pages/EmployeesPage'
import AuditPage from './pages/AuditPage'
import ShadowPage from './pages/ShadowPage'
import UsbDevicesPage from './pages/UsbDevicesPage'
import DiscoveryPage from './pages/DiscoveryPage'
import InventoryPage from './pages/InventoryPage'

const queryClient = new QueryClient({
  defaultOptions: {
    queries: { staleTime: 20_000, retry: 1, refetchOnWindowFocus: false },
  },
})

function ProtectedRoute({ children }: { children: React.ReactNode }) {
  const { isAuthenticated } = useAuth()
  return isAuthenticated ? <>{children}</> : <Navigate to="/login" replace />
}

function RoleGuard({ roles, children }: { roles: string[]; children: React.ReactNode }) {
  const { user } = useAuth()
  if (!user) return null
  return roles.includes(user.role) ? <>{children}</> : <Navigate to="/dashboard" replace />
}

function AppRoutes() {
  const { theme } = useTheme()

  // Sync data-theme attribute with tasteskill CSS variables
  if (typeof document !== 'undefined') {
    document.documentElement.setAttribute('data-theme', theme)
  }

  const isDark = theme === 'dark'

  return (
    <ConfigProvider
      locale={ruRU}
      theme={{
        algorithm: isDark ? antTheme.darkAlgorithm : antTheme.defaultAlgorithm,
        token: {
          colorPrimary: isDark ? '#5ab9fc' : '#0f62fe',
          colorInfo: isDark ? '#5ab9fc' : '#0f62fe',
          colorSuccess: '#10b981',
          colorWarning: '#f59e0b',
          colorError: '#ef4444',
          colorBgBase: isDark ? '#0a0d14' : '#f4f5f7',
          colorBgContainer: isDark ? '#131824' : '#ffffff',
          colorBgElevated: isDark ? '#182030' : '#ffffff',
          colorText: isDark ? '#e9edf3' : '#0a0d14',
          colorTextSecondary: isDark ? '#8891a1' : '#54575f',
          colorBorder: isDark ? 'rgba(233, 237, 243, 0.14)' : 'rgba(10, 13, 20, 0.14)',
          colorBorderSecondary: isDark ? 'rgba(233, 237, 243, 0.10)' : 'rgba(10, 13, 20, 0.09)',
          borderRadius: 6,
          borderRadiusSM: 4,
          borderRadiusLG: 8,
          fontSize: 13.5,
          fontFamily: "'Geist', -apple-system, BlinkMacSystemFont, 'Segoe UI', system-ui, sans-serif",
        },
        components: {
          Button: {
            primaryColor: isDark ? '#0a0d14' : '#ffffff',
            borderRadius: 4,
          },
          Card: {
            headerFontSize: 14,
            paddingLG: 20,
            borderRadiusLG: 8,
          },
          Table: {
            headerBorderRadius: 0,
            cellPaddingBlockSM: 9,
          },
          Menu: {
            itemSelectedBg: isDark ? 'rgba(90, 185, 252, 0.11)' : 'rgba(15, 98, 254, 0.08)',
            itemSelectedColor: isDark ? '#e9edf3' : '#0a0d14',
            itemHoverBg: isDark ? 'rgba(233, 237, 243, 0.05)' : 'rgba(10, 13, 20, 0.04)',
          },
          Modal: {
            borderRadiusLG: 8,
          },
          Tag: {
            borderRadiusSM: 4,
          },
        },
      }}
    >
      <Routes>
        <Route path="/login" element={<Login />} />
        <Route
          path="/"
          element={
            <ProtectedRoute>
              <Shell />
            </ProtectedRoute>
          }
        >
          <Route index element={<Navigate to="/dashboard" replace />} />
          <Route path="dashboard"  element={<DashboardPage />} />
          <Route path="employees"  element={<EmployeesPage />} />
          <Route path="agents"     element={<AgentsPage />} />
          <Route path="quadrator"  element={<RoleGuard roles={['admin', 'security_officer']}><QuadratorPage /></RoleGuard>} />
          <Route path="events"     element={<EventsPage />} />
          <Route path="incidents"  element={<IncidentsPage />} />
          <Route path="analytics"  element={<AnalyticsPage />} />
          <Route path="activity"   element={<ActivityPage />} />
          <Route path="inventory"  element={<InventoryPage />} />
          <Route path="usb-devices" element={<UsbDevicesPage />} />
          <Route path="discovery"  element={<DiscoveryPage />} />
          <Route path="shadow"     element={<RoleGuard roles={['admin', 'security_officer']}><ShadowPage /></RoleGuard>} />
          <Route path="audit"      element={<RoleGuard roles={['admin', 'security_officer']}><AuditPage /></RoleGuard>} />
          <Route path="configs"       element={<RoleGuard roles={['admin']}><ConfigsPage /></RoleGuard>} />
          <Route path="reports"       element={<ReportsPage />} />
          <Route path="surveillance"  element={<RoleGuard roles={['admin', 'security_officer']}><SurveillancePage /></RoleGuard>} />
          <Route path="settings"      element={<RoleGuard roles={['admin']}><SettingsPage /></RoleGuard>} />
        </Route>
        <Route path="*" element={<Navigate to="/dashboard" replace />} />
      </Routes>
    </ConfigProvider>
  )
}

export default function App() {
  return (
    <QueryClientProvider client={queryClient}>
      <BrowserRouter>
        <ThemeProvider>
          <AuthProvider>
            <AppRoutes />
          </AuthProvider>
        </ThemeProvider>
      </BrowserRouter>
    </QueryClientProvider>
  )
}
