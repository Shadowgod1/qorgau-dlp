import { useState } from 'react'
import {
  Button, Card, Col, DatePicker, Divider, Row, Progress,
  Space, Spin, Table, Tag, Typography, Segmented, Empty, message, theme as antTheme,
} from 'antd'
import {
  DownloadOutlined, FileTextOutlined, BarChartOutlined,
  SafetyCertificateOutlined, TeamOutlined, FileExcelOutlined, ClockCircleOutlined,
  PrinterOutlined, UsbOutlined, LaptopOutlined,
} from '@ant-design/icons'
import { useQuery } from '@tanstack/react-query'
import {
  BarChart, Bar, XAxis, YAxis, CartesianGrid, Tooltip, ResponsiveContainer,
  PieChart, Pie, Cell, Legend,
} from 'recharts'
import dayjs, { Dayjs } from 'dayjs'
import {
  fetchIncidentReport, fetchComplianceReport, fetchAgentsReport,
  fetchEmployees, fetchAgents, fetchDiscoveryFindings, fetchUsbDevices,
  downloadIncidentsExport, downloadCdr, downloadWorktimeExport,
} from '../api/client'

const { Title, Text } = Typography
const { RangePicker } = DatePicker

type ReportType = 'incidents' | 'employees' | 'agents' | 'prevention' | 'compliance'

const REPORT_CARDS: { value: ReportType; title: string; subtitle: string; icon: React.ReactNode }[] = [
  { value: 'incidents',  title: 'Инциденты DLP',   subtitle: 'Каналы, уровни, нарушители',  icon: <BarChartOutlined /> },
  { value: 'employees',  title: 'Сотрудники и УРВ', subtitle: 'Risk Score и продуктивность', icon: <TeamOutlined /> },
  { value: 'agents',     title: 'Парк агентов',    subtitle: 'Покрытие, ОС, профили',       icon: <LaptopOutlined /> },
  { value: 'prevention', title: 'ПДн и ВНИ (USB)', subtitle: 'Краулер дисков и флешки',     icon: <UsbOutlined /> },
  { value: 'compliance', title: 'Комплаенс РК',    subtitle: 'ТК РК, ПДн №94-V, №410',     icon: <SafetyCertificateOutlined /> },
]

const SEVERITY_COLOR: Record<string, string> = {
  critical: '#ef4444', high: '#f59e0b', medium: '#5ab9fc', low: '#64748b',
}
const SEVERITY_LABELS: Record<string, string> = {
  critical: 'Критический', high: 'Высокий', medium: 'Средний', low: 'Низкий',
}
const CHANNEL_LABELS: Record<string, string> = {
  messenger: 'Мессенджер', clipboard: 'Буфер обмена', usb: 'USB / ВНИ',
  web: 'Веб', app: 'Приложение', print: 'Печать', network: 'Сеть', file: 'Файлы', ocr: 'OCR экрана',
}

function fmtHours(sec: number) {
  const h = Math.floor(sec / 3600)
  const m = Math.floor((sec % 3600) / 60)
  return `${h}ч ${m}м`
}

function IncidentsReport({ days }: { days: number }) {
  const { token } = antTheme.useToken()
  const { data, isLoading } = useQuery({
    queryKey: ['report-incidents', days],
    queryFn: () => fetchIncidentReport(days),
  })

  if (isLoading) return <Spin style={{ display: 'block', margin: '48px auto' }} />
  if (!data) return null

  const severityData = Object.entries(data.by_severity ?? {}).map(([k, v]) => ({
    severity: k, label: SEVERITY_LABELS[k] ?? k, count: v as number,
  }))
  const channelData = Object.entries(data.by_channel ?? {}).map(([k, v]) => ({
    channel: k, label: CHANNEL_LABELS[k] ?? k, count: v as number,
  }))
  const totalInc = severityData.reduce((s, r) => s + r.count, 0)

  return (
    <div>
      <div className="ts-kpi-grid">
        <div className="ts-kpi-tile">
          <div className="ts-kpi-label">Всего инцидентов</div>
          <div className="ts-kpi-value">{totalInc}</div>
          <div className="ts-kpi-meta">За выбранный период</div>
        </div>
        <div className="ts-kpi-tile">
          <div className="ts-kpi-label">Критических</div>
          <div className="ts-kpi-value" style={{ color: '#ef4444' }}>{data.by_severity?.critical ?? 0}</div>
          <div className="ts-kpi-meta">Высокий приоритет SOC</div>
        </div>
        <div className="ts-kpi-tile">
          <div className="ts-kpi-label">Высоких</div>
          <div className="ts-kpi-value" style={{ color: '#f59e0b' }}>{data.by_severity?.high ?? 0}</div>
          <div className="ts-kpi-meta">Требуют разбора</div>
        </div>
        <div className="ts-kpi-tile">
          <div className="ts-kpi-label">Период анализа</div>
          <div className="ts-kpi-value">{days} дн.</div>
          <div className="ts-kpi-meta">Актуальная выборка БД</div>
        </div>
      </div>

      <Row gutter={[16, 16]} style={{ marginBottom: 18 }}>
        <Col xs={24} lg={14}>
          <Card title="Распределение по каналам утечки" size="small">
            {channelData.length > 0 ? (
              <ResponsiveContainer width="100%" height={240}>
                <BarChart data={channelData} margin={{ top: 8, right: 16, left: -12, bottom: 0 }}>
                  <CartesianGrid strokeDasharray="3 3" strokeOpacity={0.14} vertical={false} />
                  <XAxis dataKey="label" tick={{ fontSize: 11.5 }} axisLine={false} tickLine={false} />
                  <YAxis tick={{ fontSize: 11.5 }} allowDecimals={false} axisLine={false} tickLine={false} />
                  <Tooltip contentStyle={{ background: token.colorBgElevated, border: `1px solid ${token.colorBorder}`, borderRadius: 6 }} />
                  <Bar dataKey="count" fill="#5ab9fc" name="Инциденты" radius={[4, 4, 0, 0]} />
                </BarChart>
              </ResponsiveContainer>
            ) : <Empty description="Нет данных за выбранный период" style={{ padding: 40 }} />}
          </Card>
        </Col>
        <Col xs={24} lg={10}>
          <Card title="Структура по серьёзности" size="small">
            {severityData.some(d => d.count > 0) ? (
              <ResponsiveContainer width="100%" height={240}>
                <PieChart>
                  <Pie data={severityData.filter(d => d.count > 0)} dataKey="count" nameKey="label" cx="50%" cy="46%" innerRadius={52} outerRadius={78}>
                    {severityData.filter(d => d.count > 0).map(e => (
                      <Cell key={e.severity} fill={SEVERITY_COLOR[e.severity] ?? '#64748b'} />
                    ))}
                  </Pie>
                  <Legend />
                  <Tooltip />
                </PieChart>
              </ResponsiveContainer>
            ) : <Empty description="Нет данных" style={{ padding: 40 }} />}
          </Card>
        </Col>
      </Row>

      <Card title="Топ сотрудников по числу инцидентов" size="small" styles={{ body: { padding: 0 } }}>
        <Table
          dataSource={data.top_users ?? []}
          rowKey="username"
          size="small"
          pagination={false}
          columns={[
            { title: 'Сотрудник', dataIndex: 'username', render: (v: string) => <Text strong>{v}</Text> },
            {
              title: 'Инцидентов',
              dataIndex: 'count',
              align: 'right' as const,
              render: (v: number) => <Tag color={v >= 5 ? 'red' : v >= 2 ? 'gold' : 'default'}>{v}</Tag>,
            },
          ]}
        />
      </Card>
    </div>
  )
}

function EmployeesReport({ days }: { days: number }) {
  const { data: employees = [], isLoading } = useQuery({
    queryKey: ['report-employees', days],
    queryFn: () => fetchEmployees(days),
  })

  if (isLoading) return <Spin style={{ display: 'block', margin: '48px auto' }} />

  const avgProd = employees.length > 0
    ? Math.round(employees.reduce((s, e) => s + e.productivity_pct, 0) / employees.length)
    : 0
  const highRiskCount = employees.filter(e => e.risk_score >= 60).length
  const totalActiveSec = employees.reduce((s, e) => s + e.active_sec, 0)

  return (
    <div>
      <div className="ts-kpi-grid">
        <div className="ts-kpi-tile">
          <div className="ts-kpi-label">Всего сотрудников</div>
          <div className="ts-kpi-value">{employees.length}</div>
          <div className="ts-kpi-meta">Активные учётные записи</div>
        </div>
        <div className="ts-kpi-tile">
          <div className="ts-kpi-label">Высокий риск (≥ 60)</div>
          <div className="ts-kpi-value" style={{ color: highRiskCount > 0 ? '#ef4444' : '#10b981' }}>{highRiskCount}</div>
          <div className="ts-kpi-meta">Превышают порог SOC</div>
        </div>
        <div className="ts-kpi-tile">
          <div className="ts-kpi-label">Средняя продуктивность</div>
          <div className="ts-kpi-value" style={{ color: 'var(--p-accent)' }}>{avgProd}%</div>
          <div className="ts-kpi-meta">По классификатору ПО</div>
        </div>
        <div className="ts-kpi-tile">
          <div className="ts-kpi-label">Суммарное активное время</div>
          <div className="ts-kpi-value">{fmtHours(totalActiveSec)}</div>
          <div className="ts-kpi-meta">За период {days} дн.</div>
        </div>
      </div>

      <Card title={`Сводная ведомость сотрудников за ${days} дн.`} size="small" styles={{ body: { padding: 0 } }}>
        <Table
          dataSource={[...employees].sort((a, b) => b.risk_score - a.risk_score)}
          rowKey="username"
          size="small"
          pagination={{ pageSize: 20 }}
          columns={[
            { title: 'Сотрудник', dataIndex: 'username', render: (v: string) => <Text strong>{v}</Text> },
            {
              title: 'Станции',
              dataIndex: 'hostnames',
              render: (hosts: string[]) => (hosts ?? []).map(h => <Tag key={h} bordered={false}>{h}</Tag>),
            },
            {
              title: 'Risk Score',
              dataIndex: 'risk_score',
              width: 155,
              render: (v: number) => {
                const c = v >= 70 ? '#ef4444' : v >= 40 ? '#f59e0b' : '#5ab9fc'
                return <Progress percent={Math.min(100, v)} size="small" strokeColor={c} format={() => `${v}/100`} />
              },
            },
            {
              title: 'Инциденты',
              key: 'inc',
              width: 145,
              render: (_, r) => (
                <Space size={4}>
                  <span className="qorgau-mono">{r.total_incidents}</span>
                  {r.critical_incidents > 0 && <Tag color="red">{r.critical_incidents} крит.</Tag>}
                </Space>
              ),
            },
            {
              title: 'Активное время',
              dataIndex: 'active_sec',
              width: 130,
              render: (v: number) => <span className="qorgau-mono">{fmtHours(v || 0)}</span>,
            },
            {
              title: 'Продуктивность',
              dataIndex: 'productivity_pct',
              width: 130,
              render: (v: number) => <Tag color={v >= 70 ? 'green' : v >= 45 ? 'gold' : 'red'}>{Math.round(v)}%</Tag>,
            },
          ]}
        />
      </Card>
    </div>
  )
}

function AgentsReport() {
  const { data, isLoading } = useQuery({ queryKey: ['report-agents'], queryFn: fetchAgentsReport })
  const { data: agents = [] } = useQuery({ queryKey: ['agents'], queryFn: fetchAgents })

  if (isLoading) return <Spin style={{ display: 'block', margin: '48px auto' }} />
  if (!data) return null

  const total = data.total ?? data.total_agents ?? agents.length
  const online = data.online ?? data.online_agents ?? agents.filter(a => a.is_online).length
  const offline = data.offline ?? data.offline_agents ?? Math.max(0, total - online)
  const withConfig = agents.filter(a => !!a.config_id).length

  return (
    <div>
      <div className="ts-kpi-grid">
        <div className="ts-kpi-tile">
          <div className="ts-kpi-label">Всего агентов</div>
          <div className="ts-kpi-value">{total}</div>
          <div className="ts-kpi-meta">Зарегистрировано</div>
        </div>
        <div className="ts-kpi-tile">
          <div className="ts-kpi-label">Онлайн</div>
          <div className="ts-kpi-value" style={{ color: '#10b981' }}>{online}</div>
          <div className="ts-kpi-meta">Активные соединения</div>
        </div>
        <div className="ts-kpi-tile">
          <div className="ts-kpi-label">Офлайн</div>
          <div className="ts-kpi-value">{offline}</div>
          <div className="ts-kpi-meta">Вне сети</div>
        </div>
        <div className="ts-kpi-tile">
          <div className="ts-kpi-label">С назначенным профилем</div>
          <div className="ts-kpi-value" style={{ color: 'var(--p-accent)' }}>{withConfig} / {total}</div>
          <div className="ts-kpi-meta">Покрытие политиками</div>
        </div>
      </div>

      <Card title="Реестр защищаемых рабочих станций" size="small" styles={{ body: { padding: 0 } }}>
        <Table
          dataSource={agents}
          rowKey="agent_id"
          size="small"
          pagination={{ pageSize: 20 }}
          columns={[
            { title: 'Хост', dataIndex: 'hostname', render: (v: string) => <Text strong>{v}</Text> },
            { title: 'Пользователь', dataIndex: 'username' },
            { title: 'IP', dataIndex: 'ip_address', render: (v: string) => <span className="qorgau-mono">{v}</span> },
            { title: 'ОС', dataIndex: 'os_version', ellipsis: true },
            { title: 'Версия', dataIndex: 'agent_version', render: (v: string) => <span className="qorgau-mono">{v}</span> },
            {
              title: 'Статус',
              dataIndex: 'is_online',
              render: (v: boolean) => (
                <Space size={6}>
                  <span className="ts-dot" style={{ background: v ? '#10b981' : '#64748b' }} />
                  <span>{v ? 'Онлайн' : 'Офлайн'}</span>
                </Space>
              ),
            },
          ]}
        />
      </Card>
    </div>
  )
}

function PreventionReport() {
  const { data: findings = [], isLoading: fLoad } = useQuery({
    queryKey: ['report-discovery'],
    queryFn: () => fetchDiscoveryFindings({ limit: 200 }),
  })
  const { data: usbs = [], isLoading: uLoad } = useQuery({
    queryKey: ['report-usbs'],
    queryFn: fetchUsbDevices,
  })

  if (fLoad || uLoad) return <Spin style={{ display: 'block', margin: '48px auto' }} />

  const openFindings = findings.filter(f => f.status === 'open').length
  const blockedUsb = usbs.filter(u => u.status === 'blocked').length
  const allowedUsb = usbs.filter(u => u.status === 'allowed').length

  return (
    <div>
      <div className="ts-kpi-grid">
        <div className="ts-kpi-tile">
          <div className="ts-kpi-label">Открытые находки ПДн</div>
          <div className="ts-kpi-value" style={{ color: openFindings > 0 ? '#ef4444' : '#10b981' }}>{openFindings}</div>
          <div className="ts-kpi-meta">Файлы на дисках ПК</div>
        </div>
        <div className="ts-kpi-tile">
          <div className="ts-kpi-label">Всего файлов ПДн</div>
          <div className="ts-kpi-value">{findings.length}</div>
          <div className="ts-kpi-meta">Обнаружено краулером</div>
        </div>
        <div className="ts-kpi-tile">
          <div className="ts-kpi-label">Разрешённые ВНИ (USB)</div>
          <div className="ts-kpi-value" style={{ color: '#10b981' }}>{allowedUsb}</div>
          <div className="ts-kpi-meta">В белом списке</div>
        </div>
        <div className="ts-kpi-tile">
          <div className="ts-kpi-label">Заблокированные ВНИ</div>
          <div className="ts-kpi-value" style={{ color: '#f59e0b' }}>{blockedUsb}</div>
          <div className="ts-kpi-meta">Извлекаются агентом</div>
        </div>
      </div>

      <Row gutter={[16, 16]}>
        <Col xs={24} lg={14}>
          <Card title="Файлы с ПДн на дисках (Краулер)" size="small" styles={{ body: { padding: 0 } }}>
            <Table
              dataSource={findings.slice(0, 20)}
              rowKey="id"
              size="small"
              pagination={false}
              columns={[
                { title: 'Хост / пользователь', key: 'u', render: (_, r) => `${r.hostname} (${r.username})` },
                { title: 'Путь к файлу', dataIndex: 'file_path', ellipsis: true },
                {
                  title: 'Уровень',
                  dataIndex: 'severity',
                  width: 110,
                  render: (s: string) => (
                    <Space size={6}>
                      <span className="ts-dot" style={{ background: SEVERITY_COLOR[s] ?? '#64748b' }} />
                      <span>{SEVERITY_LABELS[s] ?? s}</span>
                    </Space>
                  ),
                },
                { title: 'Статус', dataIndex: 'status', width: 95, render: (s: string) => <Tag bordered={false}>{s}</Tag> },
              ]}
            />
          </Card>
        </Col>
        <Col xs={24} lg={10}>
          <Card title="Реестр USB-носителей (ВНИ)" size="small" styles={{ body: { padding: 0 } }}>
            <Table
              dataSource={usbs}
              rowKey="id"
              size="small"
              pagination={false}
              columns={[
                { title: 'Метка / серийник', key: 's', render: (_, r) => <div><strong>{r.label || 'USB'}</strong><div className="qorgau-mono" style={{ fontSize: 11.5, color: 'var(--p-muted)' }}>{r.serial_number}</div></div> },
                { title: 'Владелец', dataIndex: 'owner', render: (v: string | null) => v || '—' },
                {
                  title: 'Политика',
                  dataIndex: 'status',
                  width: 105,
                  render: (s: string) => <Tag color={s === 'allowed' ? 'green' : s === 'blocked' ? 'red' : 'gold'}>{s}</Tag>,
                },
              ]}
            />
          </Card>
        </Col>
      </Row>
    </div>
  )
}

function ComplianceReport() {
  const { data, isLoading } = useQuery({
    queryKey: ['report-compliance'],
    queryFn: fetchComplianceReport,
  })

  if (isLoading) return <Spin style={{ display: 'block', margin: '48px auto' }} />
  if (!data) return null

  const reqs: { id: string; name: string; law_ref?: string; status: string; description: string; details?: string }[] =
    Array.isArray(data) ? data : (Array.isArray(data.requirements) ? data.requirements : [])

  const statusMeta = (v: string) => {
    if (v === 'compliant' || v === 'implemented') return { color: 'green', label: 'Выполнено' }
    if (v === 'warning' || v === 'partial') return { color: 'gold', label: 'Частично' }
    return { color: 'red', label: 'Не выполнено' }
  }

  const score = data.score_pct ?? 100

  return (
    <Row gutter={[16, 16]}>
      <Col xs={24} lg={7}>
        <Card style={{ textAlign: 'center', height: '100%' }}>
          <Title level={5} style={{ marginBottom: 20 }}>Индекс соответствия РК</Title>
          <Progress
            type="dashboard"
            percent={Math.round(score)}
            strokeColor={score >= 85 ? '#10b981' : score >= 60 ? '#f59e0b' : '#ef4444'}
            size={170}
          />
          <Divider />
          <Text type="secondary" style={{ fontSize: 13, display: 'block', lineHeight: 1.55 }}>
            Закон РК «О персональных данных и их защите» №94-V, ТК РК ст. 23 и Приказ №410.
          </Text>
        </Card>
      </Col>
      <Col xs={24} lg={17}>
        <Card title="Детализация требований законодательства РК" styles={{ body: { padding: 0 } }}>
          <Table
            dataSource={reqs}
            rowKey="id"
            size="small"
            pagination={false}
            columns={[
              { title: 'Норма', dataIndex: 'law_ref', width: 185, render: (v: string, r) => <Tag bordered={false}>{v || r.id}</Tag> },
              { title: 'Требование', dataIndex: 'name', render: (v: string) => <Text strong>{v}</Text> },
              { title: 'Подробности', dataIndex: 'details', render: (v: string, r) => v || r.description },
              {
                title: 'Статус',
                dataIndex: 'status',
                width: 125,
                render: (v: string) => {
                  const m = statusMeta(v)
                  return <Tag color={m.color}>{m.label}</Tag>
                },
              },
            ]}
          />
        </Card>
      </Col>
    </Row>
  )
}

export default function ReportsPage() {
  const [reportType, setReportType] = useState<ReportType>('incidents')
  const [presetDays, setPresetDays] = useState<number | 'custom'>(30)
  const [dateRange, setDateRange] = useState<[Dayjs, Dayjs]>([
    dayjs().subtract(30, 'day'), dayjs(),
  ])

  const days = Math.min(365, Math.max(1, dateRange[1].diff(dateRange[0], 'day') + 1))

  const applyQuickDays = (d: number) => {
    setPresetDays(d)
    setDateRange([dayjs().subtract(d - 1, 'day'), dayjs()])
  }

  const runDownload = async (fn: () => Promise<void>) => {
    try {
      await fn()
      message.success('Файл экспорта сформирован')
    } catch {
      message.error('Ошибка экспорта (проверьте права доступа)')
    }
  }

  const activeMeta = REPORT_CARDS.find(c => c.value === reportType)!

  return (
    <div className="ts-enter">
      {/* Clean print header (visible only in PDF/Print mode) */}
      <div className="qorgau-print-header" style={{ marginBottom: 20, paddingBottom: 12, borderBottom: '2px solid #0a0d14' }}>
        <h2 style={{ margin: 0 }}>Qorgau DLP — Официальный отчёт: {activeMeta.title}</h2>
        <div style={{ fontSize: 12, color: '#54575f', marginTop: 4 }}>
          Период: {dateRange[0].format('DD.MM.YYYY')} — {dateRange[1].format('DD.MM.YYYY')} ({days} дн.) · Сформировано: {dayjs().format('DD.MM.YYYY HH:mm')}
        </div>
      </div>

      <div className="ts-page-head qorgau-no-print">
        <div>
          <Title level={2} className="ts-page-title">Отчёты и аналитика</Title>
          <span className="ts-page-sub">
            Выгрузки для руководства и службы ИБ (Excel, CSV, JSON, CDR) и проверка соответствия законам РК
          </span>
        </div>
        <Button type="primary" icon={<PrinterOutlined />} onClick={() => window.print()}>
          Печать / сохранить PDF
        </Button>
      </div>

      {/* 5 Clean Report Selector Cards */}
      <div
        className="qorgau-no-print"
        style={{
          display: 'grid',
          gridTemplateColumns: 'repeat(auto-fit, minmax(195px, 1fr))',
          gap: 12,
          marginBottom: 16,
        }}
      >
        {REPORT_CARDS.map(card => {
          const active = reportType === card.value
          return (
            <div
              key={card.value}
              onClick={() => setReportType(card.value)}
              className={`ts-card-link ${active ? 'is-active' : ''}`}
              style={{
                background: 'var(--p-surface)',
                border: '1px solid var(--p-hairline)',
                borderRadius: 8,
                padding: '14px 16px',
                boxShadow: 'var(--p-top-highlight)',
              }}
            >
              <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 6 }}>
                <span style={{ fontWeight: 500, fontSize: 14, color: 'var(--p-ink)' }}>{card.title}</span>
                <span style={{ color: active ? 'var(--p-accent)' : 'var(--p-muted)', fontSize: 15 }}>{card.icon}</span>
              </div>
              <div style={{ fontSize: 12.5, color: 'var(--p-muted)' }}>{card.subtitle}</div>
            </div>
          )
        })}
      </div>

      {/* Period & Export Bar */}
      <Card size="small" style={{ marginBottom: 18 }} className="qorgau-no-print">
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', flexWrap: 'wrap', gap: 12 }}>
          <Space wrap size={10}>
            <Segmented
              value={presetDays}
              onChange={v => { if (typeof v === 'number') applyQuickDays(v) }}
              options={[
                { label: 'Сегодня', value: 1 },
                { label: '7 дн.', value: 7 },
                { label: '30 дн.', value: 30 },
                { label: '90 дн.', value: 90 },
                { label: 'Год', value: 365 },
              ]}
            />
            <RangePicker
              value={dateRange}
              onChange={(v) => {
                if (v && v[0] && v[1]) {
                  setPresetDays('custom')
                  setDateRange([v[0], v[1]])
                }
              }}
              format="DD.MM.YYYY"
              allowClear={false}
            />
          </Space>

          <Space wrap size={6}>
            <Button size="small" icon={<FileExcelOutlined />} onClick={() => runDownload(() => downloadIncidentsExport('xlsx', days))}>
              Инциденты XLSX
            </Button>
            <Button size="small" icon={<DownloadOutlined />} onClick={() => runDownload(() => downloadIncidentsExport('csv', days))}>
              CSV
            </Button>
            <Button size="small" icon={<FileTextOutlined />} onClick={() => runDownload(() => downloadIncidentsExport('json', days))}>
              JSON
            </Button>
            <Button size="small" icon={<FileExcelOutlined />} onClick={() => runDownload(() => downloadCdr('xlsx', days))}>
              CDR Excel
            </Button>
            <Button size="small" icon={<ClockCircleOutlined />} onClick={() => runDownload(() => downloadWorktimeExport(days))}>
              УРВ Excel
            </Button>
          </Space>
        </div>
      </Card>

      {reportType === 'incidents'  && <IncidentsReport days={days} />}
      {reportType === 'employees'  && <EmployeesReport days={days} />}
      {reportType === 'agents'     && <AgentsReport />}
      {reportType === 'prevention' && <PreventionReport />}
      {reportType === 'compliance' && <ComplianceReport />}
    </div>
  )
}
