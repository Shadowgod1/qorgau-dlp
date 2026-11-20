import { useCallback, useState } from 'react'
import { Layout, Menu, Avatar, Dropdown, Button, Badge, Tooltip, theme as antTheme } from 'antd'
import type { MenuProps } from 'antd'
import {
  DashboardOutlined,
  LaptopOutlined,
  AlertOutlined,
  UnorderedListOutlined,
  BarChartOutlined,
  SettingOutlined,
  LogoutOutlined,
  UserOutlined,
  BulbOutlined,
  MenuFoldOutlined,
  MenuUnfoldOutlined,
  SafetyCertificateOutlined,
  ApartmentOutlined,
  FileTextOutlined,
  ClockCircleOutlined,
  VideoCameraOutlined,
} from '@ant-design/icons'
import { useNavigate, useLocation, Outlet } from 'react-router-dom'
import { useAuth } from '../context/AuthContext'
import { useTheme } from '../context/ThemeContext'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { fetchIncidentsPaged } from '../api/client'
import { useWebSocket } from '../hooks/useWebSocket'
import type { WsMessage } from '../api/types'
import { ErrorBoundary } from './ErrorBoundary'

const { Sider, Header, Content } = Layout

export default function Shell() {
  const { token: authToken, user, logout } = useAuth()
  const { theme, toggle } = useTheme()
  const navigate = useNavigate()
  const { pathname } = useLocation()
  const qc = useQueryClient()
  const [collapsed, setCollapsed] = useState(false)
  const { token } = antTheme.useToken()

  const isAdmin = user?.role === 'admin'
  const isOfficer = user?.role === 'admin' || user?.role === 'security_officer'

  const handleWsMsg = useCallback(
    (msg: WsMessage) => {
      if (msg.type === 'incident') {
        qc.invalidateQueries({ queryKey: ['incidents'] })
        qc.invalidateQueries({ queryKey: ['incidents-all'] })
        qc.invalidateQueries({ queryKey: ['incidents-open-count'] })
        qc.invalidateQueries({ queryKey: ['incidents-open-paged'] })
        qc.invalidateQueries({ queryKey: ['incident-stats'] })
      } else if (msg.type === 'screenshot') {
        qc.invalidateQueries({ queryKey: ['screenshots-recent'] })
      } else if ((msg as { type: string }).type === 'webcam') {
        qc.invalidateQueries({ queryKey: ['webcam'] })
      } else if ((msg as { type: string }).type === 'audio') {
        qc.invalidateQueries({ queryKey: ['audio'] })
      }
    },
    [qc],
  )
  const { connected: wsConnected } = useWebSocket(authToken, handleWsMsg)

  const { data: openCount = 0 } = useQuery({
    queryKey: ['incidents-open-count'],
    queryFn: () => fetchIncidentsPaged({ status: 'open', limit: 1 }),
    refetchInterval: 30_000,
    select: (data) => data.total ?? 0,
  })

  const navItems: MenuProps['items'] = [
    { key: '/dashboard', icon: <DashboardOutlined />, label: 'Дашборд' },
    { key: '/agents',    icon: <LaptopOutlined />,    label: 'Агенты' },
    { key: '/events',    icon: <UnorderedListOutlined />, label: 'События' },
    {
      key: '/incidents',
      icon: <AlertOutlined />,
      label: (
        <span style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
          Инциденты
          {openCount > 0 && (
            <Badge count={openCount} size="small" style={{ marginLeft: 8 }} />
          )}
        </span>
      ),
    },
    { key: '/analytics', icon: <BarChartOutlined />,   label: 'Аналитика' },
    { key: '/activity',  icon: <ClockCircleOutlined />, label: 'Активность' },
    ...(isAdmin ? [{ key: '/configs', icon: <ApartmentOutlined />, label: 'Конфигурации' }] : []),
    { key: '/reports',       icon: <FileTextOutlined />,     label: 'Отчёты' },
    ...(isOfficer ? [{ key: '/surveillance', icon: <VideoCameraOutlined />, label: 'Наблюдение' }] : []),
    ...(isAdmin ? [{ key: '/settings', icon: <SettingOutlined />, label: 'Настройки' }] : []),
  ]

  const userMenuItems: MenuProps['items'] = [
    { key: 'logout', icon: <LogoutOutlined />, label: 'Выйти', danger: true },
  ]

  return (
    <Layout style={{ minHeight: '100vh' }}>
      <Sider
        collapsible
        collapsed={collapsed}
        trigger={null}
        width={220}
        style={{
          background: token.colorBgContainer,
          borderRight: `1px solid ${token.colorBorderSecondary}`,
          position: 'fixed',
          height: '100vh',
          left: 0,
          top: 0,
          zIndex: 100,
          overflow: 'hidden',
        }}
      >
        <div style={{
          height: 56,
          display: 'flex',
          alignItems: 'center',
          padding: collapsed ? '0 24px' : '0 20px',
          borderBottom: `1px solid ${token.colorBorderSecondary}`,
          gap: 10,
          overflow: 'hidden',
        }}>
          <SafetyCertificateOutlined style={{ fontSize: 22, color: '#3b82f6', flexShrink: 0 }} />
          {!collapsed && (
            <span style={{ fontWeight: 700, fontSize: 16, color: token.colorText, whiteSpace: 'nowrap' }}>
              Qorgau DLP
            </span>
          )}
        </div>

        <Menu
          mode="inline"
          selectedKeys={[pathname]}
          items={navItems}
          onClick={({ key }) => navigate(key)}
          style={{ border: 'none', marginTop: 8 }}
        />
      </Sider>

      <Layout style={{ marginLeft: collapsed ? 80 : 220, transition: 'margin-left 0.2s' }}>
        <Header style={{
          background: token.colorBgContainer,
          borderBottom: `1px solid ${token.colorBorderSecondary}`,
          padding: '0 20px',
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'space-between',
          height: 56,
          position: 'sticky',
          top: 0,
          zIndex: 99,
        }}>
          <Button
            type="text"
            icon={collapsed ? <MenuUnfoldOutlined /> : <MenuFoldOutlined />}
            onClick={() => setCollapsed(v => !v)}
          />

          <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
            <Tooltip title={wsConnected ? 'Real-time WebSocket активен' : 'Подключение к WebSocket...'}>
              <Badge status={wsConnected ? 'success' : 'warning'} />
            </Tooltip>
            <Button
              type="text"
              icon={<BulbOutlined />}
              onClick={toggle}
              title={theme === 'dark' ? 'Светлая тема' : 'Тёмная тема'}
            />
            <Dropdown
              menu={{
                items: userMenuItems,
                onClick: ({ key }) => { if (key === 'logout') logout() },
              }}
              placement="bottomRight"
            >
              <div style={{
                display: 'flex', alignItems: 'center', gap: 8,
                cursor: 'pointer', padding: '4px 8px', borderRadius: 6,
              }}>
                <Avatar size={30} icon={<UserOutlined />} style={{ background: '#3b82f6' }} />
                <span style={{ fontSize: 13, color: token.colorText }}>
                  {user?.username ?? '—'}
                </span>
              </div>
            </Dropdown>
          </div>
        </Header>

        <Content style={{ padding: 24, minHeight: 'calc(100vh - 56px)' }}>
          <ErrorBoundary>
            <Outlet />
          </ErrorBoundary>
        </Content>
      </Layout>
    </Layout>
  )
}
