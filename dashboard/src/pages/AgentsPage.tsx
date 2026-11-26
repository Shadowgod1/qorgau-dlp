import { useMemo, useState } from 'react'
import {
  Table, Tag, Button, Drawer, Descriptions, Typography, Select, Modal,
  Space, Popconfirm, message, Input, Card, Tooltip, Divider, Alert,
} from 'antd'
import {
  ReloadOutlined, DeleteOutlined, CameraOutlined, SearchOutlined,
  DownloadOutlined, ApartmentOutlined, FileSearchOutlined, DesktopOutlined,
  CodeOutlined, CopyOutlined, LaptopOutlined, CheckCircleOutlined, StopOutlined,
} from '@ant-design/icons'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import dayjs from 'dayjs'
import {
  fetchAgents, deleteAgent, triggerScreenshot, fetchConfigs, assignConfig,
  unassignConfig, triggerDiscoveryScan, triggerInventoryScan, downloadWindowsAgent,
  AgentConfig,
} from '../api/client'
import type { Agent } from '../api/types'
import { useAuth } from '../context/AuthContext'

const { Title, Text } = Typography

const isOnline = (agent: { last_seen: string | null }) =>
  !!agent.last_seen && dayjs().diff(dayjs(agent.last_seen), 'minute') < 3

export default function AgentsPage() {
  const qc = useQueryClient()
  const { user } = useAuth()
  const isAdmin = user?.role === 'admin'
  const isOfficer = user?.role === 'admin' || user?.role === 'security_officer'

  const [selected, setSelected] = useState<Agent | null>(null)
  const [search, setSearch] = useState('')
  const [statusFilter, setStatusFilter] = useState<'all' | 'online' | 'offline'>('all')
  const [configFilter, setConfigFilter] = useState<number | 'none' | undefined>(undefined)
  const [selectedRowKeys, setSelectedRowKeys] = useState<string[]>([])
  const [bulkConfigId, setBulkConfigId] = useState<number | undefined>(undefined)
  const [deployOpen, setDeployOpen] = useState(false)

  const { data: agents = [], isLoading, refetch } = useQuery({
    queryKey: ['agents'],
    queryFn: fetchAgents,
    refetchInterval: 15_000,
  })

  const { data: configs = [] } = useQuery({
    queryKey: ['configs'],
    queryFn: fetchConfigs,
    enabled: isAdmin,
  })

  const configMap = useMemo(() => {
    const m = new Map<number, AgentConfig>()
    configs.forEach(c => { if (c.id) m.set(c.id, c) })
    return m
  }, [configs])

  const delMutation = useMutation({
    mutationFn: (agentId: string) => deleteAgent(agentId),
    onSuccess: () => {
      message.success('Агент удалён')
      qc.invalidateQueries({ queryKey: ['agents'] })
      setSelected(null)
    },
  })

  const assignMutation = useMutation({
    mutationFn: async ({ agentIds, configId, previousConfigId }: {
      agentIds: string[]
      configId: number | null
      previousConfigId?: number | null
    }) => {
      if (configId === null) {
        if (previousConfigId) {
          await unassignConfig(previousConfigId, agentIds)
        }
      } else {
        await assignConfig(configId, agentIds)
      }
    },
    onSuccess: () => {
      message.success('Конфигурация применена — агент обновится по WebSocket')
      qc.invalidateQueries({ queryKey: ['agents'] })
      qc.invalidateQueries({ queryKey: ['configs'] })
      setSelectedRowKeys([])
    },
    onError: () => message.error('Не удалось назначить конфигурацию'),
  })

  const handleScreenshot = async (agentId: string) => {
    try {
      await triggerScreenshot(agentId)
      message.success('Команда на скриншот отправлена')
    } catch {
      message.error('Агент оффлайн или ошибка отправки')
    }
  }

  const handleDiscovery = async (agentId: string) => {
    try {
      await triggerDiscoveryScan(agentId)
      message.success('Краулер ПДн запущен на ПК')
    } catch {
      message.error('Ошибка запуска краулера')
    }
  }

  const handleInventory = async (agentId: string) => {
    try {
      await triggerInventoryScan(agentId)
      message.success('Запрос инвентаризации отправлен')
    } catch {
      message.error('Ошибка запроса инвентаризации')
    }
  }

  const filtered = agents.filter(a => {
    const online = isOnline(a)
    if (statusFilter === 'online' && !online) return false
    if (statusFilter === 'offline' && online) return false
    if (configFilter === 'none' && a.config_id) return false
    if (typeof configFilter === 'number' && a.config_id !== configFilter) return false
    if (!search) return true
    const q = search.toLowerCase()
    return (
      a.hostname.toLowerCase().includes(q) ||
      a.username.toLowerCase().includes(q) ||
      (a.ip_address || '').includes(q) ||
      a.agent_id.toLowerCase().includes(q)
    )
  })

  const onlineCount = agents.filter(isOnline).length
  const configuredCount = agents.filter(a => !!a.config_id).length

  const serverUrl = typeof window !== 'undefined' ? window.location.origin : 'http://SERVER_IP'
  const installCmd = `# 1. Сохраните qorgau-agent.exe в C:\\ProgramData\\QorgauDLP\\
$env:QORGAU_SERVER_URL = "${serverUrl}"
$env:QORGAU_AGENT_TOKEN = "<ВСТАВЬТЕ_AGENT_TOKEN>"
# Разовый тест в консоли:
.\\qorgau-agent.exe
# Установка как служба Windows (от Администратора):
sc.exe create QorgauAgent binPath= "C:\\ProgramData\\QorgauDLP\\qorgau-agent.exe --service --server ${serverUrl} --token <AGENT_TOKEN>" start= auto DisplayName= "Qorgau DLP Endpoint Agent"
sc.exe start QorgauAgent`

  const columns = [
    {
      title: 'Статус',
      key: 'status',
      width: 98,
      render: (_: unknown, rec: Agent) => {
        const online = isOnline(rec)
        return (
          <Space size={7}>
            <span className="ts-dot" style={{ background: online ? '#10b981' : '#64748b' }} />
            <span style={{ fontSize: 13 }}>{online ? 'Онлайн' : 'Офлайн'}</span>
          </Space>
        )
      },
    },
    {
      title: 'Хост / ID',
      key: 'hostname',
      render: (_: unknown, rec: Agent) => (
        <div>
          <div style={{ fontWeight: 500 }}>{rec.hostname || '—'}</div>
          <div className="qorgau-mono" style={{ fontSize: 11.5, color: 'var(--p-muted)' }}>{rec.agent_id}</div>
        </div>
      ),
    },
    {
      title: 'Пользователь',
      dataIndex: 'username',
      key: 'username',
      render: (v: string) => v ? <Text style={{ fontWeight: 500 }}>{v}</Text> : '—',
    },
    {
      title: 'IP-адрес',
      dataIndex: 'ip_address',
      key: 'ip',
      width: 132,
      render: (v: string) => <span className="qorgau-mono" style={{ fontSize: 12.5 }}>{v || '—'}</span>,
    },
    {
      title: 'Конфигурация',
      key: 'config',
      width: 245,
      render: (_: unknown, rec: Agent) => {
        if (!isAdmin) {
          return rec.config_id ? <Tag bordered={false}>Профиль #{rec.config_id}</Tag> : <Tag bordered={false}>По умолчанию</Tag>
        }
        return (
          <div onClick={e => e.stopPropagation()}>
            <Select
              size="small"
              style={{ width: '100%' }}
              placeholder="По умолчанию"
              allowClear
              value={rec.config_id ?? undefined}
              loading={assignMutation.isPending}
              onChange={(val: number | undefined) => {
                assignMutation.mutate({
                  agentIds: [rec.agent_id],
                  configId: val ?? null,
                  previousConfigId: rec.config_id,
                })
              }}
              options={configs.map(c => ({ value: c.id!, label: c.name }))}
            />
          </div>
        )
      },
    },
    {
      title: 'ОС / версия',
      key: 'os',
      width: 175,
      render: (_: unknown, rec: Agent) => (
        <div>
          <div style={{ fontSize: 12.5, maxWidth: 165, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
            {rec.os_version || 'Windows'}
          </div>
          <span className="qorgau-mono" style={{ fontSize: 11.5, color: 'var(--p-muted)' }}>v{rec.agent_version || '2.0'}</span>
        </div>
      ),
    },
    {
      title: 'Последний сигнал',
      dataIndex: 'last_seen',
      key: 'last_seen',
      width: 140,
      render: (v: string | null) => v ? <span className="qorgau-mono" style={{ fontSize: 12, color: 'var(--p-muted)' }}>{dayjs(v).format('DD.MM.YY HH:mm')}</span> : '—',
    },
    {
      title: 'Команды',
      key: 'actions',
      width: 145,
      render: (_: unknown, rec: Agent) => (
        <Space size={4} onClick={e => e.stopPropagation()}>
          {isOfficer && (
            <>
              <Tooltip title="Сделать скриншот">
                <Button size="small" aria-label="Сделать скриншот" icon={<CameraOutlined />} onClick={() => handleScreenshot(rec.agent_id)} />
              </Tooltip>
              <Tooltip title="Запустить краулер ПДн">
                <Button size="small" aria-label="Запустить краулер ПДн" icon={<FileSearchOutlined />} onClick={() => handleDiscovery(rec.agent_id)} />
              </Tooltip>
              <Tooltip title="Собрать инвентаризацию">
                <Button size="small" aria-label="Собрать инвентаризацию" icon={<DesktopOutlined />} onClick={() => handleInventory(rec.agent_id)} />
              </Tooltip>
            </>
          )}
          {isAdmin && (
            <Popconfirm title="Удалить агент из реестра?" onConfirm={() => delMutation.mutate(rec.agent_id)}>
              <Button size="small" danger aria-label="Удалить агент" icon={<DeleteOutlined />} />
            </Popconfirm>
          )}
        </Space>
      ),
    },
  ]

  const currentSelected = selected ? (agents.find(a => a.agent_id === selected.agent_id) ?? selected) : null

  return (
    <div className="ts-enter">
      <div className="ts-page-head">
        <div>
          <Title level={2} className="ts-page-title">Рабочие станции и агенты</Title>
          <span className="ts-page-sub">
            Переключайте профили мониторинга на лету и отправляйте команды агентам по WebSocket
          </span>
        </div>
        <Space wrap size={8}>
          <Button icon={<CodeOutlined />} onClick={() => setDeployOpen(true)}>
            Инструкция установки
          </Button>
          <Button icon={<ReloadOutlined />} onClick={() => refetch()}>
            Обновить
          </Button>
          <Button
            type="primary"
            icon={<DownloadOutlined />}
            onClick={() => downloadWindowsAgent().catch(() => message.error('Файл агента не найден на сервере'))}
          >
            Скачать агент (.exe)
          </Button>
        </Space>
      </div>

      {/* 4 hairline KPI tiles */}
      <div className="ts-kpi-grid">
        <div className="ts-kpi-tile">
          <div className="ts-kpi-label"><span>Всего станций</span><LaptopOutlined /></div>
          <div className="ts-kpi-value">{agents.length}</div>
          <div className="ts-kpi-meta">Зарегистрировано в системе</div>
        </div>
        <div className="ts-kpi-tile">
          <div className="ts-kpi-label"><span>Онлайн</span><CheckCircleOutlined /></div>
          <div className="ts-kpi-value" style={{ color: '#10b981' }}>{onlineCount}</div>
          <div className="ts-kpi-meta">Активный heartbeat (&lt; 3 мин)</div>
        </div>
        <div className="ts-kpi-tile">
          <div className="ts-kpi-label"><span>Офлайн</span><StopOutlined /></div>
          <div className="ts-kpi-value">{agents.length - onlineCount}</div>
          <div className="ts-kpi-meta">Отключены или вне сети</div>
        </div>
        <div className="ts-kpi-tile">
          <div className="ts-kpi-label"><span>С назначенным профилем</span><ApartmentOutlined /></div>
          <div className="ts-kpi-value" style={{ color: 'var(--p-accent)' }}>{configuredCount}</div>
          <div className="ts-kpi-meta">Остальные на профиле по умолчанию</div>
        </div>
      </div>

      {/* Filters & Bulk Assignment */}
      <Card size="small" style={{ marginBottom: 14 }}>
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', flexWrap: 'wrap', gap: 10 }}>
          <Space wrap>
            <Input
              placeholder="Поиск по хосту, пользователю, IP..."
              prefix={<SearchOutlined />}
              value={search}
              onChange={e => setSearch(e.target.value)}
              style={{ width: 260 }}
              allowClear
            />
            <Select
              value={statusFilter}
              onChange={setStatusFilter}
              style={{ width: 140 }}
              options={[
                { value: 'all', label: 'Все статусы' },
                { value: 'online', label: 'Только онлайн' },
                { value: 'offline', label: 'Только офлайн' },
              ]}
            />
            {isAdmin && (
              <Select
                placeholder="Фильтр по профилю"
                allowClear
                style={{ width: 230 }}
                value={configFilter}
                onChange={setConfigFilter}
                options={[
                  { value: 'none', label: 'Без профиля (по умолчанию)' },
                  ...configs.map(c => ({ value: c.id!, label: c.name })),
                ]}
              />
            )}
          </Space>

          {isAdmin && selectedRowKeys.length > 0 && (
            <Space wrap>
              <Text style={{ fontSize: 13 }}>Выбрано: <strong>{selectedRowKeys.length}</strong></Text>
              <Select
                placeholder="Выберите профиль..."
                style={{ width: 230 }}
                value={bulkConfigId}
                onChange={setBulkConfigId}
                options={configs.map(c => ({ value: c.id!, label: c.name }))}
              />
              <Button
                type="primary"
                disabled={!bulkConfigId}
                loading={assignMutation.isPending}
                onClick={() => {
                  if (bulkConfigId) {
                    assignMutation.mutate({ agentIds: selectedRowKeys, configId: bulkConfigId })
                  }
                }}
              >
                Применить профиль ({selectedRowKeys.length})
              </Button>
            </Space>
          )}
        </div>
      </Card>

      <Card styles={{ body: { padding: 0 } }}>
        <Table
          dataSource={filtered}
          columns={columns}
          rowKey="agent_id"
          loading={isLoading}
          size="small"
          pagination={{ pageSize: 20, showSizeChanger: true }}
          scroll={{ x: 960 }}
          rowSelection={isAdmin ? {
            selectedRowKeys,
            onChange: keys => setSelectedRowKeys(keys as string[]),
          } : undefined}
          onRow={rec => ({ onClick: () => setSelected(rec), style: { cursor: 'pointer' } })}
        />
      </Card>

      {/* Agent Detail Drawer */}
      <Drawer
        open={!!currentSelected}
        onClose={() => setSelected(null)}
        title={
          currentSelected ? (
            <Space size={8}>
              <span className="ts-dot" style={{ background: isOnline(currentSelected) ? '#10b981' : '#64748b' }} />
              <span>{currentSelected.hostname}</span>
              <span style={{ color: 'var(--p-muted)', fontWeight: 400 }}>{currentSelected.username || '—'}</span>
            </Space>
          ) : null
        }
        width={480}
      >
        {currentSelected && (
          <>
            <Descriptions column={1} bordered size="small">
              <Descriptions.Item label="Agent ID"><span className="qorgau-mono">{currentSelected.agent_id}</span></Descriptions.Item>
              <Descriptions.Item label="Хост">{currentSelected.hostname}</Descriptions.Item>
              <Descriptions.Item label="Пользователь">{currentSelected.username}</Descriptions.Item>
              <Descriptions.Item label="IP-адрес"><span className="qorgau-mono">{currentSelected.ip_address || '—'}</span></Descriptions.Item>
              <Descriptions.Item label="ОС">{currentSelected.os_version || '—'}</Descriptions.Item>
              <Descriptions.Item label="Версия">{currentSelected.agent_version || '2.0'}</Descriptions.Item>
              <Descriptions.Item label="Последний сигнал">
                {currentSelected.last_seen ? dayjs(currentSelected.last_seen).format('DD.MM.YYYY HH:mm:ss') : '—'}
              </Descriptions.Item>
              <Descriptions.Item label="Регистрация">
                {currentSelected.registered_at ? dayjs(currentSelected.registered_at).format('DD.MM.YYYY HH:mm') : '—'}
              </Descriptions.Item>
            </Descriptions>

            {isAdmin && (
              <>
                <Divider orientation="left" style={{ fontSize: 12.5 }}>Профиль конфигурации</Divider>
                <Select
                  style={{ width: '100%' }}
                  placeholder="Использовать профиль по умолчанию"
                  allowClear
                  value={currentSelected.config_id ?? undefined}
                  onChange={(val: number | undefined) => {
                    assignMutation.mutate({
                      agentIds: [currentSelected.agent_id],
                      configId: val ?? null,
                      previousConfigId: currentSelected.config_id,
                    })
                  }}
                  options={configs.map(c => ({ value: c.id!, label: c.name }))}
                />
                {currentSelected.config_id && configMap.get(currentSelected.config_id)?.description && (
                  <Text type="secondary" style={{ fontSize: 12.5, display: 'block', marginTop: 8 }}>
                    {configMap.get(currentSelected.config_id)?.description}
                  </Text>
                )}
              </>
            )}

            {isOfficer && (
              <>
                <Divider orientation="left" style={{ fontSize: 12.5 }}>Команды агенту</Divider>
                <Space direction="vertical" style={{ width: '100%' }}>
                  <Button block icon={<CameraOutlined />} onClick={() => handleScreenshot(currentSelected.agent_id)}>
                    Сделать скриншот экрана
                  </Button>
                  <Button block icon={<FileSearchOutlined />} onClick={() => handleDiscovery(currentSelected.agent_id)}>
                    Запустить краулер ПДн
                  </Button>
                  <Button block icon={<DesktopOutlined />} onClick={() => handleInventory(currentSelected.agent_id)}>
                    Собрать инвентаризацию ПО и железа
                  </Button>
                </Space>
              </>
            )}
          </>
        )}
      </Drawer>

      {/* Deploy Instructions Modal */}
      <Modal
        open={deployOpen}
        onCancel={() => setDeployOpen(false)}
        title="Подключение рабочей станции Windows"
        width={680}
        footer={[
          <Button
            key="dl"
            icon={<DownloadOutlined />}
            onClick={() => downloadWindowsAgent().catch(() => message.error('Файл агента не найден'))}
          >
            Скачать qorgau-agent.exe
          </Button>,
          <Button
            key="copy"
            type="primary"
            icon={<CopyOutlined />}
            onClick={() => {
              navigator.clipboard?.writeText(installCmd)
              message.success('Команда скопирована')
            }}
          >
            Скопировать команду
          </Button>,
        ]}
      >
        <Alert
          type="info"
          showIcon
          style={{ marginBottom: 14 }}
          message="Нативный агент на Rust (2.94 МБ, без .NET и Python)"
          description="После запуска агент зарегистрируется в таблице в течение 5–10 секунд. Затем выберите ему профиль прямо в колонке «Конфигурация»."
        />
        <pre
          className="qorgau-mono"
          style={{
            padding: 14,
            borderRadius: 6,
            fontSize: 12,
            overflowX: 'auto',
            background: '#0a0d14',
            color: '#e9edf3',
            border: '1px solid rgba(233, 237, 243, 0.14)',
          }}
        >
          {installCmd}
        </pre>
      </Modal>
    </div>
  )
}
