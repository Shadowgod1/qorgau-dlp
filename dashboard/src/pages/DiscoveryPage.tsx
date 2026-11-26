import { useState } from 'react'
import {
  Button, Card, Input, Select, Space, Table, Tag, Tooltip, Typography, message,
} from 'antd'
import { FileSearchOutlined, PlayCircleOutlined, ReloadOutlined, SearchOutlined } from '@ant-design/icons'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import dayjs from 'dayjs'
import {
  fetchAgents, fetchDiscoveryFindings, patchDiscoveryStatus, triggerDiscoveryScan,
  type DiscoveryFinding,
} from '../api/client'
import { useAuth } from '../context/AuthContext'

const { Title, Text } = Typography

const SEV_COLOR: Record<string, string> = { critical: 'red', high: 'orange', medium: 'gold', low: 'green' }
const STATUS_OPTS = [
  { value: 'open', label: 'Открыто' },
  { value: 'resolved', label: 'Устранено' },
  { value: 'ignored', label: 'Исключение' },
]

export default function DiscoveryPage() {
  const qc = useQueryClient()
  const { user } = useAuth()
  const canEdit = user?.role === 'admin' || user?.role === 'security_officer'
  const [search, setSearch] = useState('')
  const [statusFilter, setStatusFilter] = useState<string | undefined>()

  const { data: agents = [] } = useQuery({ queryKey: ['agents'], queryFn: fetchAgents })
  const { data: findings = [], isLoading, refetch } = useQuery({
    queryKey: ['discovery', statusFilter],
    queryFn: () => fetchDiscoveryFindings(statusFilter ? { status: statusFilter } : {}),
  })

  const scanMut = useMutation({
    mutationFn: (agentId?: string) => triggerDiscoveryScan(agentId),
    onSuccess: () => message.success('Команда сканирования дисков отправлена агентам'),
    onError: () => message.error('Ошибка запуска сканирования'),
  })

  const patchMut = useMutation({
    mutationFn: ({ id, status }: { id: number; status: string }) => patchDiscoveryStatus(id, status),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['discovery'] }),
  })

  const filtered = findings.filter((f) => {
    if (!search.trim()) return true
    const q = search.toLowerCase()
    return (
      f.file_path.toLowerCase().includes(q) ||
      f.username.toLowerCase().includes(q) ||
      f.hostname.toLowerCase().includes(q)
    )
  })

  const columns = [
    {
      title: 'Обнаружено', dataIndex: 'scanned_at', key: 'ts', width: 135,
      render: (v: string) => dayjs(v).format('DD.MM.YY HH:mm'),
    },
    {
      title: 'Риск', dataIndex: 'severity', key: 'sev', width: 95,
      render: (v: string) => <Tag color={SEV_COLOR[v]}>{v}</Tag>,
    },
    {
      title: 'Хост / Пользователь', key: 'user', width: 160,
      render: (_: unknown, r: DiscoveryFinding) => (
        <Space direction="vertical" size={0}>
          <Text strong style={{ fontSize: 12 }}>{r.hostname}</Text>
          <Text type="secondary" style={{ fontSize: 11 }}>{r.username}</Text>
        </Space>
      ),
    },
    {
      title: 'Путь к файлу', dataIndex: 'file_path', key: 'path', ellipsis: true,
      render: (v: string) => <Tooltip title={v}><code style={{ fontSize: 11 }}>{v}</code></Tooltip>,
    },
    {
      title: 'Найденные ПДн / Правила', dataIndex: 'matched_rules', key: 'rules',
      render: (rules: string[]) => (
        <Space wrap size={2}>
          {(rules || []).map((r) => <Tag key={r} color="volcano" style={{ fontSize: 10 }}>{r}</Tag>)}
        </Space>
      ),
    },
    {
      title: 'Статус', dataIndex: 'status', key: 'status', width: 140,
      render: (v: string, rec: DiscoveryFinding) =>
        canEdit ? (
          <Select
            size="small"
            value={v}
            style={{ width: 130 }}
            options={STATUS_OPTS}
            onChange={(st) => patchMut.mutate({ id: rec.id, status: st })}
          />
        ) : (
          <Tag>{v}</Tag>
        ),
    },
  ]

  return (
    <div>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 16, flexWrap: 'wrap', gap: 12 }}>
        <div>
          <Title level={4} style={{ margin: 0 }}>
            <FileSearchOutlined style={{ marginRight: 8 }} />
            Краулер ПДн (Data Discovery на дисках ПК)
          </Title>
          <Text type="secondary" style={{ fontSize: 13 }}>
            Поиск неконтролируемых файлов с ИИН/БИН, банковскими картами, IBAN и грифами ДСП на рабочих станциях
          </Text>
        </div>
        <Space wrap>
          {canEdit && (
            <>
              <Select
                placeholder="Сканировать выбранный ПК"
                style={{ width: 220 }}
                value={undefined}
                onSelect={(aid: string | undefined) => { if (aid) scanMut.mutate(aid) }}
                options={agents.filter((a) => a.is_online).map((a) => ({ value: a.agent_id, label: `🔍 ${a.hostname}` }))}
              />
              <Button
                type="primary"
                icon={<PlayCircleOutlined />}
                loading={scanMut.isPending}
                onClick={() => scanMut.mutate(undefined)}
              >
                Сканировать все онлайн ПК
              </Button>
            </>
          )}
          <Button icon={<ReloadOutlined />} onClick={() => refetch()}>Обновить</Button>
        </Space>
      </div>

      <Card style={{ marginBottom: 16 }} styles={{ body: { padding: '12px 16px' } }}>
        <Space wrap>
          <Input
            placeholder="Поиск по пути, хосту или пользователю..."
            prefix={<SearchOutlined />}
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            style={{ width: 300 }}
            allowClear
          />
          <Select
            placeholder="Статус"
            allowClear
            style={{ width: 150 }}
            value={statusFilter}
            onChange={setStatusFilter}
            options={STATUS_OPTS}
          />
        </Space>
      </Card>

      <Card>
        <Table
          dataSource={filtered}
          columns={columns}
          rowKey="id"
          loading={isLoading}
          size="small"
          scroll={{ x: 1000 }}
          pagination={{ pageSize: 25, showTotal: (t) => `Найдено файлов: ${t}` }}
        />
      </Card>
    </div>
  )
}
