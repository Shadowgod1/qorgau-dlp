import { useState } from 'react'
import {
  Button, Card, Descriptions, Drawer, Input, Progress, Space, Spin, Table, Tag, Typography, message,
} from 'antd'
import {
  DesktopOutlined, DownloadOutlined, ReloadOutlined, SearchOutlined, SyncOutlined,
} from '@ant-design/icons'
import { useMutation, useQuery } from '@tanstack/react-query'
import dayjs from 'dayjs'
import {
  downloadWindowsAgent, fetchAgentInventoryDetail, fetchInventoryList, triggerInventoryScan,
  type AgentInventory,
} from '../api/client'
import { useAuth } from '../context/AuthContext'

const { Title, Text } = Typography

export default function InventoryPage() {
  const { user } = useAuth()
  const canTrigger = user?.role === 'admin' || user?.role === 'security_officer'
  const [selected, setSelected] = useState<AgentInventory | null>(null)
  const [swSearch, setSwSearch] = useState('')

  const { data: list = [], isLoading, refetch } = useQuery({
    queryKey: ['inventory-list'],
    queryFn: fetchInventoryList,
  })

  const { data: detail, isLoading: detailLoading } = useQuery({
    queryKey: ['inventory-detail', selected?.agent_id],
    queryFn: () => fetchAgentInventoryDetail(selected!.agent_id),
    enabled: !!selected,
  })

  const scanMut = useMutation({
    mutationFn: (agentId?: string) => triggerInventoryScan(agentId),
    onSuccess: () => message.success('Запрос инвентаризации отправлен онлайн-агентам'),
    onError: () => message.error('Ошибка отправки команды'),
  })

  const swList = (detail?.software ?? []).filter(
    (s) =>
      !swSearch.trim() ||
      s.name.toLowerCase().includes(swSearch.toLowerCase()) ||
      s.publisher.toLowerCase().includes(swSearch.toLowerCase()),
  )

  const columns = [
    {
      title: 'Рабочая станция', key: 'host',
      render: (_: unknown, r: AgentInventory) => (
        <Space direction="vertical" size={0}>
          <Text strong>{r.hostname}</Text>
          <Text type="secondary" style={{ fontSize: 11 }}>{r.username}</Text>
        </Space>
      ),
    },
    { title: 'ОС', dataIndex: 'os_build', key: 'os', ellipsis: true },
    {
      title: 'Процессор', key: 'cpu', ellipsis: true,
      render: (_: unknown, r: AgentInventory) => `${r.cpu_model} (${r.cpu_cores} ядер)`,
    },
    {
      title: 'ОЗУ', dataIndex: 'ram_total_mb', key: 'ram', width: 100,
      render: (v: number) => `${(v / 1024).toFixed(1)} ГБ`,
    },
    {
      title: 'Диск C:', key: 'disk', width: 170,
      render: (_: unknown, r: AgentInventory) => {
        const usedPct = r.disk_total_gb > 0 ? Math.round(((r.disk_total_gb - r.disk_free_gb) / r.disk_total_gb) * 100) : 0
        return (
          <div>
            <Progress percent={usedPct} size="small" status={usedPct > 90 ? 'exception' : 'normal'} />
            <Text type="secondary" style={{ fontSize: 10 }}>
              Свободно {r.disk_free_gb} / {r.disk_total_gb} ГБ
            </Text>
          </div>
        )
      },
    },
    {
      title: 'ПО', dataIndex: 'software_count', key: 'sw', width: 90,
      render: (v: number) => <Tag color="blue">{v} прог.</Tag>,
    },
    {
      title: 'Обновлено', dataIndex: 'updated_at', key: 'upd', width: 135,
      render: (v: string) => dayjs(v).format('DD.MM.YY HH:mm'),
    },
  ]

  return (
    <div>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 16, flexWrap: 'wrap', gap: 12 }}>
        <div>
          <Title level={4} style={{ margin: 0 }}>
            <DesktopOutlined style={{ marginRight: 8 }} />
            Инвентаризация оборудования и ПО
          </Title>
          <Text type="secondary" style={{ fontSize: 13 }}>
            Автоматический учёт характеристик ПК, установленных программ и отслеживание новых/удалённых приложений
          </Text>
        </div>
        <Space wrap>
          <Button
            icon={<DownloadOutlined />}
            onClick={() =>
              downloadWindowsAgent().catch(() =>
                message.warning('Бинарник qorgau-agent.exe не размещён в /app/downloads на сервере'),
              )
            }
          >
            Скачать агент (.exe)
          </Button>
          {canTrigger && (
            <Button
              type="primary"
              icon={<SyncOutlined />}
              loading={scanMut.isPending}
              onClick={() => scanMut.mutate(undefined)}
            >
              Опросить все ПК
            </Button>
          )}
          <Button icon={<ReloadOutlined />} onClick={() => refetch()}>Обновить</Button>
        </Space>
      </div>

      <Card>
        <Table
          dataSource={list}
          columns={columns}
          rowKey="agent_id"
          loading={isLoading}
          size="small"
          scroll={{ x: 960 }}
          onRow={(rec) => ({ onClick: () => setSelected(rec), style: { cursor: 'pointer' } })}
        />
      </Card>

      <Drawer
        open={!!selected}
        onClose={() => { setSelected(null); setSwSearch('') }}
        width={720}
        title={selected ? `Инвентаризация: ${selected.hostname} (${selected.username})` : ''}
      >
        {detailLoading ? (
          <Spin style={{ display: 'block', margin: '48px auto' }} />
        ) : (
          selected && (
            <>
              <Descriptions bordered size="small" column={2} style={{ marginBottom: 16 }}>
                <Descriptions.Item label="ОС" span={2}>{selected.os_build}</Descriptions.Item>
                <Descriptions.Item label="CPU" span={2}>{selected.cpu_model} ({selected.cpu_cores} ядер)</Descriptions.Item>
                <Descriptions.Item label="ОЗУ">{(selected.ram_total_mb / 1024).toFixed(1)} ГБ</Descriptions.Item>
                <Descriptions.Item label="Диск C:">Свободно {selected.disk_free_gb} / {selected.disk_total_gb} ГБ</Descriptions.Item>
              </Descriptions>

              <Input
                placeholder="Фильтр установленного ПО..."
                prefix={<SearchOutlined />}
                value={swSearch}
                onChange={(e) => setSwSearch(e.target.value)}
                style={{ marginBottom: 12 }}
                allowClear
              />

              <Table
                size="small"
                rowKey="id"
                dataSource={swList}
                pagination={{ pageSize: 20, showTotal: (t) => `Всего ПО: ${t}` }}
                columns={[
                  { title: 'Программа', dataIndex: 'name', key: 'name' },
                  { title: 'Версия', dataIndex: 'version', key: 'ver', width: 120 },
                  { title: 'Издатель', dataIndex: 'publisher', key: 'pub', ellipsis: true },
                  {
                    title: 'Статус', dataIndex: 'status', key: 'st', width: 105,
                    render: (v: string) => (
                      <Tag color={v === 'installed' ? 'green' : 'red'}>
                        {v === 'installed' ? 'Установлено' : 'Удалено'}
                      </Tag>
                    ),
                  },
                ]}
              />
            </>
          )
        )}
      </Drawer>
    </div>
  )
}
