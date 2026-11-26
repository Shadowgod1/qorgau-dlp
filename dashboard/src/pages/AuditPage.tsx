import { useState } from 'react'
import { Alert, Button, Card, Input, Space, Table, Tag, Typography } from 'antd'
import { CheckCircleOutlined, ReloadOutlined, SafetyCertificateOutlined, SearchOutlined, WarningOutlined } from '@ant-design/icons'
import { useMutation, useQuery } from '@tanstack/react-query'
import dayjs from 'dayjs'
import { fetchAuditLogs, verifyAuditChain, type AuditEntry, type AuditVerifyResult } from '../api/client'

const { Title, Text } = Typography

export default function AuditPage() {
  const [actor, setActor] = useState('')
  const [action, setAction] = useState('')
  const [page, setPage] = useState(1)
  const [verifyRes, setVerifyRes] = useState<AuditVerifyResult | null>(null)
  const PAGE_SIZE = 50

  const params: Record<string, unknown> = { limit: PAGE_SIZE, offset: (page - 1) * PAGE_SIZE }
  if (actor.trim()) params.actor = actor.trim()
  if (action.trim()) params.action = action.trim()

  const { data, isLoading, refetch } = useQuery({
    queryKey: ['audit-logs', params],
    queryFn: () => fetchAuditLogs(params),
  })

  const verifyMut = useMutation({
    mutationFn: verifyAuditChain,
    onSuccess: (res) => setVerifyRes(res),
  })

  const columns = [
    { title: 'ID', dataIndex: 'id', key: 'id', width: 70 },
    {
      title: 'Время', dataIndex: 'ts', key: 'ts', width: 160,
      render: (v: string) => dayjs(v).format('DD.MM.YYYY HH:mm:ss'),
    },
    {
      title: 'Актор', dataIndex: 'actor', key: 'actor', width: 130,
      render: (v: string, r: AuditEntry) => (
        <Space size={4}>
          <Text strong>{v}</Text>
          {r.actor_role && <Tag style={{ fontSize: 10 }}>{r.actor_role}</Tag>}
        </Space>
      ),
    },
    {
      title: 'Действие', dataIndex: 'action', key: 'action', width: 190,
      render: (v: string) => <Tag color={v.includes('failed') || v.includes('deleted') ? 'red' : 'blue'}>{v}</Tag>,
    },
    { title: 'Ресурс', dataIndex: 'resource', key: 'resource', width: 160 },
    {
      title: 'Детали', dataIndex: 'details', key: 'details', ellipsis: true,
      render: (v: Record<string, unknown>) => <code style={{ fontSize: 11 }}>{JSON.stringify(v)}</code>,
    },
    {
      title: 'SHA-256', dataIndex: 'entry_hash', key: 'hash', width: 120,
      render: (v: string) => <code style={{ fontSize: 11 }}>{v.slice(0, 12)}…</code>,
    },
  ]

  return (
    <div>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 16, flexWrap: 'wrap', gap: 12 }}>
        <div>
          <Title level={4} style={{ margin: 0 }}>Журнал аудита (Приказ МЦРИАП РК №410)</Title>
          <Text type="secondary" style={{ fontSize: 13 }}>
            Неизменяемая криптографическая хэш-цепочка действий операторов и системы
          </Text>
        </div>
        <Space>
          <Button
            type="primary"
            icon={<SafetyCertificateOutlined />}
            loading={verifyMut.isPending}
            onClick={() => verifyMut.mutate()}
          >
            Проверить целостность цепочки
          </Button>
          <Button icon={<ReloadOutlined />} onClick={() => refetch()}>Обновить</Button>
        </Space>
      </div>

      {verifyRes && (
        <Alert
          style={{ marginBottom: 16 }}
          type={verifyRes.valid ? 'success' : 'error'}
          showIcon
          icon={verifyRes.valid ? <CheckCircleOutlined /> : <WarningOutlined />}
          message={verifyRes.valid ? 'Хэш-цепочка аудита целостна' : 'Обнаружено нарушение целостности!'}
          description={verifyRes.message}
          closable
          onClose={() => setVerifyRes(null)}
        />
      )}

      <Card style={{ marginBottom: 16 }} styles={{ body: { padding: '12px 16px' } }}>
        <Space wrap>
          <Input
            placeholder="Актор (username)"
            prefix={<SearchOutlined />}
            value={actor}
            onChange={(e) => { setActor(e.target.value); setPage(1) }}
            style={{ width: 200 }}
            allowClear
          />
          <Input
            placeholder="Действие (login_success...)"
            value={action}
            onChange={(e) => { setAction(e.target.value); setPage(1) }}
            style={{ width: 240 }}
            allowClear
          />
          <Button onClick={() => { setActor(''); setAction(''); setPage(1) }}>Сбросить</Button>
        </Space>
      </Card>

      <Card>
        <Table
          dataSource={data?.items ?? []}
          columns={columns}
          rowKey="id"
          loading={isLoading}
          size="small"
          scroll={{ x: 1000 }}
          pagination={{
            current: page,
            pageSize: PAGE_SIZE,
            total: data?.total ?? 0,
            onChange: setPage,
            showTotal: (t) => `Всего записей: ${t}`,
          }}
        />
      </Card>
    </div>
  )
}
