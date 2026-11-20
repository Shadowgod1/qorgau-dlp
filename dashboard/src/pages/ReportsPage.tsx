import { useState } from 'react'
import {
  Button, Card, Col, DatePicker, Descriptions, Divider, Row, Select,
  Space, Spin, Statistic, Table, Tag, Typography,
} from 'antd'
import {
  DownloadOutlined, FileTextOutlined, BarChartOutlined,
  SafetyCertificateOutlined, TeamOutlined, FileExcelOutlined, ClockCircleOutlined,
} from '@ant-design/icons'
import { message } from 'antd'
import { useQuery } from '@tanstack/react-query'
import dayjs, { Dayjs } from 'dayjs'
import {
  fetchIncidentReport, fetchComplianceReport, fetchAgentsReport,
  downloadIncidentsExport, downloadCdr, downloadWorktimeExport,
} from '../api/client'

const { Title, Text } = Typography
const { RangePicker } = DatePicker

type ReportType = 'incidents' | 'agents' | 'compliance'

const REPORT_TYPES = [
  { value: 'incidents',  label: 'Отчёт по инцидентам',        icon: <BarChartOutlined /> },
  { value: 'agents',     label: 'Сводка по агентам',           icon: <TeamOutlined /> },
  { value: 'compliance', label: 'Соответствие законодательству РК', icon: <SafetyCertificateOutlined /> },
]

const SEVERITY_COLOR: Record<string, string> = {
  critical: 'red', high: 'orange', medium: 'gold', low: 'green',
}
const CHANNEL_LABELS: Record<string, string> = {
  messenger: 'Мессенджер', clipboard: 'Буфер обмена', usb: 'USB',
  web: 'Веб', app: 'Приложение', print: 'Печать', network: 'Сеть',
}

function IncidentsReport({ days }: { days: number }) {
  const { data, isLoading } = useQuery({
    queryKey: ['report-incidents', days],
    queryFn: () => fetchIncidentReport(days),
  })

  if (isLoading) return <Spin style={{ display: 'block', margin: '40px auto' }} />
  if (!data) return null

  const severityData = Object.entries(data.by_severity ?? {}).map(([k, v]) => ({
    severity: k, count: v as number,
  }))
  const channelData = Object.entries(data.by_channel ?? {}).map(([k, v]) => ({
    channel: k, count: v as number,
  }))

  return (
    <div>
      <Row gutter={16} style={{ marginBottom: 20 }}>
        <Col span={6}>
          <Card>
            <Statistic
              title="Всего инцидентов"
              value={severityData.reduce((s, r) => s + r.count, 0)}
            />
          </Card>
        </Col>
        <Col span={6}>
          <Card>
            <Statistic
              title="Критических"
              value={data.by_severity?.critical ?? 0}
              valueStyle={{ color: '#cf1322' }}
            />
          </Card>
        </Col>
        <Col span={6}>
          <Card>
            <Statistic
              title="Высоких"
              value={data.by_severity?.high ?? 0}
              valueStyle={{ color: '#d46b08' }}
            />
          </Card>
        </Col>
        <Col span={6}>
          <Card>
            <Statistic
              title="Период"
              value={`${days} дней`}
            />
          </Card>
        </Col>
      </Row>

      <Row gutter={16}>
        <Col span={12}>
          <Card title="По серьёзности" size="small">
            <Table
              dataSource={severityData}
              rowKey="severity"
              size="small"
              pagination={false}
              columns={[
                {
                  title: 'Серьёзность',
                  dataIndex: 'severity',
                  render: (v: string) => (
                    <Tag color={SEVERITY_COLOR[v] ?? 'default'}>{v.toUpperCase()}</Tag>
                  ),
                },
                { title: 'Кол-во', dataIndex: 'count', align: 'right' as const },
              ]}
            />
          </Card>
        </Col>
        <Col span={12}>
          <Card title="По каналам" size="small">
            <Table
              dataSource={channelData}
              rowKey="channel"
              size="small"
              pagination={false}
              columns={[
                {
                  title: 'Канал',
                  dataIndex: 'channel',
                  render: (v: string) => CHANNEL_LABELS[v] ?? v,
                },
                { title: 'Кол-во', dataIndex: 'count', align: 'right' as const },
              ]}
            />
          </Card>
        </Col>
      </Row>

      <Divider />
      <Card title="Топ пользователей по инцидентам" size="small">
        <Table
          dataSource={data.top_users ?? []}
          rowKey="username"
          size="small"
          pagination={false}
          columns={[
            { title: 'Пользователь', dataIndex: 'username' },
            { title: 'Инциденты', dataIndex: 'count', align: 'right' as const },
          ]}
        />
      </Card>
    </div>
  )
}

function AgentsReport() {
  const { data, isLoading } = useQuery({
    queryKey: ['report-agents'],
    queryFn: fetchAgentsReport,
  })

  if (isLoading) return <Spin style={{ display: 'block', margin: '40px auto' }} />
  if (!data) return null

  return (
    <Row gutter={[16, 16]}>
      <Col xs={24} sm={8}>
        <Card>
          <Statistic title="Всего агентов" value={data.total ?? data.total_agents ?? 0} />
        </Card>
      </Col>
      <Col xs={24} sm={8}>
        <Card>
          <Statistic
            title="Онлайн"
            value={data.online ?? data.online_agents ?? 0}
            valueStyle={{ color: '#3f8600' }}
          />
        </Card>
      </Col>
      <Col xs={24} sm={8}>
        <Card>
          <Statistic
            title="Офлайн"
            value={data.offline ?? data.offline_agents ?? 0}
            valueStyle={{ color: '#cf1322' }}
          />
        </Card>
      </Col>
    </Row>
  )
}

function ComplianceReport() {
  const { data, isLoading } = useQuery({
    queryKey: ['report-compliance'],
    queryFn: fetchComplianceReport,
  })

  if (isLoading) return <Spin style={{ display: 'block', margin: '40px auto' }} />
  if (!data) return null

  const reqs: { id: string; name: string; law_ref?: string; status: string; description: string; details?: string }[] =
    Array.isArray(data) ? data : (Array.isArray(data.requirements) ? data.requirements : [])

  const statusMeta = (v: string) => {
    if (v === 'compliant' || v === 'implemented') return { color: 'green', label: 'Выполнено' }
    if (v === 'warning' || v === 'partial') return { color: 'gold', label: 'Предупреждение' }
    return { color: 'red', label: 'Не выполнено' }
  }

  return (
    <Card
      title="Требования законодательства РК в области ИБ"
      extra={data.score_pct != null ? <Tag color={data.score_pct >= 80 ? 'green' : 'orange'}>Соответствие: {data.score_pct}%</Tag> : null}
    >
      <Table
        dataSource={reqs}
        rowKey="id"
        size="small"
        pagination={false}
        columns={[
          { title: 'Норма', dataIndex: 'law_ref', width: 200, render: (v: string, r) => v || r.id },
          { title: 'Требование', dataIndex: 'name' },
          { title: 'Подробности', dataIndex: 'details', render: (v: string, r) => v || r.description },
          {
            title: 'Статус',
            dataIndex: 'status',
            width: 140,
            render: (v: string) => {
              const m = statusMeta(v)
              return <Tag color={m.color}>{m.label}</Tag>
            },
          },
        ]}
      />
    </Card>
  )
}

export default function ReportsPage() {
  const [reportType, setReportType] = useState<ReportType>('incidents')
  const [dateRange, setDateRange] = useState<[Dayjs, Dayjs]>([
    dayjs().subtract(30, 'day'), dayjs(),
  ])

  const days = Math.min(365, Math.max(1, dateRange[1].diff(dateRange[0], 'day') + 1))

  const runDownload = async (fn: () => Promise<void>) => {
    try {
      await fn()
    } catch {
      message.error('Ошибка экспорта (проверьте права доступа)')
    }
  }

  return (
    <div>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 20 }}>
        <div>
          <Title level={4} style={{ margin: 0 }}>Отчёты</Title>
          <Text type="secondary" style={{ fontSize: 13 }}>Аналитика и экспорт данных</Text>
        </div>
      </div>

      <Card style={{ marginBottom: 16 }}>
        <Space wrap size={12}>
          <div>
            <Text style={{ fontSize: 12, display: 'block', marginBottom: 4 }}>Тип отчёта</Text>
            <Select
              value={reportType}
              onChange={setReportType}
              style={{ width: 280 }}
              options={REPORT_TYPES.map(r => ({ value: r.value, label: r.label }))}
            />
          </div>

          {reportType === 'incidents' && (
            <div>
              <Text style={{ fontSize: 12, display: 'block', marginBottom: 4 }}>Период</Text>
              <RangePicker
                value={dateRange}
                onChange={(v) => v && setDateRange(v as [Dayjs, Dayjs])}
                format="DD.MM.YYYY"
                allowClear={false}
              />
            </div>
          )}

          {reportType === 'incidents' && (
            <div>
              <Text style={{ fontSize: 12, display: 'block', marginBottom: 4 }}>Экспорт инцидентов</Text>
              <Space wrap>
                <Button icon={<DownloadOutlined />} onClick={() => runDownload(() => downloadIncidentsExport('csv', days))}>
                  CSV
                </Button>
                <Button icon={<FileTextOutlined />} onClick={() => runDownload(() => downloadIncidentsExport('json', days))}>
                  JSON
                </Button>
                <Button icon={<FileExcelOutlined />} onClick={() => runDownload(() => downloadIncidentsExport('xlsx', days))}>
                  Excel
                </Button>
              </Space>
            </div>
          )}

          <div>
            <Text style={{ fontSize: 12, display: 'block', marginBottom: 4 }}>CDR (детальные записи)</Text>
            <Space wrap>
              <Button icon={<DownloadOutlined />} onClick={() => runDownload(() => downloadCdr('csv', days))}>
                CDR CSV
              </Button>
              <Button icon={<FileExcelOutlined />} onClick={() => runDownload(() => downloadCdr('xlsx', days))}>
                CDR Excel
              </Button>
            </Space>
          </div>

          <div>
            <Text style={{ fontSize: 12, display: 'block', marginBottom: 4 }}>Учёт рабочего времени</Text>
            <Button icon={<ClockCircleOutlined />} onClick={() => runDownload(() => downloadWorktimeExport(days))}>
              Excel
            </Button>
          </div>
        </Space>
      </Card>

      {reportType === 'incidents' && <IncidentsReport days={days} />}
      {reportType === 'agents'    && <AgentsReport />}
      {reportType === 'compliance' && <ComplianceReport />}
    </div>
  )
}
