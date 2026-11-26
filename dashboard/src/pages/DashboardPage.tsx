import { Row, Col, Card, Table, Tag, Typography, Spin, Image, Empty, Button, Space, Progress, Tooltip, theme as antTheme, message } from 'antd'
import {
  AppstoreOutlined, FileTextOutlined, ApartmentOutlined, DownloadOutlined,
  ArrowRightOutlined, LaptopOutlined, AlertOutlined, SecurityScanOutlined, CameraOutlined,
} from '@ant-design/icons'
import { useQuery } from '@tanstack/react-query'
import { useNavigate } from 'react-router-dom'
import {
  BarChart, Bar, XAxis, YAxis, CartesianGrid, Tooltip as ReTooltip, ResponsiveContainer,
  PieChart, Pie, Cell, Legend,
} from 'recharts'
import dayjs from 'dayjs'
import {
  fetchIncidents, fetchIncidentsPaged, fetchAgents, fetchIncidentStats,
  fetchScreenshotsPaged, fetchEmployees, downloadWindowsAgent, screenshotPath,
} from '../api/client'
import type { Incident } from '../api/types'
import { useAuth } from '../context/AuthContext'
import { useAuthMediaUrl } from '../hooks/useAuthMedia'

function AuthScreenshotThumb({ id }: { id: number }) {
  const src = useAuthMediaUrl(screenshotPath(id))
  if (!src) {
    return <div style={{ width: '100%', height: 92, borderRadius: 4, background: 'var(--p-bg)', border: '1px solid var(--p-hairline)' }} />
  }
  return <Image src={src} style={{ width: '100%', height: 92, objectFit: 'cover', borderRadius: 4, border: '1px solid var(--p-hairline)' }} />
}

const { Title, Text } = Typography

const SEV_COLORS: Record<string, string> = {
  critical: '#ef4444',
  high: '#f59e0b',
  medium: '#5ab9fc',
  low: '#64748b',
}

const SEV_LABELS: Record<string, string> = {
  critical: 'Критический',
  high: 'Высокий',
  medium: 'Средний',
  low: 'Низкий',
}

const STATUS_META: Record<string, { color: string; label: string }> = {
  open: { color: 'red', label: 'Открыт' },
  reviewed: { color: 'gold', label: 'В работе' },
  resolved: { color: 'green', label: 'Закрыт' },
  false_positive: { color: 'default', label: 'Ложный' },
}

const CHANNEL_LABELS: Record<string, string> = {
  messenger: 'Мессенджер',
  clipboard: 'Буфер',
  usb: 'USB / ВНИ',
  web: 'Веб',
  app: 'ПО',
  print: 'Печать',
  network: 'Сеть',
  file: 'Файлы',
  ocr: 'OCR экрана',
}

function buildDailyData(incidents: Incident[]) {
  const map: Record<string, { label: string; critical: number; high: number; medium: number; low: number }> = {}
  incidents.forEach(inc => {
    if (!inc.created_at) return
    const key = dayjs(inc.created_at).format('YYYY-MM-DD')
    const label = dayjs(inc.created_at).format('DD.MM')
    if (!map[key]) map[key] = { label, critical: 0, high: 0, medium: 0, low: 0 }
    if (inc.severity in map[key]) {
      map[key][inc.severity as 'critical' | 'high' | 'medium' | 'low'] += 1
    }
  })
  return Object.entries(map)
    .sort(([a], [b]) => a.localeCompare(b))
    .slice(-14)
    .map(([, vals]) => ({ day: vals.label, critical: vals.critical, high: vals.high, medium: vals.medium, low: vals.low }))
}

export default function DashboardPage() {
  const { user } = useAuth()
  const navigate = useNavigate()
  const { token } = antTheme.useToken()
  const isOfficer = user?.role === 'admin' || user?.role === 'security_officer'
  const isAdmin = user?.role === 'admin'

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

  const { data: employees = [] } = useQuery({
    queryKey: ['employees-top-risk'],
    queryFn: () => fetchEmployees(30),
    refetchInterval: 60_000,
  })

  const { data: stats } = useQuery({
    queryKey: ['incident-stats'],
    queryFn: fetchIncidentStats,
    refetchInterval: 30_000,
  })

  const { data: screenshotsPaged } = useQuery({
    queryKey: ['screenshots-recent'],
    queryFn: () => fetchScreenshotsPaged({ limit: 6 }),
    enabled: isOfficer,
    refetchInterval: 60_000,
  })
  const screenshots = screenshotsPaged?.items ?? []
  const totalScreenshots = screenshotsPaged?.total ?? 0

  const onlineAgents = agents.filter(a => a.is_online).length
  const onlinePct = agents.length > 0 ? Math.round((onlineAgents / agents.length) * 100) : 0
  const openCount = openPaged?.total ?? incidents.filter(i => i.status === 'open').length
  const criticalCount = stats?.by_severity?.critical ?? incidents.filter(i => i.severity === 'critical').length
  const highRiskEmployees = [...employees].sort((a, b) => b.risk_score - a.risk_score).slice(0, 6)

  const recentIncidents = [...incidents]
    .sort((a, b) => dayjs(b.created_at ?? 0).valueOf() - dayjs(a.created_at ?? 0).valueOf())
    .slice(0, 7)

  const sevStats = stats?.by_severity ?? {}
  const pieData = [
    { name: 'critical', label: 'Критические', value: sevStats['Severity.critical'] ?? sevStats.critical ?? 0 },
    { name: 'high', label: 'Высокие', value: sevStats['Severity.high'] ?? sevStats.high ?? 0 },
    { name: 'medium', label: 'Средние', value: sevStats['Severity.medium'] ?? sevStats.medium ?? 0 },
    { name: 'low', label: 'Низкие', value: sevStats['Severity.low'] ?? sevStats.low ?? 0 },
  ].filter(d => d.value > 0)

  const dailyData = buildDailyData(incidents)

  const recentCols = [
    {
      title: 'Сотрудник / хост',
      key: 'actor',
      width: 175,
      render: (_: unknown, r: Incident) => (
        <div>
          <div style={{ fontWeight: 500, fontSize: 13 }}>{r.username || '—'}</div>
          <div className="qorgau-mono" style={{ fontSize: 11.5, color: 'var(--p-muted)' }}>{r.hostname}</div>
        </div>
      ),
    },
    {
      title: 'Уровень',
      dataIndex: 'severity',
      key: 'severity',
      width: 120,
      render: (s: string) => (
        <Space size={6}>
          <span className="ts-dot" style={{ background: SEV_COLORS[s] ?? '#64748b' }} />
          <span style={{ fontSize: 13 }}>{SEV_LABELS[s] ?? s}</span>
        </Space>
      ),
    },
    {
      title: 'Канал',
      dataIndex: 'channel',
      key: 'channel',
      width: 110,
      render: (c: string) => <Tag bordered={false}>{CHANNEL_LABELS[c] ?? c}</Tag>,
    },
    {
      title: 'Статус',
      dataIndex: 'status',
      key: 'status',
      width: 100,
      render: (s: string) => {
        const m = STATUS_META[s] ?? { color: 'default', label: s }
        return <Tag color={m.color}>{m.label}</Tag>
      },
    },
    {
      title: 'Время',
      dataIndex: 'created_at',
      key: 'created_at',
      width: 105,
      render: (v: string) => <span className="qorgau-mono" style={{ fontSize: 12, color: 'var(--p-muted)' }}>{dayjs(v).format('DD.MM HH:mm')}</span>,
    },
  ]

  return (
    <div className="ts-enter">
      {/* Clean engineering page header — one primary action */}
      <div className="ts-page-head">
        <div>
          <Title level={2} className="ts-page-title">Оперативный обзор</Title>
          <span className="ts-page-sub">
            {onlineAgents} из {agents.length} агентов в сети · {openCount} открытых инцидентов · мониторинг каналов утечек
          </span>
        </div>

        <Space wrap size={8}>
          {isOfficer && (
            <Button type="primary" icon={<AppstoreOutlined />} onClick={() => navigate('/quadrator')}>
              Открыть Квадратор
            </Button>
          )}
          <Button icon={<FileTextOutlined />} onClick={() => navigate('/reports')}>
            Отчёты
          </Button>
          {isAdmin && (
            <Button icon={<ApartmentOutlined />} onClick={() => navigate('/configs')}>
              Конфигурации
            </Button>
          )}
          <Button
            icon={<DownloadOutlined />}
            onClick={() => downloadWindowsAgent().catch(() => message.error('Агент не найден на сервере'))}
          >
            Скачать агент
          </Button>
        </Space>
      </div>

      {/* 4 hairline KPI tiles */}
      <div className="ts-kpi-grid">
        <div className="ts-kpi-tile ts-card-link" onClick={() => navigate('/agents')}>
          <div className="ts-kpi-label">
            <span>Агенты онлайн</span>
            <LaptopOutlined />
          </div>
          <div className="ts-kpi-value">
            {onlineAgents}
            <span style={{ fontSize: 16, fontWeight: 400, color: 'var(--p-muted)' }}> / {agents.length}</span>
          </div>
          <div className="ts-kpi-meta">
            <Progress percent={onlinePct} size="small" showInfo={false} strokeColor="var(--p-accent)" style={{ margin: 0 }} />
          </div>
        </div>

        <div className="ts-kpi-tile ts-card-link" onClick={() => navigate('/incidents')}>
          <div className="ts-kpi-label">
            <span>Открытые инциденты</span>
            <AlertOutlined />
          </div>
          <div className="ts-kpi-value" style={{ color: openCount > 0 ? '#f59e0b' : undefined }}>
            {openCount}
          </div>
          <div className="ts-kpi-meta">
            {openCount > 0 ? 'Ожидают проверки аналитиком' : 'Неразобранных событий нет'}
          </div>
        </div>

        <div className="ts-kpi-tile ts-card-link" onClick={() => navigate('/incidents')}>
          <div className="ts-kpi-label">
            <span>Критические события</span>
            <SecurityScanOutlined />
          </div>
          <div className="ts-kpi-value" style={{ color: criticalCount > 0 ? '#ef4444' : undefined }}>
            {criticalCount}
          </div>
          <div className="ts-kpi-meta">
            Всего в выборке: <span className="qorgau-mono">{incidents.length}</span>
          </div>
        </div>

        <div className="ts-kpi-tile ts-card-link" onClick={() => navigate(isOfficer ? '/surveillance' : '/employees')}>
          <div className="ts-kpi-label">
            <span>Сотрудники и снимки</span>
            <CameraOutlined />
          </div>
          <div className="ts-kpi-value">
            {employees.length}
            <span style={{ fontSize: 15, fontWeight: 400, color: 'var(--p-muted)' }}> · {totalScreenshots} сним.</span>
          </div>
          <div className="ts-kpi-meta">
            Контроль активности и экранов
          </div>
        </div>
      </div>

      {/* Charts + Risk Watchlist */}
      <Row gutter={[16, 16]} style={{ marginBottom: 20 }}>
        <Col xs={24} lg={11}>
          <Card
            title="Динамика инцидентов (14 дней)"
            extra={<Button type="link" size="small" onClick={() => navigate('/analytics')}>Аналитика <ArrowRightOutlined /></Button>}
            styles={{ body: { padding: '16px 14px 8px 0' } }}
          >
            {dailyData.length > 0 ? (
              <ResponsiveContainer width="100%" height={235}>
                <BarChart data={dailyData} margin={{ top: 6, right: 12, left: -12, bottom: 0 }}>
                  <CartesianGrid strokeDasharray="3 3" strokeOpacity={0.14} vertical={false} />
                  <XAxis dataKey="day" tick={{ fontSize: 11.5 }} axisLine={false} tickLine={false} />
                  <YAxis tick={{ fontSize: 11.5 }} allowDecimals={false} axisLine={false} tickLine={false} />
                  <ReTooltip
                    contentStyle={{
                      background: token.colorBgElevated,
                      border: `1px solid ${token.colorBorder}`,
                      borderRadius: 6,
                    }}
                  />
                  <Bar dataKey="critical" stackId="a" fill="#ef4444" name="Критический" />
                  <Bar dataKey="high" stackId="a" fill="#f59e0b" name="Высокий" />
                  <Bar dataKey="medium" stackId="a" fill="#5ab9fc" name="Средний" />
                  <Bar dataKey="low" stackId="a" fill="#64748b" name="Низкий" radius={[3, 3, 0, 0]} />
                </BarChart>
              </ResponsiveContainer>
            ) : (
              <Empty description="Инцидентов за период не зафиксировано" style={{ padding: '54px 0' }} />
            )}
          </Card>
        </Col>

        <Col xs={24} md={12} lg={6}>
          <Card title="Структура по серьёзности" styles={{ body: { padding: '14px 8px 8px' } }}>
            {pieData.length > 0 ? (
              <ResponsiveContainer width="100%" height={235}>
                <PieChart>
                  <Pie data={pieData} cx="50%" cy="46%" innerRadius={54} outerRadius={78} dataKey="value" nameKey="label" paddingAngle={2}>
                    {pieData.map(entry => (
                      <Cell key={entry.name} fill={SEV_COLORS[entry.name]} />
                    ))}
                  </Pie>
                  <Legend wrapperStyle={{ fontSize: 12 }} />
                  <ReTooltip />
                </PieChart>
              </ResponsiveContainer>
            ) : (
              <Empty description="Нет данных" style={{ padding: '54px 0' }} />
            )}
          </Card>
        </Col>

        <Col xs={24} md={12} lg={7}>
          <Card
            title="Сотрудники в зоне риска"
            extra={<Button type="link" size="small" onClick={() => navigate('/employees')}>Все <ArrowRightOutlined /></Button>}
            styles={{ body: { padding: '12px 18px' } }}
          >
            {highRiskEmployees.length === 0 ? (
              <Empty description="Нет данных по сотрудникам" style={{ padding: '44px 0' }} />
            ) : (
              <div style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
                {highRiskEmployees.map(emp => {
                  const color = emp.risk_score >= 70 ? '#ef4444' : emp.risk_score >= 40 ? '#f59e0b' : '#5ab9fc'
                  return (
                    <div
                      key={emp.username}
                      onClick={() => navigate('/employees')}
                      style={{ cursor: 'pointer' }}
                    >
                      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 4 }}>
                        <Space size={7}>
                          <span className="ts-dot" style={{ background: emp.is_online ? '#10b981' : '#64748b' }} />
                          <Text style={{ fontSize: 13.5, fontWeight: 500 }}>{emp.username}</Text>
                        </Space>
                        <Space size={6}>
                          {emp.critical_incidents > 0 && (
                            <Tooltip title="Критических инцидентов">
                              <Tag color="red" style={{ margin: 0, fontSize: 11 }}>{emp.critical_incidents} крит.</Tag>
                            </Tooltip>
                          )}
                          <span className="qorgau-mono" style={{ color, fontSize: 13, fontWeight: 500 }}>{emp.risk_score}/100</span>
                        </Space>
                      </div>
                      <Progress percent={Math.min(100, emp.risk_score)} size="small" showInfo={false} strokeColor={color} />
                    </div>
                  )
                })}
              </div>
            )}
          </Card>
        </Col>
      </Row>

      {/* Recent Incidents & Screenshots */}
      <Row gutter={[16, 16]}>
        <Col xs={24} xl={14}>
          <Card
            title="Последние инциденты"
            extra={<Button type="link" size="small" onClick={() => navigate('/incidents')}>Открыть журнал <ArrowRightOutlined /></Button>}
            styles={{ body: { padding: 0 } }}
          >
            <Spin spinning={incLoading}>
              <Table
                dataSource={recentIncidents}
                columns={recentCols}
                rowKey="id"
                size="small"
                pagination={false}
                scroll={{ x: 560 }}
                onRow={() => ({ onClick: () => navigate('/incidents'), style: { cursor: 'pointer' } })}
              />
            </Spin>
          </Card>
        </Col>

        <Col xs={24} xl={10}>
          <Card
            title="Последние снимки экранов"
            extra={isOfficer ? <Button type="link" size="small" onClick={() => navigate('/surveillance')}>Галерея <ArrowRightOutlined /></Button> : null}
          >
            {screenshots.length === 0 ? (
              <Empty description="Нет скриншотов" style={{ padding: '24px 0' }} />
            ) : (
              <Image.PreviewGroup>
                <div style={{ display: 'grid', gridTemplateColumns: 'repeat(3, 1fr)', gap: 10 }}>
                  {screenshots.slice(0, 6).map(s => (
                    <div key={s.id}>
                      <AuthScreenshotThumb id={s.id} />
                      <div className="qorgau-mono" style={{ fontSize: 11, color: 'var(--p-muted)', marginTop: 4, whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>
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
