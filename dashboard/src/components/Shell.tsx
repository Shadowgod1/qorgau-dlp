import { useCallback, useEffect, useMemo, useState } from 'react'
import {
  Layout, Menu, Avatar, Dropdown, Button, Badge, Tooltip, notification,
  Modal, Input, Tag, Space, Typography, Empty, theme as antTheme, message,
} from 'antd'
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
  ApartmentOutlined,
  FileTextOutlined,
  ClockCircleOutlined,
  VideoCameraOutlined,
  AppstoreOutlined,
  TeamOutlined,
  AuditOutlined,
  FileProtectOutlined,
  UsbOutlined,
  FileSearchOutlined,
  DesktopOutlined,
  SearchOutlined,
  DownloadOutlined,
  RightOutlined,
} from '@ant-design/icons'
import { useNavigate, useLocation, Outlet } from 'react-router-dom'
import { useAuth } from '../context/AuthContext'
import { useTheme } from '../context/ThemeContext'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { fetchIncidentsPaged, fetchAgents, downloadWindowsAgent } from '../api/client'
import { useWebSocket } from '../hooks/useWebSocket'
import type { WsMessage } from '../api/types'
import { ErrorBoundary } from './ErrorBoundary'

const { Sider, Header, Content } = Layout
const { Text } = Typography

const PAGE_TITLES: Record<string, { title: string; group: string }> = {
  '/dashboard':    { title: 'Оперативный обзор', group: 'Обзор' },
  '/employees':    { title: 'Сотрудники и риск', group: 'Обзор' },
  '/agents':       { title: 'Агенты и рабочие станции', group: 'Обзор' },
  '/quadrator':    { title: 'Квадратор Live', group: 'Обзор' },
  '/incidents':    { title: 'Инциденты DLP', group: 'Безопасность' },
  '/events':       { title: 'Лента событий', group: 'Безопасность' },
  '/shadow':       { title: 'Теневые копии', group: 'Безопасность' },
  '/discovery':    { title: 'Краулер ПДн', group: 'Безопасность' },
  '/usb-devices':  { title: 'Устройства (ВНИ)', group: 'Контроль и защита' },
  '/inventory':    { title: 'Инвентаризация ПО и железа', group: 'Контроль и защита' },
  '/activity':     { title: 'Активность и УРВ', group: 'Контроль и защита' },
  '/surveillance': { title: 'Наблюдение', group: 'Контроль и защита' },
  '/analytics':    { title: 'Аналитика', group: 'Аналитика и управление' },
  '/reports':      { title: 'Отчёты и комплаенс РК', group: 'Аналитика и управление' },
  '/configs':      { title: 'Конфигурации агентов', group: 'Аналитика и управление' },
  '/audit':        { title: 'Журнал аудита', group: 'Аналитика и управление' },
  '/settings':     { title: 'Настройки и политики', group: 'Аналитика и управление' },
}

const ROLE_META: Record<string, { label: string }> = {
  admin:            { label: 'Администратор' },
  security_officer: { label: 'Офицер ИБ' },
  viewer:           { label: 'Наблюдатель' },
}

export default function Shell() {
  const { token: authToken, user, logout } = useAuth()
  const { theme, toggle } = useTheme()
  const navigate = useNavigate()
  const { pathname } = useLocation()
  const qc = useQueryClient()
  const [collapsed, setCollapsed] = useState(false)
  const [cmdOpen, setCmdOpen] = useState(false)
  const [cmdQuery, setCmdQuery] = useState('')
  const { token } = antTheme.useToken()

  const isAdmin = user?.role === 'admin'
  const isOfficer = user?.role === 'admin' || user?.role === 'security_officer'

  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'k') {
        e.preventDefault()
        setCmdOpen(v => !v)
      }
    }
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [])

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
      } else if ((msg as { type: string }).type === 'risk_alert') {
        const ra = msg as unknown as { username: string; risk_score: number; threshold: number }
        notification.warning({
          message: `Превышен порог риска: ${ra.username}`,
          description: `Уровень риска достиг ${ra.risk_score}/100 (порог: ${ra.threshold})`,
          placement: 'topRight',
        })
        qc.invalidateQueries({ queryKey: ['employees'] })
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

  const { data: agents = [] } = useQuery({
    queryKey: ['agents'],
    queryFn: fetchAgents,
    refetchInterval: 30_000,
  })
  const onlineAgents = agents.filter(a => a.is_online).length

  const navItems: MenuProps['items'] = useMemo(() => {
    const overviewChildren: MenuProps['items'] = [
      { key: '/dashboard', icon: <DashboardOutlined />, label: 'Дашборд' },
      { key: '/employees', icon: <TeamOutlined />,      label: 'Сотрудники' },
      { key: '/agents',    icon: <LaptopOutlined />,    label: 'Агенты' },
      ...(isOfficer ? [{ key: '/quadrator', icon: <AppstoreOutlined />, label: 'Квадратор Live' }] : []),
    ]

    const securityChildren: MenuProps['items'] = [
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
      { key: '/events',    icon: <UnorderedListOutlined />, label: 'Лента событий' },
      ...(isOfficer ? [{ key: '/shadow', icon: <FileProtectOutlined />, label: 'Теневые копии' }] : []),
      { key: '/discovery', icon: <FileSearchOutlined />,  label: 'Краулер ПДн' },
    ]

    const controlChildren: MenuProps['items'] = [
      { key: '/usb-devices', icon: <UsbOutlined />,       label: 'Устройства (ВНИ)' },
      { key: '/inventory',   icon: <DesktopOutlined />,   label: 'Инвентаризация' },
      { key: '/activity',    icon: <ClockCircleOutlined />, label: 'Активность / УРВ' },
      ...(isOfficer ? [{ key: '/surveillance', icon: <VideoCameraOutlined />, label: 'Наблюдение' }] : []),
    ]

    const adminChildren: MenuProps['items'] = [
      { key: '/analytics', icon: <BarChartOutlined />,   label: 'Аналитика' },
      { key: '/reports',   icon: <FileTextOutlined />,   label: 'Отчёты' },
      ...(isAdmin ? [{ key: '/configs', icon: <ApartmentOutlined />, label: 'Конфигурации' }] : []),
      ...(isOfficer ? [{ key: '/audit', icon: <AuditOutlined />, label: 'Журнал аудита' }] : []),
      ...(isAdmin ? [{ key: '/settings', icon: <SettingOutlined />, label: 'Настройки' }] : []),
    ]

    if (collapsed) {
      return [
        ...overviewChildren,
        { type: 'divider' },
        ...securityChildren,
        { type: 'divider' },
        ...controlChildren,
        { type: 'divider' },
        ...adminChildren,
      ]
    }

    return [
      { type: 'group', label: 'Обзор', children: overviewChildren },
      { type: 'group', label: 'Безопасность', children: securityChildren },
      { type: 'group', label: 'Контроль и защита', children: controlChildren },
      { type: 'group', label: 'Аналитика и управление', children: adminChildren },
    ]
  }, [collapsed, isAdmin, isOfficer, openCount])

  const cmdItems = useMemo(() => {
    const q = cmdQuery.trim().toLowerCase()
    const pages = Object.entries(PAGE_TITLES)
      .filter(([path]) => {
        if (['/configs', '/settings'].includes(path) && !isAdmin) return false
        if (['/quadrator', '/shadow', '/surveillance', '/audit'].includes(path) && !isOfficer) return false
        return true
      })
      .map(([path, meta]) => ({
        key: `page:${path}`,
        kind: 'Раздел',
        title: meta.title,
        subtitle: meta.group,
        onSelect: () => { navigate(path); setCmdOpen(false); setCmdQuery('') },
      }))

    const agentItems = agents.map(a => ({
      key: `agent:${a.agent_id}`,
      kind: a.is_online ? 'Агент Online' : 'Агент Offline',
      title: `${a.hostname} (${a.username || '—'})`,
      subtitle: a.ip_address || a.os_version || a.agent_id,
      onSelect: () => { navigate('/agents'); setCmdOpen(false); setCmdQuery('') },
    }))

    const quickActions = [
      {
        key: 'action:download-agent',
        kind: 'Действие',
        title: 'Скачать Windows-агент (qorgau-agent.exe)',
        subtitle: 'Нативный бинарник Rust v2.0',
        onSelect: () => {
          setCmdOpen(false)
          downloadWindowsAgent().catch(() => message.error('Файл агента не найден на сервере'))
        },
      },
      ...(isAdmin ? [{
        key: 'action:configs',
        kind: 'Действие',
        title: 'Открыть библиотеку стандартных конфигураций',
        subtitle: 'Профили для отделов и назначение на ПК',
        onSelect: () => { navigate('/configs'); setCmdOpen(false); setCmdQuery('') },
      }] : []),
    ]

    const all = [...pages, ...quickActions, ...agentItems]
    if (!q) return all.slice(0, 14)
    return all.filter(item =>
      item.title.toLowerCase().includes(q) ||
      item.subtitle.toLowerCase().includes(q) ||
      item.kind.toLowerCase().includes(q)
    ).slice(0, 14)
  }, [cmdQuery, agents, isAdmin, isOfficer, navigate])

  const pageMeta = PAGE_TITLES[pathname] ?? { title: 'Qorgau DLP', group: 'Консоль' }
  const roleMeta = ROLE_META[user?.role ?? 'viewer'] ?? ROLE_META.viewer

  const userMenuItems: MenuProps['items'] = [
    {
      key: 'role',
      disabled: true,
      label: (
        <div>
          <div style={{ fontWeight: 500 }}>{user?.username}</div>
          <div style={{ fontSize: 12, opacity: 0.7 }}>{roleMeta.label}</div>
        </div>
      ),
    },
    { type: 'divider' },
    { key: 'logout', icon: <LogoutOutlined />, label: 'Выйти', danger: true },
  ]

  return (
    <Layout style={{ minHeight: '100vh', background: 'var(--p-bg)' }}>
      <Sider
        collapsible
        collapsed={collapsed}
        trigger={null}
        width={236}
        className="qorgau-no-print"
        style={{
          background: 'var(--p-bg)',
          borderRight: '1px solid var(--p-hairline)',
          position: 'fixed',
          height: '100vh',
          left: 0,
          top: 0,
          zIndex: 100,
        }}
      >
        <div
          onClick={() => navigate('/dashboard')}
          style={{
            height: 56,
            display: 'flex',
            alignItems: 'center',
            padding: collapsed ? '0 26px' : '0 18px',
            borderBottom: '1px solid var(--p-hairline)',
            gap: 10,
            cursor: 'pointer',
            overflow: 'hidden',
          }}
        >
          <svg width="22" height="22" viewBox="0 0 32 32" style={{ flexShrink: 0 }} aria-hidden="true">
            <path
              fill="none"
              stroke="var(--p-accent)"
              strokeWidth="2.4"
              strokeLinejoin="round"
              d="M16 4L6 8v8c0 6 4.2 11.2 10 12.6C21.8 27.2 26 22 26 16V8L16 4z"
            />
            <path
              fill="none"
              stroke="var(--p-ink)"
              strokeWidth="2.4"
              strokeLinecap="round"
              strokeLinejoin="round"
              d="M11.5 16.2l3 3 6-6.4"
            />
          </svg>
          {!collapsed && (
            <div style={{ display: 'flex', alignItems: 'baseline', gap: 6, overflow: 'hidden' }}>
              <span style={{ fontWeight: 500, fontSize: 15, letterSpacing: '-0.02em', color: 'var(--p-ink)', whiteSpace: 'nowrap' }}>
                Qorgau DLP
              </span>
              <span className="qorgau-mono" style={{ fontSize: 11, color: 'var(--p-muted)' }}>
                2.0
              </span>
            </div>
          )}
        </div>

        <div className="ts-sider-scroll">
          <Menu
            mode="inline"
            selectedKeys={[pathname]}
            items={navItems}
            onClick={({ key }) => navigate(key)}
            style={{ border: 'none', background: 'transparent' }}
          />
        </div>
      </Sider>

      <Layout style={{ marginLeft: collapsed ? 80 : 236, transition: 'margin-left 180ms cubic-bezier(.2,.8,.2,1)', background: 'var(--p-bg)' }}>
        <Header
          className="qorgau-no-print"
          style={{
            background: 'var(--p-bg)',
            borderBottom: '1px solid var(--p-hairline)',
            padding: '0 24px',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'space-between',
            height: 56,
            position: 'sticky',
            top: 0,
            zIndex: 99,
            gap: 12,
          }}
        >
          <div style={{ display: 'flex', alignItems: 'center', gap: 10, minWidth: 0 }}>
            <Button
              type="text"
              aria-label={collapsed ? 'Развернуть меню' : 'Свернуть меню'}
              icon={collapsed ? <MenuUnfoldOutlined /> : <MenuFoldOutlined />}
              onClick={() => setCollapsed(v => !v)}
            />
            <div style={{ display: 'flex', alignItems: 'center', gap: 8, minWidth: 0, fontSize: 13.5 }}>
              <span style={{ color: 'var(--p-muted)', whiteSpace: 'nowrap' }}>{pageMeta.group}</span>
              <span style={{ color: 'var(--p-hairline-strong)' }}>/</span>
              <span
                style={{
                  fontWeight: 500,
                  color: 'var(--p-ink)',
                  whiteSpace: 'nowrap',
                  overflow: 'hidden',
                  textOverflow: 'ellipsis',
                }}
              >
                {pageMeta.title}
              </span>
            </div>
          </div>

          <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
            <button
              type="button"
              className="ts-cmd-trigger"
              onClick={() => setCmdOpen(true)}
              aria-label="Открыть быстрый поиск"
            >
              <SearchOutlined />
              <span>Поиск</span>
              <span className="ts-kbd">Ctrl K</span>
            </button>

            <Tooltip title={wsConnected ? 'WebSocket подключён · активные агенты' : 'Ожидание WebSocket'}>
              <div
                onClick={() => navigate('/agents')}
                style={{
                  display: 'inline-flex',
                  alignItems: 'center',
                  gap: 7,
                  height: 32,
                  padding: '0 10px',
                  borderRadius: 4,
                  fontSize: 13,
                  cursor: 'pointer',
                  border: '1px solid var(--p-hairline)',
                  background: 'var(--p-surface)',
                }}
              >
                <span
                  className={wsConnected && onlineAgents > 0 ? 'ts-dot ts-dot--live' : 'ts-dot'}
                  style={!(wsConnected && onlineAgents > 0) ? { background: wsConnected ? '#10b981' : '#f59e0b' } : undefined}
                />
                <span className="qorgau-mono" style={{ fontWeight: 500, color: 'var(--p-ink)' }}>
                  {onlineAgents}/{agents.length}
                </span>
                <span style={{ color: 'var(--p-muted)', fontSize: 12 }}>ПК</span>
              </div>
            </Tooltip>

            <Tooltip title="Скачать Windows-агент (qorgau-agent.exe)">
              <Button
                type="text"
                aria-label="Скачать Windows-агент"
                icon={<DownloadOutlined />}
                onClick={() =>
                  downloadWindowsAgent().catch(() => message.error('Файл агента не найден на сервере'))
                }
              />
            </Tooltip>

            <Tooltip title={theme === 'dark' ? 'Светлая тема' : 'Тёмная тема'}>
              <Button type="text" aria-label="Переключить тему" icon={<BulbOutlined />} onClick={toggle} />
            </Tooltip>

            <Dropdown
              menu={{
                items: userMenuItems,
                onClick: ({ key }) => { if (key === 'logout') logout() },
              }}
              placement="bottomRight"
            >
              <div
                style={{
                  display: 'flex',
                  alignItems: 'center',
                  gap: 8,
                  cursor: 'pointer',
                  height: 32,
                  padding: '0 10px 0 6px',
                  borderRadius: 4,
                  border: '1px solid var(--p-hairline)',
                  background: 'var(--p-surface)',
                }}
              >
                <Avatar
                  size={22}
                  icon={<UserOutlined />}
                  style={{ background: 'var(--p-elevated)', color: 'var(--p-ink)', border: '1px solid var(--p-hairline)' }}
                />
                <span style={{ fontSize: 13, fontWeight: 500, color: 'var(--p-ink)' }}>
                  {user?.username ?? '—'}
                </span>
              </div>
            </Dropdown>
          </div>
        </Header>

        <Content style={{ padding: '24px 28px 32px', minHeight: 'calc(100vh - 56px)', maxWidth: 1520, width: '100%', margin: '0 auto' }}>
          <ErrorBoundary>
            <Outlet />
          </ErrorBoundary>
        </Content>
      </Layout>

      {/* Command Palette Modal (Ctrl+K) */}
      <Modal
        open={cmdOpen}
        onCancel={() => { setCmdOpen(false); setCmdQuery('') }}
        footer={null}
        closable={false}
        width={540}
        styles={{ body: { padding: 14 } }}
      >
        <Input
          autoFocus
          size="large"
          prefix={<SearchOutlined style={{ color: 'var(--p-muted)' }} />}
          placeholder="Перейти к разделу, хосту или действию..."
          value={cmdQuery}
          onChange={e => setCmdQuery(e.target.value)}
          allowClear
          style={{ marginBottom: 12 }}
        />
        <div style={{ maxHeight: 360, overflowY: 'auto', display: 'flex', flexDirection: 'column', gap: 4 }}>
          {cmdItems.length === 0 ? (
            <Empty description="Ничего не найдено" style={{ padding: '24px 0' }} />
          ) : (
            cmdItems.map(item => (
              <div
                key={item.key}
                onClick={item.onSelect}
                style={{
                  display: 'flex',
                  alignItems: 'center',
                  justifyContent: 'space-between',
                  padding: '9px 12px',
                  borderRadius: 4,
                  cursor: 'pointer',
                  border: `1px solid ${token.colorBorderSecondary}`,
                }}
              >
                <Space direction="vertical" size={0}>
                  <Space size={8}>
                    <Tag bordered={false} style={{ margin: 0, fontSize: 11.5 }}>{item.kind}</Tag>
                    <Text strong style={{ fontSize: 13.5 }}>{item.title}</Text>
                  </Space>
                  <Text type="secondary" style={{ fontSize: 12 }}>{item.subtitle}</Text>
                </Space>
                <RightOutlined style={{ opacity: 0.4, fontSize: 11 }} />
              </div>
            ))
          )}
        </div>
      </Modal>
    </Layout>
  )
}
