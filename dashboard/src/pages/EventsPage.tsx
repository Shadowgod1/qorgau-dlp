import { useState, useCallback } from 'react'
import {
  Card, Table, Tag, Select, Input, Button, Space, DatePicker,
  Typography, Drawer, Descriptions, Spin,
} from 'antd'
import { ReloadOutlined, SearchOutlined } from '@ant-design/icons'
import { useQuery } from '@tanstack/react-query'
import dayjs from 'dayjs'
import { fetchTimeline, fetchAgents } from '../api/client'
import type { TimelineEvent, EventType, Severity } from '../api/types'

const { Title } = Typography
const { RangePicker } = DatePicker

const TYPE_COLOR: Record<string, string> = {
  incident: 'red', usb: 'orange', messenger: 'purple', app: 'blue',
  web: 'cyan', screenshot: 'green', session: 'default', clipboard: 'magenta',
  network: 'geekblue', print: 'gold',
}

const SEV_COLOR: Record<string, string> = {
  critical: 'red', high: 'orange', medium: 'gold', low: 'green',
}

const TYPE_OPTIONS = [
  'incident','usb','messenger','app','web','clipboard','network','print','screenshot','session',
].map(t => ({ label: t, value: t }))

const SEV_OPTIONS = ['critical','high','medium','low'].map(s => ({ label: s, value: s }))

export default function EventsPage() {
  const [selected, setSelected] = useState<TimelineEvent | null>(null)
  const [search, setSearch] = useState('')
  const [eventTypes, setEventTypes] = useState<EventType[]>([])
  const [severity, setSeverity] = useState<Severity | null>(null)
  const [agentId, setAgentId] = useState<string | null>(null)
  const [dateRange, setDateRange] = useState<[dayjs.Dayjs, dayjs.Dayjs] | null>(null)
  const [page, setPage] = useState(1)
  const PAGE_SIZE = 50

  const { data: agents = [] } = useQuery({ queryKey: ['agents'], queryFn: fetchAgents })

  const params: Record<string, unknown> = {
    limit: PAGE_SIZE,
    offset: (page - 1) * PAGE_SIZE,
  }
  if (eventTypes.length) params.event_types = eventTypes.join(',')
  if (severity) params.severity = severity
  if (agentId) params.agent_id = agentId
  if (search) params.search = search
  if (dateRange) {
    params.date_from = dateRange[0].toISOString()
    params.date_to = dateRange[1].toISOString()
  }

  const { data, isLoading, refetch } = useQuery({
    queryKey: ['timeline', params],
    queryFn: () => fetchTimeline(params),
    refetchInterval: 20_000,
  })

  const reset = useCallback(() => {
    setSearch(''); setEventTypes([]); setSeverity(null)
    setAgentId(null); setDateRange(null); setPage(1)
  }, [])

  const columns = [
    {
      title: 'Время', dataIndex: 'ts', key: 'ts', width: 140,
      render: (v: string) => dayjs(v).format('DD.MM.YY HH:mm:ss'),
    },
    {
      title: 'Тип', dataIndex: 'type', key: 'type', width: 100,
      render: (v: string) => <Tag color={TYPE_COLOR[v] || 'default'}>{v}</Tag>,
    },
    {
      title: 'Серьёзность', dataIndex: 'severity', key: 'severity', width: 100,
      render: (v: string | null) => v ? <Tag color={SEV_COLOR[v]}>{v}</Tag> : <span style={{ color: '#aaa' }}>—</span>,
    },
    { title: 'Хост', dataIndex: 'hostname', key: 'hostname', width: 140 },
    { title: 'Пользователь', dataIndex: 'username', key: 'username', width: 120 },
    { title: 'Подтип', dataIndex: 'subtype', key: 'subtype', width: 120 },
    { title: 'Описание', dataIndex: 'summary', key: 'summary', ellipsis: true },
  ]

  return (
    <div>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 16 }}>
        <Title level={4} style={{ margin: 0 }}>События</Title>
        <Button icon={<ReloadOutlined />} onClick={() => refetch()}>Обновить</Button>
      </div>

      {/* Filters */}
      <Card style={{ marginBottom: 16 }} styles={{ body: { padding: '12px 16px' } }}>
        <Space wrap size={8}>
          <Input
            placeholder="Поиск..."
            prefix={<SearchOutlined />}
            value={search}
            onChange={e => { setSearch(e.target.value); setPage(1) }}
            style={{ width: 200 }}
            allowClear
          />
          <Select
            mode="multiple"
            placeholder="Тип события"
            options={TYPE_OPTIONS}
            value={eventTypes}
            onChange={v => { setEventTypes(v as EventType[]); setPage(1) }}
            style={{ width: 260 }}
            maxTagCount={2}
            allowClear
          />
          <Select
            placeholder="Серьёзность"
            options={SEV_OPTIONS}
            value={severity}
            onChange={v => { setSeverity(v); setPage(1) }}
            style={{ width: 130 }}
            allowClear
          />
          <Select
            placeholder="Агент"
            options={agents.map(a => ({ label: a.hostname, value: a.agent_id }))}
            value={agentId}
            onChange={v => { setAgentId(v); setPage(1) }}
            style={{ width: 180 }}
            allowClear
          />
          <RangePicker
            showTime={{ format: 'HH:mm' }}
            format="DD.MM.YY HH:mm"
            value={dateRange}
            onChange={v => { setDateRange(v as [dayjs.Dayjs, dayjs.Dayjs]); setPage(1) }}
            style={{ width: 320 }}
          />
          <Button onClick={reset}>Сбросить</Button>
        </Space>
      </Card>

      <Card>
        <Spin spinning={isLoading}>
          <Table
            dataSource={data?.items || []}
            columns={columns}
            rowKey={(r) => `${r.type}-${r.id}`}
            size="small"
            pagination={{
              current: page,
              pageSize: PAGE_SIZE,
              total: data?.total || 0,
              onChange: setPage,
              showTotal: (t) => `Всего: ${t}`,
            }}
            onRow={rec => ({ onClick: () => setSelected(rec), style: { cursor: 'pointer' } })}
            scroll={{ x: 900 }}
          />
        </Spin>
      </Card>

      <Drawer
        title="Детали события"
        open={!!selected}
        onClose={() => setSelected(null)}
        width={440}
      >
        {selected && (
          <Descriptions column={1} bordered size="small">
            <Descriptions.Item label="Время">{dayjs(selected.ts).format('DD.MM.YYYY HH:mm:ss')}</Descriptions.Item>
            <Descriptions.Item label="Тип"><Tag color={TYPE_COLOR[selected.type]}>{selected.type}</Tag></Descriptions.Item>
            <Descriptions.Item label="Серьёзность">
              {selected.severity ? <Tag color={SEV_COLOR[selected.severity]}>{selected.severity}</Tag> : '—'}
            </Descriptions.Item>
            <Descriptions.Item label="Хост">{selected.hostname}</Descriptions.Item>
            <Descriptions.Item label="Пользователь">{selected.username}</Descriptions.Item>
            <Descriptions.Item label="ID агента"><code style={{ fontSize: 11 }}>{selected.agent_id}</code></Descriptions.Item>
            <Descriptions.Item label="Подтип">{selected.subtype || '—'}</Descriptions.Item>
            <Descriptions.Item label="Описание">{selected.summary}</Descriptions.Item>
          </Descriptions>
        )}
      </Drawer>
    </div>
  )
}
