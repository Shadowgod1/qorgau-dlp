import { Card, Row, Col, Table, Tag, Typography, Progress, Spin, Empty } from 'antd'
import { useQuery } from '@tanstack/react-query'
import {
  BarChart, Bar, XAxis, YAxis, CartesianGrid, Tooltip,
  ResponsiveContainer, RadarChart, Radar, PolarGrid,
  PolarAngleAxis, PolarRadiusAxis, Legend,
} from 'recharts'
import dayjs from 'dayjs'
import http from '../api/client'

const { Title } = Typography

interface DimUser {
  username: string
  agents_count: number
  critical_incidents: number
  high_incidents: number
  medium_incidents: number
  low_incidents: number
  open_incidents: number
  risk_score: number
  last_incident_at: string | null
}

interface CubeRiskRow {
  username: string
  day: string
  risk_score: number
  incident_count: number
  channels: string[]
}

interface DimApp {
  app_name: string
  agents_using: number
  users_count: number
  total_events: number
  total_duration_sec: number | null
  avg_duration_sec: number | null
}

const riskColor = (score: number) => {
  if (score >= 100) return '#f5222d'
  if (score >= 40) return '#fa8c16'
  if (score >= 10) return '#fadb14'
  return '#52c41a'
}

export default function AnalyticsPage() {
  const { data: users = [], isLoading: usersLoading } = useQuery<DimUser[]>({
    queryKey: ['olap-users'],
    queryFn: () => http.get('/olap/dim/users').then(r => r.data),
    refetchInterval: 60_000,
  })

  const { data: riskCube = [] } = useQuery<CubeRiskRow[]>({
    queryKey: ['olap-risk-cube'],
    queryFn: () => http.get('/olap/cube/risk?days=30').then(r => r.data),
    refetchInterval: 60_000,
  })

  const { data: apps = [] } = useQuery<DimApp[]>({
    queryKey: ['olap-apps'],
    queryFn: () => http.get('/olap/dim/apps').then(r => r.data),
    refetchInterval: 120_000,
  })

  // Daily risk trend: sum per day across all users
  const dailyRisk = Object.entries(
    riskCube.reduce((acc: Record<string, number>, row) => {
      acc[row.day] = (acc[row.day] || 0) + row.risk_score
      return acc
    }, {})
  )
    .sort(([a], [b]) => a.localeCompare(b))
    .slice(-14)
    .map(([day, score]) => ({ day: dayjs(day).format('DD.MM'), score }))

  // Top 5 risky users for radar
  const top5 = users.slice(0, 5).map(u => ({
    user: u.username,
    critical: u.critical_incidents,
    high: u.high_incidents,
    medium: u.medium_incidents,
    low: u.low_incidents,
  }))

  const maxRisk = users[0]?.risk_score || 1

  const userCols = [
    { title: 'Пользователь', dataIndex: 'username', key: 'username' },
    {
      title: 'Риск', dataIndex: 'risk_score', key: 'risk',
      width: 200,
      render: (v: number) => (
        <Progress
          percent={Math.min(100, (v / maxRisk) * 100)}
          size="small"
          format={() => v}
          strokeColor={riskColor(v)}
        />
      ),
      sorter: (a: DimUser, b: DimUser) => a.risk_score - b.risk_score,
      defaultSortOrder: 'descend' as const,
    },
    {
      title: 'Критич.', dataIndex: 'critical_incidents', key: 'crit', width: 80,
      render: (v: number) => v > 0 ? <Tag color="red">{v}</Tag> : 0,
    },
    { title: 'High', dataIndex: 'high_incidents', key: 'high', width: 70 },
    { title: 'Medium', dataIndex: 'medium_incidents', key: 'med', width: 80 },
    { title: 'Low', dataIndex: 'low_incidents', key: 'low', width: 70 },
    {
      title: 'Открытых', dataIndex: 'open_incidents', key: 'open', width: 90,
      render: (v: number) => v > 0 ? <Tag color="orange">{v}</Tag> : 0,
    },
    {
      title: 'Последний', dataIndex: 'last_incident_at', key: 'last', width: 130,
      render: (v: string | null) => v ? dayjs(v).format('DD.MM.YY HH:mm') : '—',
    },
  ]

  const appCols = [
    { title: 'Приложение', dataIndex: 'app_name', key: 'app' },
    { title: 'Пользователей', dataIndex: 'users_count', key: 'users', width: 120 },
    { title: 'Агентов', dataIndex: 'agents_using', key: 'agents', width: 100 },
    { title: 'Событий', dataIndex: 'total_events', key: 'events', width: 100 },
    {
      title: 'Время (мин)', dataIndex: 'total_duration_sec', key: 'duration', width: 110,
      render: (v: number | null) => v ? Math.round(v / 60).toLocaleString() : '—',
      sorter: (a: DimApp, b: DimApp) => (a.total_duration_sec || 0) - (b.total_duration_sec || 0),
      defaultSortOrder: 'descend' as const,
    },
  ]

  return (
    <div>
      <Title level={4} style={{ marginBottom: 20 }}>Аналитика</Title>

      {/* Risk trend */}
      <Row gutter={[16, 16]} style={{ marginBottom: 16 }}>
        <Col xs={24} md={14}>
          <Card title="Риск за 30 дней (сумма по всем пользователям)">
            {dailyRisk.length > 0 ? (
              <ResponsiveContainer width="100%" height={200}>
                <BarChart data={dailyRisk} margin={{ top: 4, right: 16, left: -10, bottom: 0 }}>
                  <CartesianGrid strokeDasharray="3 3" strokeOpacity={0.3} />
                  <XAxis dataKey="day" tick={{ fontSize: 11 }} />
                  <YAxis tick={{ fontSize: 11 }} />
                  <Tooltip />
                  <Bar dataKey="score" fill="#3b82f6" name="Риск-скор" radius={[3, 3, 0, 0]} />
                </BarChart>
              </ResponsiveContainer>
            ) : (
              <Empty description="Нет данных" style={{ height: 200, paddingTop: 60 }} />
            )}
          </Card>
        </Col>
        <Col xs={24} md={10}>
          <Card title="Инциденты топ-5 пользователей (радар)">
            {top5.length > 0 ? (
              <ResponsiveContainer width="100%" height={200}>
                <RadarChart data={top5}>
                  <PolarGrid />
                  <PolarAngleAxis dataKey="user" tick={{ fontSize: 10 }} />
                  <PolarRadiusAxis tick={{ fontSize: 9 }} />
                  <Radar name="critical" dataKey="critical" stroke="#f5222d" fill="#f5222d" fillOpacity={0.3} />
                  <Radar name="high" dataKey="high" stroke="#fa8c16" fill="#fa8c16" fillOpacity={0.2} />
                  <Legend wrapperStyle={{ fontSize: 11 }} />
                </RadarChart>
              </ResponsiveContainer>
            ) : (
              <Empty description="Нет данных" style={{ height: 200, paddingTop: 60 }} />
            )}
          </Card>
        </Col>
      </Row>

      {/* User risk table */}
      <Card title="Риск-скор пользователей" style={{ marginBottom: 16 }}>
        <Spin spinning={usersLoading}>
          {users.length === 0 && !usersLoading ? (
            <Empty description="Нет инцидентов" />
          ) : (
            <Table
              dataSource={users}
              columns={userCols}
              rowKey="username"
              size="small"
              pagination={{ pageSize: 15 }}
              scroll={{ x: 700 }}
            />
          )}
        </Spin>
      </Card>

      {/* Top apps */}
      <Card title="Топ приложений по времени использования">
        <Table
          dataSource={apps.slice(0, 20)}
          columns={appCols}
          rowKey="app_name"
          size="small"
          pagination={false}
          scroll={{ x: 600 }}
        />
      </Card>
    </div>
  )
}
