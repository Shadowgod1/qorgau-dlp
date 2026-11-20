import { useState } from 'react'
import {
  Table, Tag, Button, Drawer, Descriptions, Typography,
  Space, Popconfirm, message, Badge, Input, Card,
} from 'antd'
import {
  ReloadOutlined, DeleteOutlined, CameraOutlined, SearchOutlined,
} from '@ant-design/icons'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import dayjs from 'dayjs'

const isOnline = (agent: { last_seen: string | null }) =>
  !!agent.last_seen && dayjs().diff(dayjs(agent.last_seen), 'minute') < 3
import { fetchAgents, deleteAgent, triggerScreenshot } from '../api/client'
import type { Agent } from '../api/types'

const { Title } = Typography

export default function AgentsPage() {
  const qc = useQueryClient()
  const [selected, setSelected] = useState<Agent | null>(null)
  const [search, setSearch] = useState('')

  const { data: agents = [], isLoading, refetch } = useQuery({
    queryKey: ['agents'],
    queryFn: fetchAgents,
    refetchInterval: 15_000,
  })

  const delMutation = useMutation({
    mutationFn: (agentId: string) => deleteAgent(agentId),
    onSuccess: () => {
      message.success('Агент удалён')
      qc.invalidateQueries({ queryKey: ['agents'] })
      setSelected(null)
    },
  })

  const handleScreenshot = async (agentId: string) => {
    try {
      await triggerScreenshot(agentId)
      message.success('Скриншот запрошен')
    } catch {
      message.error('Ошибка запроса скриншота')
    }
  }

  const filtered = agents.filter(a =>
    !search ||
    a.hostname.toLowerCase().includes(search.toLowerCase()) ||
    a.username.toLowerCase().includes(search.toLowerCase()) ||
    (a.ip_address || '').includes(search)
  )

  const columns = [
    {
      title: 'Статус', key: 'status', width: 90,
      render: (_: unknown, rec: Agent) => {
        const online = isOnline(rec)
        return <Badge status={online ? 'success' : 'default'} text={online ? 'Online' : 'Offline'} />
      },
    },
    { title: 'Хост', dataIndex: 'hostname', key: 'hostname' },
    { title: 'Пользователь', dataIndex: 'username', key: 'username' },
    { title: 'IP', dataIndex: 'ip_address', key: 'ip', width: 130 },
    { title: 'ОС', dataIndex: 'os_version', key: 'os', ellipsis: true },
    { title: 'Версия', dataIndex: 'agent_version', key: 'version', width: 100 },
    {
      title: 'Последний сигнал', dataIndex: 'last_seen', key: 'last_seen', width: 150,
      render: (v: string | null) => v ? dayjs(v).format('DD.MM.YYYY HH:mm') : '—',
    },
    {
      title: '', key: 'actions', width: 90,
      render: (_: unknown, rec: Agent) => (
        <Space>
          <Button
            size="small"
            icon={<CameraOutlined />}
            onClick={e => { e.stopPropagation(); handleScreenshot(rec.agent_id) }}
            title="Сделать скриншот"
          />
          <Popconfirm
            title="Удалить агент?"
            onConfirm={e => { e?.stopPropagation(); delMutation.mutate(rec.agent_id) }}
          >
            <Button size="small" danger icon={<DeleteOutlined />} onClick={e => e.stopPropagation()} />
          </Popconfirm>
        </Space>
      ),
    },
  ]

  return (
    <div>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 20 }}>
        <Title level={4} style={{ margin: 0 }}>Агенты</Title>
        <Space>
          <Input
            placeholder="Поиск по хосту, пользователю, IP"
            prefix={<SearchOutlined />}
            value={search}
            onChange={e => setSearch(e.target.value)}
            style={{ width: 260 }}
            allowClear
          />
          <Button icon={<ReloadOutlined />} onClick={() => refetch()}>Обновить</Button>
        </Space>
      </div>

      <Card>
        <Table
          dataSource={filtered}
          columns={columns}
          rowKey="agent_id"
          loading={isLoading}
          size="small"
          pagination={{ pageSize: 20 }}
          onRow={rec => ({ onClick: () => setSelected(rec), style: { cursor: 'pointer' } })}
        />
      </Card>

      <Drawer
        title={selected?.hostname || 'Агент'}
        open={!!selected}
        onClose={() => setSelected(null)}
        width={480}
        extra={
          <Space>
            <Button
              icon={<CameraOutlined />}
              onClick={() => selected && handleScreenshot(selected.agent_id)}
            >
              Скриншот
            </Button>
            <Popconfirm
              title="Удалить агент?"
              onConfirm={() => selected && delMutation.mutate(selected.agent_id)}
            >
              <Button danger icon={<DeleteOutlined />}>Удалить</Button>
            </Popconfirm>
          </Space>
        }
      >
        {selected && (
          <Descriptions column={1} bordered size="small">
            <Descriptions.Item label="ID агента">
              <code style={{ fontSize: 11 }}>{selected.agent_id}</code>
            </Descriptions.Item>
            <Descriptions.Item label="Хост">{selected.hostname}</Descriptions.Item>
            <Descriptions.Item label="Пользователь">{selected.username}</Descriptions.Item>
            <Descriptions.Item label="IP">{selected.ip_address || '—'}</Descriptions.Item>
            <Descriptions.Item label="ОС">{selected.os_version}</Descriptions.Item>
            <Descriptions.Item label="Версия агента">{selected.agent_version}</Descriptions.Item>
            <Descriptions.Item label="Статус">
              <Tag color={selected.is_online ? 'green' : 'default'}>
                {selected.is_online ? 'Онлайн' : 'Оффлайн'}
              </Tag>
            </Descriptions.Item>
            <Descriptions.Item label="Последний сигнал">
              {selected.last_seen ? dayjs(selected.last_seen).format('DD.MM.YYYY HH:mm:ss') : '—'}
            </Descriptions.Item>
            <Descriptions.Item label="Зарегистрирован">
              {selected.registered_at ? dayjs(selected.registered_at).format('DD.MM.YYYY HH:mm') : '—'}
            </Descriptions.Item>
          </Descriptions>
        )}
      </Drawer>
    </div>
  )
}
