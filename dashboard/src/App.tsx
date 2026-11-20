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

const queryClient = new QueryClient({
  defaultOptions: {
    queries: { staleTime: 20_000, retry: 1, refetchOnWindowFocus: false },
  },
})

function ProtectedRoute({ children }: { children: React.ReactNode }) {
  const { isAuthenticated } = useAuth()
  return isAuthenticated ? <>{children}</> : <Navigate to="/login" replace />
}

function AppRoutes() {
  const { theme } = useTheme()

  return (
    <ConfigProvider
      locale={ruRU}
      theme={{
        algorithm: theme === 'dark' ? antTheme.darkAlgorithm : antTheme.defaultAlgorithm,
        token: {
          colorPrimary: '#3b82f6',
          colorBgBase: theme === 'dark' ? '#0d0f14' : '#f0f2f5',
          colorBgContainer: theme === 'dark' ? '#161b24' : '#ffffff',
          borderRadius: 6,
          fontSize: 13,
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
          <Route path="agents"     element={<AgentsPage />} />
          <Route path="events"     element={<EventsPage />} />
          <Route path="incidents"  element={<IncidentsPage />} />
          <Route path="analytics"  element={<AnalyticsPage />} />
          <Route path="activity"   element={<ActivityPage />} />
          <Route path="configs"       element={<ConfigsPage />} />
          <Route path="reports"       element={<ReportsPage />} />
          <Route path="surveillance"  element={<SurveillancePage />} />
          <Route path="settings"      element={<SettingsPage />} />
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
