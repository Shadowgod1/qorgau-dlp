import { Outlet, useNavigate, useLocation } from 'react-router-dom'
import { Layout, Menu, Badge, notification, Typography } from 'antd'
import {
  DashboardOutlined,
  LaptopOutlined,
  AlertOutlined,
  CameraOutlined,
  UnorderedListOutlined,
  SettingOutlined,
} from '@ant-design/icons'
import { QueryClient, useQueryClient } from '@tanstack/react-query'
import { useWebSocket } from '../hooks/useWebSocket'
import type { WsMessage } from '../api/types'

const { Sider, Content, Header } = Layout

const SEVERITY_COLOR: Record<string, string> = {
  low: '#faad14',
  medium: '#fa8c16',
  high: '#f5222d',
  critical: '#820014',
}

const NAV_ITEMS = [
  { key: '/dashboard',   icon: <DashboardOutlined />,      label: 'Дашборд' },
  { key: '/agents',      icon: <LaptopOutlined />,          label: 'Агенты' },
  { key: '/incidents',   icon: <AlertOutlined />,           label: 'Инциденты' },
  { key: '/screenshots', icon: <CameraOutlined />,          label: 'Скриншоты' },
  { key: '/events',      icon: <UnorderedListOutlined />,   label: 'События' },
  { key: '/rules',       icon: <SettingOutlined />,         label: 'Политики' },
]

interface Props {
  queryClient: QueryClient
}

export default function AppLayout({ queryClient }: Props) {
  const navigate = useNavigate()
  const location = useLocation()
  const qc = useQueryClient()

  useWebSocket((msg: WsMessage) => {
    if (msg.type === 'incident') {
      notification.open({
        message: `🚨 Инцидент #${msg.id}`,
        description: `${msg.hostname} / ${msg.username} — ${msg.channel} [${msg.severity}]\n${msg.snippet}`,
        style: { borderLeft: `4px solid ${SEVERITY_COLOR[msg.severity ?? 'low']}` },
        duration: 8,
      })
      qc.invalidateQueries({ queryKey: ['incidents'] })
    }
    if (msg.type === 'screenshot') {
      qc.invalidateQueries({ queryKey: ['screenshots'] })
    }
  })

  return (
    <Layout style={{ minHeight: '100vh' }}>
      <Sider theme="dark" width={210} collapsible>
        <div style={{ padding: '16px 24px', color: '#fff', fontWeight: 700, fontSize: 18 }}>
          🛡 Qorgau DLP
        </div>
        <Menu
          theme="dark"
          mode="inline"
          selectedKeys={[location.pathname]}
          items={NAV_ITEMS}
          onClick={({ key }) => navigate(key)}
        />
      </Sider>
      <Layout>
        <Header style={{ background: '#fff', padding: '0 24px', borderBottom: '1px solid #f0f0f0' }}>
          <Typography.Text type="secondary">Data Loss Prevention — Консоль управления</Typography.Text>
        </Header>
        <Content style={{ margin: 24, minHeight: 360 }}>
          <Outlet />
        </Content>
      </Layout>
    </Layout>
  )
}
