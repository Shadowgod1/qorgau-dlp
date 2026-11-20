import { useState } from 'react'
import {
  Card, Table, Tag, Button, Space, Select, Input, Typography,
  Drawer, Descriptions, Popconfirm, message, Spin,
} from 'antd'
import { ReloadOutlined, SearchOutlined } from '@ant-design/icons'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import dayjs from 'dayjs'
import { fetchIncidentsPaged, fetchIncidentById, patchIncident, fetchAgents } from '../api/client'
import type { Incident, Severity, IncidentStatus } from '../api/types'
import { useAuth } from '../context/AuthContext'

const { Title } = Typography

const SEV_COLOR: Record<string, string> = {
  critical: 'red', high: 'orange', medium: 'gold', low: 'green',
}

const STATUS_COLOR: Record<string, string> = {
  open: 'red', reviewed: 'orange', resolved: 'green', false_positive: 'default',
}

const STATUS_LABELS: Record<string, string> = {
  open: 'Открыт', reviewed: 'Просмотрен', resolved: 'Закрыт', false_positive: 'Ложное срабатывание',
}

const STATUS_OPTIONS = Object.entries(STATUS_LABELS).map(([k, v]) => ({ label: v, value: k }))
const SEV_OPTIONS = ['critical','high','medium','low'].map(s => ({ label: s, value: s }))

export default function IncidentsPage() {
  const qc = useQueryClient()
  const { user } = useAuth()
  const canEdit = user?.role === 'admin' || user?.role === 'security_officer'
  const [selected, setSelected] = useState<Incident | null>(null)
  const [search, setSearch] = useState('')
  const [statusFilter, setStatusFilter] = useState<string | null>(null)
  const [sevFilter, setSevFilter] = useState<Severity | null>(null)
  const [agentId, setAgentId] = useState<string | null>(null)
  const [page, setPage] = useState(1)
  const PAGE_SIZE = 30

  const { data: agents = [] } = useQuery({ queryKey: ['agents'], queryFn: fetchAgents })

  const params: Record<string, unknown> = { limit: PAGE_SIZE, offset: (page - 1) * PAGE_SIZE }
  if (statusFilter) params.status = statusFilter
  if (sevFilter) params.severity = sevFilter
  if (agentId) params.agent_id = agentId
  if (search.trim()) params.search = search.trim()

  const { data: paged, isLoading, refetch } = useQuery({
    queryKey: ['incidents', params],
    queryFn: () => fetchIncidentsPaged(params),
    refetchInterval: 20_000,
  })

  const incidents = paged?.items ?? []
  const totalCount = paged?.total ?? 0

  const patchMutation = useMutation({
    mutationFn: ({ id, status }: { id: number; status: string }) => patchIncident(id, status),
    onSuccess: (_res, vars) => {
      message.success('Статус обновлён')
      setSelected(prev => prev && prev.id === vars.id ? { ...prev, status: vars.status as IncidentStatus } : prev)
      qc.invalidateQueries({ queryKey: ['incidents'] })
      qc.invalidateQueries({ queryKey: ['incidents-all'] })
      qc.invalidateQueries({ queryKey: ['incidents-open-count'] })
      qc.invalidateQueries({ queryKey: ['incidents-open-paged'] })
      qc.invalidateQueries({ queryKey: ['incident-stats'] })
    },
    onError: () => message.error('Недостаточно прав или ошибка обновления статуса'),
  })

  const openIncident = async (rec: Incident) => {
    setSelected(rec)
    try {
      const full = await fetchIncidentById(rec.id)
      setSelected(full)
    } catch {
      // keep row preview
    }
  }

  const filtered = incidents

  const columns = [
    {
      title: 'Время', dataIndex: 'created_at', key: 'ts', width: 140,
      render: (v: string) => dayjs(v).format('DD.MM.YY HH:mm'),
      sorter: (a: Incident, b: Incident) => dayjs(a.created_at!).valueOf() - dayjs(b.created_at!).valueOf(),
      defaultSortOrder: 'descend' as const,
    },
    {
      title: 'Серьёзность', dataIndex: 'severity', key: 'severity', width: 110,
      render: (v: string) => <Tag color={SEV_COLOR[v]}>{v}</Tag>,
      filters: ['critical','high','medium','low'].map(s => ({ text: s, value: s })),
      onFilter: (v: unknown, r: Incident) => r.severity === v,
    },
    { title: 'Хост', dataIndex: 'hostname', key: 'hostname', width: 140 },
    { title: 'Пользователь', dataIndex: 'username', key: 'username', width: 120 },
    { title: 'Канал', dataIndex: 'channel', key: 'channel', width: 100 },
    {
      title: 'Статус', dataIndex: 'status', key: 'status', width: 160,
      render: (v: string, rec: Incident) => (
        <Select
          value={v}
          size="small"
          disabled={!canEdit}
          style={{ width: 155 }}
          options={STATUS_OPTIONS}
          onChange={(val) => patchMutation.mutate({ id: rec.id, status: val })}
          onClick={e => e.stopPropagation()}
        />
      ),
    },
    {
      title: 'Правила', dataIndex: 'matched_rules', key: 'rules',
      render: (rules: string[]) => (
        <Space size={2} wrap>
          {(rules || []).slice(0, 2).map(r => <Tag key={r} style={{ fontSize: 10 }}>{r}</Tag>)}
          {rules?.length > 2 && <Tag>+{rules.length - 2}</Tag>}
        </Space>
      ),
    },
    {
      title: 'Сниппет', dataIndex: 'snippet', key: 'snippet', ellipsis: true,
      render: (v: string) => <span style={{ fontSize: 12, color: '#888' }}>{v?.slice(0, 60)}</span>,
    },
  ]

  return (
    <div>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 16 }}>
        <Title level={4} style={{ margin: 0 }}>Инциденты</Title>
        <Button icon={<ReloadOutlined />} onClick={() => refetch()}>Обновить</Button>
      </div>

      {/* Filters */}
      <Card style={{ marginBottom: 16 }} styles={{ body: { padding: '12px 16px' } }}>
        <Space wrap size={8}>
          <Input
            placeholder="Поиск..."
            prefix={<SearchOutlined />}
            value={search}
            onChange={e => setSearch(e.target.value)}
            style={{ width: 200 }}
            allowClear
          />
          <Select
            placeholder="Статус"
            options={STATUS_OPTIONS}
            value={statusFilter}
            onChange={v => { setStatusFilter(v); setPage(1) }}
            style={{ width: 170 }}
            allowClear
          />
          <Select
            placeholder="Серьёзность"
            options={SEV_OPTIONS}
            value={sevFilter}
            onChange={v => { setSevFilter(v); setPage(1) }}
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
          <Button onClick={() => { setSearch(''); setStatusFilter(null); setSevFilter(null); setAgentId(null); setPage(1) }}>
            Сбросить
          </Button>
        </Space>
      </Card>

      <Card>
        <Spin spinning={isLoading}>
          <Table
            dataSource={filtered}
            columns={columns}
            rowKey="id"
            size="small"
            pagination={{ current: page, pageSize: PAGE_SIZE, total: totalCount, onChange: setPage, showTotal: t => `Всего: ${t}` }}
            onRow={rec => ({ onClick: () => openIncident(rec), style: { cursor: 'pointer' } })}
            scroll={{ x: 1000 }}
            rowClassName={rec => rec.severity === 'critical' ? 'critical-row' : ''}
          />
        </Spin>
      </Card>

      <Drawer
        title={`Инцидент #${selected?.id}`}
        open={!!selected}
        onClose={() => setSelected(null)}
        width={500}
        extra={
          selected && canEdit && (
            <Space>
              <Button size="small" onClick={() => patchMutation.mutate({ id: selected.id, status: 'resolved' })}>
                Закрыть
              </Button>
              <Button size="small" onClick={() => patchMutation.mutate({ id: selected.id, status: 'false_positive' })}>
                Ложное срабатывание
              </Button>
            </Space>
          )
        }
      >
        {selected && (
          <>
            <Descriptions column={1} bordered size="small" style={{ marginBottom: 16 }}>
              <Descriptions.Item label="Время">{dayjs(selected.created_at!).format('DD.MM.YYYY HH:mm:ss')}</Descriptions.Item>
              <Descriptions.Item label="Серьёзность">
                <Tag color={SEV_COLOR[selected.severity]}>{selected.severity}</Tag>
              </Descriptions.Item>
              <Descriptions.Item label="Статус">
                <Tag color={STATUS_COLOR[selected.status]}>{STATUS_LABELS[selected.status]}</Tag>
              </Descriptions.Item>
              <Descriptions.Item label="Хост">{selected.hostname}</Descriptions.Item>
              <Descriptions.Item label="Пользователь">{selected.username}</Descriptions.Item>
              <Descriptions.Item label="ID агента"><code style={{ fontSize: 11 }}>{selected.agent_id}</code></Descriptions.Item>
              <Descriptions.Item label="Канал">{selected.channel}</Descriptions.Item>
              <Descriptions.Item label="Правила">
                {(selected.matched_rules || []).map(r => <Tag key={r}>{r}</Tag>)}
              </Descriptions.Item>
            </Descriptions>
            {selected.snippet && (
              <Card size="small" title="Фрагмент данных" styles={{ body: { background: '#141414' } }}>
                <pre style={{ fontSize: 12, whiteSpace: 'pre-wrap', wordBreak: 'break-all', margin: 0 }}>
                  {selected.snippet}
                </pre>
              </Card>
            )}
          </>
        )}
      </Drawer>
    </div>
  )
}
