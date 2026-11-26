import { useState } from 'react'
import {
  Card, Table, Tag, Button, Space, Select, Input, Typography,
  Drawer, Descriptions, Switch, Modal, Form, Divider, Image, Empty, Tooltip, message, Spin,
} from 'antd'
import {
  DownloadOutlined, LockOutlined, PlusOutlined, ReloadOutlined,
  SearchOutlined, SendOutlined,
} from '@ant-design/icons'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import dayjs from 'dayjs'
import {
  fetchIncidentsPaged, fetchIncidentById, patchIncident, createManualIncident,
  fetchIncidentComments, addIncidentComment, fetchIncidentEvidence,
  fetchAgents, downloadShadowCopy, screenshotPath,
} from '../api/client'
import type { Incident, Severity, IncidentStatus } from '../api/types'
import { useAuth } from '../context/AuthContext'
import { useAuthMediaUrl } from '../hooks/useAuthMedia'

const { Title, Text } = Typography

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
const SEV_OPTIONS = ['critical', 'high', 'medium', 'low'].map((s) => ({ label: s, value: s }))
const CHANNELS = ['usb', 'clipboard', 'messenger', 'web', 'network', 'print', 'file', 'manual'].map((c) => ({ label: c, value: c }))

function EvidenceThumb({ id }: { id: number }) {
  const src = useAuthMediaUrl(screenshotPath(id))
  if (!src) return <div style={{ width: '100%', height: 80, background: 'rgba(128,128,128,0.15)', borderRadius: 4 }} />
  return <Image src={src} style={{ width: '100%', height: 80, objectFit: 'cover', borderRadius: 4 }} />
}

export default function IncidentsPage() {
  const qc = useQueryClient()
  const { user } = useAuth()
  const canEdit = user?.role === 'admin' || user?.role === 'security_officer'

  const [selected, setSelected] = useState<Incident | null>(null)
  const [search, setSearch] = useState('')
  const [statusFilter, setStatusFilter] = useState<string | null>(null)
  const [sevFilter, setSevFilter] = useState<Severity | null>(null)
  const [agentId, setAgentId] = useState<string | null>(null)
  const [onlyHold, setOnlyHold] = useState(false)
  const [page, setPage] = useState(1)
  const [createOpen, setCreateOpen] = useState(false)
  const [commentText, setCommentText] = useState('')
  const [assigneeInput, setAssigneeInput] = useState('')
  const [groupInput, setGroupInput] = useState('')
  const [createForm] = Form.useForm()
  const PAGE_SIZE = 30

  const { data: agents = [] } = useQuery({ queryKey: ['agents'], queryFn: fetchAgents })

  const params: Record<string, unknown> = { limit: PAGE_SIZE, offset: (page - 1) * PAGE_SIZE }
  if (statusFilter) params.status = statusFilter
  if (sevFilter) params.severity = sevFilter
  if (agentId) params.agent_id = agentId
  if (onlyHold) params.legal_hold = true
  if (search.trim()) params.search = search.trim()

  const { data: paged, isLoading, refetch } = useQuery({
    queryKey: ['incidents', params],
    queryFn: () => fetchIncidentsPaged(params),
    refetchInterval: 20_000,
  })

  const { data: comments = [], refetch: refetchComments } = useQuery({
    queryKey: ['incident-comments', selected?.id],
    queryFn: () => fetchIncidentComments(selected!.id),
    enabled: !!selected,
  })

  const { data: evidence } = useQuery({
    queryKey: ['incident-evidence', selected?.id],
    queryFn: () => fetchIncidentEvidence(selected!.id),
    enabled: !!selected && canEdit,
  })

  const incidents = paged?.items ?? []
  const totalCount = paged?.total ?? 0

  const invalidateAll = () => {
    qc.invalidateQueries({ queryKey: ['incidents'] })
    qc.invalidateQueries({ queryKey: ['incidents-all'] })
    qc.invalidateQueries({ queryKey: ['incidents-open-count'] })
    qc.invalidateQueries({ queryKey: ['incidents-open-paged'] })
    qc.invalidateQueries({ queryKey: ['incident-stats'] })
  }

  const patchMutation = useMutation({
    mutationFn: ({ id, body }: { id: number; body: Record<string, unknown> }) => patchIncident(id, body),
    onSuccess: (updated) => {
      message.success('Инцидент обновлён')
      setSelected((prev) => (prev && prev.id === updated.id ? updated : prev))
      invalidateAll()
    },
    onError: () => message.error('Недостаточно прав или ошибка обновления'),
  })

  const createMut = useMutation({
    mutationFn: createManualIncident,
    onSuccess: () => {
      message.success('Инцидент создан')
      setCreateOpen(false)
      createForm.resetFields()
      invalidateAll()
    },
    onError: () => message.error('Ошибка создания инцидента'),
  })

  const commentMut = useMutation({
    mutationFn: ({ id, body }: { id: number; body: string }) => addIncidentComment(id, body),
    onSuccess: () => {
      setCommentText('')
      refetchComments()
    },
    onError: () => message.error('Ошибка добавления комментария'),
  })

  const openIncident = async (rec: Incident) => {
    setSelected(rec)
    setAssigneeInput(rec.assigned_to ?? '')
    setGroupInput(rec.group_tag ?? '')
    try {
      const full = await fetchIncidentById(rec.id)
      setSelected(full)
      setAssigneeInput(full.assigned_to ?? '')
      setGroupInput(full.group_tag ?? '')
    } catch {
      // keep preview
    }
  }

  const columns = [
    {
      title: 'Время', dataIndex: 'created_at', key: 'ts', width: 135,
      render: (v: string) => dayjs(v).format('DD.MM.YY HH:mm'),
    },
    {
      title: 'Серьёзность', dataIndex: 'severity', key: 'severity', width: 105,
      render: (v: string, r: Incident) => (
        <Space size={4}>
          <Tag color={SEV_COLOR[v]}>{v}</Tag>
          {r.legal_hold && (
            <Tooltip title="Legal Hold — доказательства защищены от удаления">
              <LockOutlined style={{ color: '#faad14' }} />
            </Tooltip>
          )}
        </Space>
      ),
    },
    { title: 'Хост', dataIndex: 'hostname', key: 'hostname', width: 120 },
    { title: 'Пользователь', dataIndex: 'username', key: 'username', width: 120 },
    { title: 'Канал', dataIndex: 'channel', key: 'channel', width: 95 },
    {
      title: 'Ответственный', dataIndex: 'assigned_to', key: 'assigned_to', width: 120,
      render: (v: string | null) => (v ? <Tag color="blue">{v}</Tag> : <Text type="secondary">—</Text>),
    },
    {
      title: 'Статус', dataIndex: 'status', key: 'status', width: 160,
      render: (v: string, rec: Incident) => (
        <Select
          value={v}
          size="small"
          disabled={!canEdit}
          style={{ width: 150 }}
          options={STATUS_OPTIONS}
          onChange={(val) => patchMutation.mutate({ id: rec.id, body: { status: val } })}
          onClick={(e) => e.stopPropagation()}
        />
      ),
    },
    {
      title: 'Правила / Группа', dataIndex: 'matched_rules', key: 'rules',
      render: (rules: string[], r: Incident) => (
        <Space size={2} wrap>
          {r.group_tag && <Tag color="purple" style={{ fontSize: 10 }}>{r.group_tag}</Tag>}
          {(rules || []).slice(0, 2).map((rule) => <Tag key={rule} style={{ fontSize: 10 }}>{rule}</Tag>)}
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
        <Title level={4} style={{ margin: 0 }}>Консоль управления инцидентами</Title>
        <Space>
          {canEdit && (
            <Button type="primary" icon={<PlusOutlined />} onClick={() => setCreateOpen(true)}>
              Создать инцидент
            </Button>
          )}
          <Button icon={<ReloadOutlined />} onClick={() => refetch()}>Обновить</Button>
        </Space>
      </div>

      <Card style={{ marginBottom: 16 }} styles={{ body: { padding: '12px 16px' } }}>
        <Space wrap size={8}>
          <Input
            placeholder="Поиск (пользователь, группа, сниппет)..."
            prefix={<SearchOutlined />}
            value={search}
            onChange={(e) => { setSearch(e.target.value); setPage(1) }}
            style={{ width: 240 }}
            allowClear
          />
          <Select
            placeholder="Статус"
            options={STATUS_OPTIONS}
            value={statusFilter}
            onChange={(v) => { setStatusFilter(v); setPage(1) }}
            style={{ width: 165 }}
            allowClear
          />
          <Select
            placeholder="Серьёзность"
            options={SEV_OPTIONS}
            value={sevFilter}
            onChange={(v) => { setSevFilter(v); setPage(1) }}
            style={{ width: 125 }}
            allowClear
          />
          <Select
            placeholder="Агент"
            options={agents.map((a) => ({ label: a.hostname, value: a.agent_id }))}
            value={agentId}
            onChange={(v) => { setAgentId(v); setPage(1) }}
            style={{ width: 170 }}
            allowClear
          />
          <Switch checked={onlyHold} onChange={(v) => { setOnlyHold(v); setPage(1) }} size="small" />
          <Text style={{ fontSize: 12 }}><LockOutlined /> Только Legal Hold</Text>
          <Button onClick={() => { setSearch(''); setStatusFilter(null); setSevFilter(null); setAgentId(null); setOnlyHold(false); setPage(1) }}>
            Сбросить
          </Button>
        </Space>
      </Card>

      <Card>
        <Spin spinning={isLoading}>
          <Table
            dataSource={incidents}
            columns={columns}
            rowKey="id"
            size="small"
            pagination={{ current: page, pageSize: PAGE_SIZE, total: totalCount, onChange: setPage, showTotal: (t) => `Всего: ${t}` }}
            onRow={(rec) => ({ onClick: () => openIncident(rec), style: { cursor: 'pointer' } })}
            scroll={{ x: 1100 }}
          />
        </Spin>
      </Card>

      <Drawer
        title={
          <Space>
            <span>Инцидент #{selected?.id}</span>
            {selected?.legal_hold && <Tag color="gold" icon={<LockOutlined />}>Legal Hold</Tag>}
          </Space>
        }
        open={!!selected}
        onClose={() => setSelected(null)}
        width={620}
        extra={
          selected && canEdit && (
            <Space>
              <Button size="small" onClick={() => patchMutation.mutate({ id: selected.id, body: { status: 'resolved' as IncidentStatus } })}>
                Закрыть
              </Button>
              <Button size="small" onClick={() => patchMutation.mutate({ id: selected.id, body: { status: 'false_positive' as IncidentStatus } })}>
                Ложное
              </Button>
            </Space>
          )
        }
      >
        {selected && (
          <>
            <Descriptions column={1} bordered size="small" style={{ marginBottom: 16 }}>
              <Descriptions.Item label="Время">{selected.created_at ? dayjs(selected.created_at).format('DD.MM.YYYY HH:mm:ss') : '—'}</Descriptions.Item>
              <Descriptions.Item label="Серьёзность"><Tag color={SEV_COLOR[selected.severity]}>{selected.severity}</Tag></Descriptions.Item>
              <Descriptions.Item label="Статус"><Tag color={STATUS_COLOR[selected.status]}>{STATUS_LABELS[selected.status]}</Tag></Descriptions.Item>
              <Descriptions.Item label="Хост / Пользователь">{selected.hostname} / <b>{selected.username}</b></Descriptions.Item>
              <Descriptions.Item label="Канал">{selected.channel}</Descriptions.Item>
              <Descriptions.Item label="Правила">{(selected.matched_rules || []).map((r) => <Tag key={r}>{r}</Tag>)}</Descriptions.Item>
              {canEdit && (
                <Descriptions.Item label="Legal Hold (защита от удаления)">
                  <Space>
                    <Switch
                      checked={!!selected.legal_hold}
                      onChange={(checked) => patchMutation.mutate({ id: selected.id, body: { legal_hold: checked } })}
                    />
                    <Text type="secondary" style={{ fontSize: 12 }}>
                      Запрещает удаление доказательств (вкл. автоочистку)
                    </Text>
                  </Space>
                </Descriptions.Item>
              )}
              {canEdit && (
                <Descriptions.Item label="Ответственный / Группа">
                  <Space.Compact style={{ width: '100%' }}>
                    <Input
                      size="small"
                      placeholder="Ответственный офицер"
                      value={assigneeInput}
                      onChange={(e) => setAssigneeInput(e.target.value)}
                    />
                    <Input
                      size="small"
                      placeholder="Группа (напр. Финансы)"
                      value={groupInput}
                      onChange={(e) => setGroupInput(e.target.value)}
                    />
                    <Button
                      size="small"
                      type="primary"
                      onClick={() =>
                        patchMutation.mutate({
                          id: selected.id,
                          body: { assigned_to: assigneeInput, group_tag: groupInput },
                        })
                      }
                    >
                      OK
                    </Button>
                  </Space.Compact>
                </Descriptions.Item>
              )}
            </Descriptions>

            {selected.snippet && (
              <Card size="small" title="Фрагмент данных" style={{ marginBottom: 16 }}>
                <pre style={{ fontSize: 12, whiteSpace: 'pre-wrap', wordBreak: 'break-all', margin: 0 }}>
                  {selected.snippet}
                </pre>
              </Card>
            )}

            {canEdit && evidence && (
              <>
                <Divider orientation="left" style={{ fontSize: 12 }}>Доказательная база</Divider>
                {evidence.shadow_copies.length > 0 && (
                  <Card size="small" title="Связанные теневые копии" style={{ marginBottom: 12 }}>
                    <Space direction="vertical" style={{ width: '100%' }}>
                      {evidence.shadow_copies.map((s) => (
                        <div key={s.id} style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
                          <Text style={{ fontSize: 12 }}>{s.file_name} <Text type="secondary">({s.sha256.slice(0, 10)}…)</Text></Text>
                          <Button size="small" icon={<DownloadOutlined />} onClick={() => downloadShadowCopy(s.id, s.file_name)}>
                            Скачать
                          </Button>
                        </div>
                      ))}
                    </Space>
                  </Card>
                )}
                {evidence.screenshots.length > 0 ? (
                  <Image.PreviewGroup>
                    <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr 1fr', gap: 8, marginBottom: 16 }}>
                      {evidence.screenshots.map((sc) => (
                        <div key={sc.id}>
                          <EvidenceThumb id={sc.id} />
                          <Text type="secondary" style={{ fontSize: 10 }}>{dayjs(sc.created_at).format('HH:mm:ss')}</Text>
                        </div>
                      ))}
                    </div>
                  </Image.PreviewGroup>
                ) : (
                  <Text type="secondary" style={{ fontSize: 12 }}>Скриншоты в окне ±5 мин не найдены</Text>
                )}
              </>
            )}

            <Divider orientation="left" style={{ fontSize: 12 }}>Журнал расследования ({comments.length})</Divider>
            {comments.length === 0 ? (
              <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description="Комментариев пока нет" />
            ) : (
              <Space direction="vertical" style={{ width: '100%', marginBottom: 12 }}>
                {comments.map((c) => (
                  <Card key={c.id} size="small" styles={{ body: { padding: '8px 12px' } }}>
                    <div style={{ display: 'flex', justifyContent: 'space-between', marginBottom: 4 }}>
                      <Text strong style={{ fontSize: 12 }}>{c.author} <Tag style={{ fontSize: 10 }}>{c.author_role}</Tag></Text>
                      <Text type="secondary" style={{ fontSize: 11 }}>{dayjs(c.created_at).format('DD.MM.YY HH:mm')}</Text>
                    </div>
                    <Text style={{ fontSize: 13 }}>{c.body}</Text>
                  </Card>
                ))}
              </Space>
            )}
            {canEdit && (
              <Space.Compact style={{ width: '100%' }}>
                <Input
                  placeholder="Добавить комментарий расследования..."
                  value={commentText}
                  onChange={(e) => setCommentText(e.target.value)}
                  onPressEnter={() => commentText.trim() && commentMut.mutate({ id: selected.id, body: commentText })}
                />
                <Button
                  type="primary"
                  icon={<SendOutlined />}
                  loading={commentMut.isPending}
                  onClick={() => commentText.trim() && commentMut.mutate({ id: selected.id, body: commentText })}
                >
                  Отправить
                </Button>
              </Space.Compact>
            )}
          </>
        )}
      </Drawer>

      <Modal
        title="Создать инцидент вручную"
        open={createOpen}
        onCancel={() => setCreateOpen(false)}
        onOk={() => createForm.submit()}
        confirmLoading={createMut.isPending}
        okText="Создать"
        cancelText="Отмена"
      >
        <Form form={createForm} layout="vertical" onFinish={(v) => createMut.mutate(v)}>
          <Form.Item name="username" label="Сотрудник (username)" rules={[{ required: true }]}>
            <Input placeholder="ivanov_a" />
          </Form.Item>
          <Form.Item name="channel" label="Канал" initialValue="manual" rules={[{ required: true }]}>
            <Select options={CHANNELS} />
          </Form.Item>
          <Form.Item name="severity" label="Серьёзность" initialValue="high" rules={[{ required: true }]}>
            <Select options={SEV_OPTIONS} />
          </Form.Item>
          <Form.Item name="group_tag" label="Группа / Категория">
            <Input placeholder="Например: Утечка ПДн" />
          </Form.Item>
          <Form.Item name="assigned_to" label="Ответственный офицер">
            <Input placeholder={user?.username} />
          </Form.Item>
          <Form.Item name="snippet" label="Описание / Фрагмент" rules={[{ required: true }]}>
            <Input.TextArea rows={3} />
          </Form.Item>
          <Form.Item name="legal_hold" label="Поставить на Legal Hold (защита от удаления)" valuePropName="checked">
            <Switch />
          </Form.Item>
        </Form>
      </Modal>
    </div>
  )
}
