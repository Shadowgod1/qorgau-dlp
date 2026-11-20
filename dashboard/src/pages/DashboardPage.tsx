import { Row, Col, Card, Statistic, Table, Tag, Typography, Spin, Image, Empty } from 'antd'
import {
  AlertOutlined, LaptopOutlined, CameraOutlined, SecurityScanOutlined,
} from '@ant-design/icons'
import { useQuery } from '@tanstack/react-query'
import {
  BarChart, Bar, XAxis, YAxis, CartesianGrid, Tooltip, ResponsiveContainer,
  PieChart, Pie, Cell, Legend,
} from 'recharts'
import dayjs from 'dayjs'
import { fetchIncidents, fetchIncidentsPaged, fetchAgents, fetchIncidentStats, fetchScreenshots, screenshotUrl } from '../api/client'
import type { Incident } from '../api/types'

const { Title } = Typography

const SEV_COLORS: Record<string, string> = {
  critical: '#f5222d',
  high: '#fa8c16',
  medium: '#fadb14',
  low: '#52c41a',
}

const STATUS_COLOR: Record<string, string> = {
  open: 'red',
  reviewed: 'orange',
  resolved: 'green',
  false_positive: 'default',
}

const RECENT_COLS = [
  { title: 'Хост', dataIndex: 'hostname', key: 'hostname', width: 130 },
  { title: 'Пользователь', dataIndex: 'username', key: 'username', width: 120 },
  {
    title: 'Серьёзность', dataIndex: 'severity', key: 'severity', width: 110,
    render: (s: string) => <Tag color={SEV_COLORS[s]}>{s}</Tag>,
  },
  { title: 'Канал', dataIndex: 'channel', key: 'channel', width: 100 },
  {
    title: 'Статус', dataIndex: 'status', key: 'status', width: 110,
    render: (s: string) => <Tag color={STATUS_COLOR[s]}>{s}</Tag>,
  },
  {
    title: 'Время', dataIndex: 'created_at', key: 'created_at',
    render: (v: string) => dayjs(v).format('DD.MM HH:mm'),
  },
]

function buildDailyData(incidents: Incident[]) {
  const map: Record<string, Record<string, number>> = {}
  incidents.forEach(inc => {
    const day = dayjs(inc.created_at!).format('DD.MM')
    if (!map[day]) map[day] = { critical: 0, high: 0, medium: 0, low: 0 }
    map[day][inc.severity] = (map[day][inc.severity] || 0) + 1
  })
  return Object.entries(map)
    .sort(([a], [b]) => a.localeCompare(b))
    .slice(-14)
    .map(([day, vals]) => ({ day, ...vals }))
}

export default function DashboardPage() {
  const { data: incidents = [], isLoading: incLoading } = useQuery({
    queryKey: ['incidents-all'],
    queryFn: () => fetchIncidents({ limit: 200 }),
    refetchInterval: 30_000,
  })

  const { data: openPaged } = useQuery({
    queryKey: ['incidents-open-paged'],
    queryFn: () => fetchIncidentsPaged({ status: 'open', limit: 1 }),
    refetchInterval: 30_000,
  })

  const { data: agents = [] } = useQuery({
    queryKey: ['agents'],
    queryFn: fetchAgents,
    refetchInterval: 30_000,
  })

  const { data: stats } = useQuery({
    queryKey: ['incident-stats'],
    queryFn: fetchIncidentStats,
    refetchInterval: 30_000,
  })

  const { data: screenshots = [] } = useQuery({
    queryKey: ['screenshots-recent'],
    queryFn: () => fetchScreenshots({ limit: 6 }),
    refetchInterval: 60_000,
  })

  const onlineAgents = agents.filter(a => a.is_online).length
  const openCount = openPaged?.total ?? incidents.filter(i => i.status === 'open').length
  const criticalCount = stats?.by_severity?.critical ?? incidents.filter(i => i.severity === 'critical').length

  const recentIncidents = [...incidents].sort(
    (a, b) => dayjs(b.created_at ?? 0).valueOf() - dayjs(a.created_at ?? 0).valueOf()
  ).slice(0, 8)

  const sevStats = stats?.by_severity ?? {}
  const pieData = [
    { name: 'critical', value: sevStats['Severity.critical'] ?? sevStats.critical ?? 0 },
    { name: 'high',     value: sevStats['Severity.high']     ?? sevStats.high     ?? 0 },
    { name: 'medium',   value: sevStats['Severity.medium']   ?? sevStats.medium   ?? 0 },
    { name: 'low',      value: sevStats['Severity.low']      ?? sevStats.low      ?? 0 },
  ].filter(d => d.value > 0)

  const dailyData = buildDailyData(incidents)

  return (
    <div>
      <Title level={4} style={{ marginBottom: 20 }}>Дашборд</Title>

      {/* KPI cards */}
      <Row gutter={[16, 16]} style={{ marginBottom: 24 }}>
        <Col xs={24} sm={12} md={6}>
          <Card>
            <Statistic
              title="Агентов онлайн"
              value={onlineAgents}
              suffix={`/ ${agents.length}`}
              prefix={<LaptopOutlined style={{ color: '#3b82f6' }} />}
              valueStyle={{ color: '#3b82f6' }}
            />
          </Card>
        </Col>
        <Col xs={24} sm={12} md={6}>
          <Card>
            <Statistic
              title="Открытых инцидентов"
              value={openCount}
              prefix={<AlertOutlined style={{ color: '#fa8c16' }} />}
              valueStyle={{ color: openCount > 0 ? '#fa8c16' : undefined }}
            />
          </Card>
        </Col>
        <Col xs={24} sm={12} md={6}>
          <Card>
            <Statistic
              title="Критических"
              value={criticalCount}
              prefix={<SecurityScanOutlined style={{ color: '#f5222d' }} />}
              valueStyle={{ color: criticalCount > 0 ? '#f5222d' : undefined }}
            />
          </Card>
        </Col>
        <Col xs={24} sm={12} md={6}>
          <Card>
            <Statistic
              title="Скриншотов"
              value={screenshots.length}
              prefix={<CameraOutlined style={{ color: '#52c41a' }} />}
            />
          </Card>
        </Col>
      </Row>

      {/* Charts row */}
      <Row gutter={[16, 16]} style={{ marginBottom: 24 }}>
        <Col xs={24} md={16}>
          <Card title="Инциденты по дням" styles={{ body: { padding: '12px 0' } }}>
            <ResponsiveContainer width="100%" height={220}>
              <BarChart data={dailyData} margin={{ top: 4, right: 16, left: -10, bottom: 0 }}>
                <CartesianGrid strokeDasharray="3 3" strokeOpacity={0.3} />
                <XAxis dataKey="day" tick={{ fontSize: 11 }} />
                <YAxis tick={{ fontSize: 11 }} />
                <Tooltip />
                <Bar dataKey="critical" stackId="a" fill="#f5222d" name="critical" />
                <Bar dataKey="high" stackId="a" fill="#fa8c16" name="high" />
                <Bar dataKey="medium" stackId="a" fill="#fadb14" name="medium" />
                <Bar dataKey="low" stackId="a" fill="#52c41a" name="low" />
              </BarChart>
            </ResponsiveContainer>
          </Card>
        </Col>
        <Col xs={24} md={8}>
          <Card title="По серьёзности" styles={{ body: { padding: '12px 0' } }}>
            {pieData.length > 0 ? (
              <ResponsiveContainer width="100%" height={220}>
                <PieChart>
                  <Pie data={pieData} cx="50%" cy="50%" innerRadius={55} outerRadius={80} dataKey="value">
                    {pieData.map(entry => (
                      <Cell key={entry.name} fill={SEV_COLORS[entry.name]} />
                    ))}
                  </Pie>
                  <Legend />
                  <Tooltip />
                </PieChart>
              </ResponsiveContainer>
            ) : (
              <Empty description="Нет данных" style={{ paddingTop: 40 }} />
            )}
          </Card>
        </Col>
      </Row>

      {/* Recent incidents */}
      <Row gutter={[16, 16]}>
        <Col xs={24} xl={14}>
          <Card title="Последние инциденты">
            <Spin spinning={incLoading}>
              <Table
                dataSource={recentIncidents}
                columns={RECENT_COLS}
                rowKey="id"
                size="small"
                pagination={false}
                scroll={{ x: 600 }}
              />
            </Spin>
          </Card>
        </Col>
        <Col xs={24} xl={10}>
          <Card title="Последние скриншоты">
            {screenshots.length === 0 ? (
              <Empty description="Нет скриншотов" />
            ) : (
              <Image.PreviewGroup>
                <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr 1fr', gap: 8 }}>
                  {screenshots.slice(0, 6).map(s => (
                    <div key={s.id}>
                      <Image
                        src={screenshotUrl(s.id)}
                        style={{ width: '100%', height: 90, objectFit: 'cover', borderRadius: 4 }}
                        fallback="data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg=="
                      />
                      <div style={{ fontSize: 10, color: '#888', marginTop: 2 }}>
                        {s.hostname} · {dayjs(s.created_at).format('HH:mm')}
                      </div>
                    </div>
                  ))}
                </div>
              </Image.PreviewGroup>
            )}
          </Card>
        </Col>
      </Row>
    </div>
  )
}
