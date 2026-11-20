import { useState } from 'react'
import {
  Modal, Tabs, Table, Button, Form, Input, Select, Switch, Space,
  Popconfirm, Tag, message,
} from 'antd'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import {
  fetchPolicies, createPolicy, updatePolicy, deletePolicy,
  fetchBlockedDomains, createBlockedDomain, deleteBlockedDomain,
  fetchAgents, deleteAgent,
} from '../api/client'
import type { Policy, BlockedDomain, Agent } from '../api/types'
import { PlusOutlined, DeleteOutlined, EditOutlined } from '@ant-design/icons'
import dayjs from 'dayjs'

// ── Policies tab ─────────────────────────────────────────────────────────────
function PoliciesTab() {
  const qc = useQueryClient()
  const [form] = Form.useForm()
  const [editing, setEditing] = useState<Policy | null>(null)
  const [open, setOpen] = useState(false)

  const { data: policies = [], isLoading } = useQuery({
    queryKey: ['policies'],
    queryFn: fetchPolicies,
  })

  const save = useMutation({
    mutationFn: (vals: Partial<Policy>) =>
      editing ? updatePolicy(editing.id, vals) : createPolicy(vals),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['policies'] })
      setOpen(false)
      form.resetFields()
      setEditing(null)
      message.success('Сохранено')
    },
  })

  const remove = useMutation({
    mutationFn: deletePolicy,
    onSuccess: () => qc.invalidateQueries({ queryKey: ['policies'] }),
  })

  const openNew = () => { setEditing(null); form.resetFields(); setOpen(true) }
  const openEdit = (p: Policy) => { setEditing(p); form.setFieldsValue(p); setOpen(true) }

  const columns = [
    { title: 'Название', dataIndex: 'name', key: 'name' },
    { title: 'Канал', dataIndex: 'channel', key: 'channel', render: (v: string) => <Tag>{v}</Tag> },
    { title: 'Паттерн', dataIndex: 'pattern', key: 'pattern', render: (v: string) => <code style={{ fontSize: 11 }}>{v}</code> },
    { title: 'Действие', dataIndex: 'action', key: 'action' },
    { title: 'Серьёзность', dataIndex: 'severity', key: 'severity' },
    {
      title: 'Активна', dataIndex: 'enabled', key: 'enabled',
      render: (v: boolean) => <Tag color={v ? 'success' : 'default'}>{v ? 'Да' : 'Нет'}</Tag>,
    },
    {
      title: '', key: 'actions', width: 80,
      render: (_: unknown, row: Policy) => (
        <Space size={4}>
          <Button size="small" icon={<EditOutlined />} onClick={() => openEdit(row)} />
          <Popconfirm title="Удалить политику?" onConfirm={() => remove.mutate(row.id)}>
            <Button size="small" danger icon={<DeleteOutlined />} />
          </Popconfirm>
        </Space>
      ),
    },
  ]

  return (
    <>
      <div style={{ padding: '12px 0', display: 'flex', justifyContent: 'flex-end' }}>
        <Button type="primary" icon={<PlusOutlined />} size="small" onClick={openNew}>
          Добавить политику
        </Button>
      </div>
      <Table
        dataSource={policies}
        columns={columns}
        loading={isLoading}
        rowKey="id"
        size="small"
        pagination={{ pageSize: 10 }}
      />
      <Modal
        open={open}
        title={editing ? 'Редактировать политику' : 'Новая политика'}
        onCancel={() => setOpen(false)}
        onOk={() => form.submit()}
        okText="Сохранить"
        cancelText="Отмена"
      >
        <Form form={form} layout="vertical" onFinish={v => save.mutate(v)} size="small">
          <Form.Item name="name" label="Название" rules={[{ required: true }]}>
            <Input />
          </Form.Item>
          <Form.Item name="channel" label="Канал" rules={[{ required: true }]}>
            <Select options={[
              { value: 'usb', label: 'USB' },
              { value: 'messenger', label: 'Мессенджер' },
              { value: 'clipboard', label: 'Буфер обмена' },
              { value: 'browser', label: 'Браузер' },
              { value: 'file', label: 'Файлы' },
              { value: 'email', label: 'Email' },
            ]} />
          </Form.Item>
          <Form.Item name="pattern" label="Паттерн (regex)" rules={[{ required: true }]}>
            <Input placeholder="напр. \bИИН\b|\bBIN\b" />
          </Form.Item>
          <Form.Item name="action" label="Действие" rules={[{ required: true }]}>
            <Select options={[
              { value: 'alert', label: 'Алерт' },
              { value: 'block', label: 'Блок' },
              { value: 'log', label: 'Лог' },
            ]} />
          </Form.Item>
          <Form.Item name="severity" label="Серьёзность" rules={[{ required: true }]}>
            <Select options={[
              { value: 'low', label: 'Низкая' },
              { value: 'medium', label: 'Средняя' },
              { value: 'high', label: 'Высокая' },
              { value: 'critical', label: 'Критично' },
            ]} />
          </Form.Item>
          <Form.Item name="enabled" label="Активна" valuePropName="checked" initialValue={true}>
            <Switch />
          </Form.Item>
        </Form>
      </Modal>
    </>
  )
}

// ── Blocked domains tab ──────────────────────────────────────────────────────
function BlockedDomainsTab() {
  const qc = useQueryClient()
  const [form] = Form.useForm()
  const [open, setOpen] = useState(false)

  const { data: domains = [], isLoading } = useQuery({
    queryKey: ['blocked-domains'],
    queryFn: fetchBlockedDomains,
  })

  const add = useMutation({
    mutationFn: createBlockedDomain,
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['blocked-domains'] })
      setOpen(false)
      form.resetFields()
      message.success('Домен добавлен')
    },
  })

  const remove = useMutation({
    mutationFn: deleteBlockedDomain,
    onSuccess: () => qc.invalidateQueries({ queryKey: ['blocked-domains'] }),
  })

  const columns = [
    { title: 'Домен', dataIndex: 'domain', key: 'domain', render: (v: string) => <code>{v}</code> },
    { title: 'Категория', dataIndex: 'category', key: 'category', render: (v: string) => <Tag>{v}</Tag> },
    { title: 'Причина', dataIndex: 'reason', key: 'reason' },
    {
      title: 'Активен', dataIndex: 'enabled', key: 'enabled',
      render: (v: boolean) => <Tag color={v ? 'success' : 'default'}>{v ? 'Да' : 'Нет'}</Tag>,
    },
    {
      title: '', key: 'del', width: 50,
      render: (_: unknown, row: BlockedDomain) => (
        <Popconfirm title="Удалить домен?" onConfirm={() => remove.mutate(row.id)}>
          <Button size="small" danger icon={<DeleteOutlined />} />
        </Popconfirm>
      ),
    },
  ]

  return (
    <>
      <div style={{ padding: '12px 0', display: 'flex', justifyContent: 'flex-end' }}>
        <Button type="primary" icon={<PlusOutlined />} size="small" onClick={() => setOpen(true)}>
          Добавить домен
        </Button>
      </div>
      <Table
        dataSource={domains}
        columns={columns}
        loading={isLoading}
        rowKey="id"
        size="small"
        pagination={{ pageSize: 15 }}
      />
      <Modal
        open={open}
        title="Заблокировать домен"
        onCancel={() => setOpen(false)}
        onOk={() => form.submit()}
        okText="Добавить"
        cancelText="Отмена"
      >
        <Form form={form} layout="vertical" onFinish={v => add.mutate(v)} size="small">
          <Form.Item name="domain" label="Домен" rules={[{ required: true }]}>
            <Input placeholder="example.com" />
          </Form.Item>
          <Form.Item name="category" label="Категория" rules={[{ required: true }]}>
            <Select options={[
              { value: 'social', label: 'Соцсети' },
              { value: 'gambling', label: 'Азартные игры' },
              { value: 'adult', label: 'Взрослый контент' },
              { value: 'malware', label: 'Вредоносный' },
              { value: 'phishing', label: 'Фишинг' },
              { value: 'custom', label: 'Другое' },
            ]} />
          </Form.Item>
          <Form.Item name="reason" label="Причина">
            <Input />
          </Form.Item>
          <Form.Item name="enabled" label="Активен" valuePropName="checked" initialValue={true}>
            <Switch />
          </Form.Item>
        </Form>
      </Modal>
    </>
  )
}

// ── Agents tab ───────────────────────────────────────────────────────────────
function AgentsTab() {
  const qc = useQueryClient()

  const { data: agents = [], isLoading } = useQuery({
    queryKey: ['agents'],
    queryFn: fetchAgents,
  })

  const remove = useMutation({
    mutationFn: deleteAgent,
    onSuccess: () => qc.invalidateQueries({ queryKey: ['agents'] }),
  })

  const columns = [
    { title: 'Хост', dataIndex: 'hostname', key: 'hostname' },
    { title: 'Пользователь', dataIndex: 'username', key: 'username' },
    { title: 'IP', dataIndex: 'ip_address', key: 'ip_address' },
    { title: 'ОС', dataIndex: 'os_version', key: 'os_version', ellipsis: true },
    { title: 'Версия агента', dataIndex: 'agent_version', key: 'agent_version' },
    {
      title: 'Статус', dataIndex: 'is_online', key: 'is_online',
      render: (v: boolean) => <Tag color={v ? 'success' : 'default'}>{v ? 'Online' : 'Offline'}</Tag>,
    },
    {
      title: 'Последний раз', dataIndex: 'last_seen', key: 'last_seen',
      render: (v: string) => v ? dayjs(v).format('DD.MM.YY HH:mm') : '—',
    },
    {
      title: '', key: 'del', width: 50,
      render: (_: unknown, row: Agent) => (
        <Popconfirm title="Удалить агент?" onConfirm={() => remove.mutate(row.agent_id)}>
          <Button size="small" danger icon={<DeleteOutlined />} />
        </Popconfirm>
      ),
    },
  ]

  return (
    <Table
      dataSource={agents}
      columns={columns}
      loading={isLoading}
      rowKey="id"
      size="small"
      pagination={{ pageSize: 20 }}
    />
  )
}

// ── Main AdminPanel modal ────────────────────────────────────────────────────
interface AdminPanelProps {
  open: boolean
  onClose: () => void
}

export default function AdminPanel({ open, onClose }: AdminPanelProps) {
  const items = [
    { key: 'policies', label: 'Политики DLP', children: <PoliciesTab /> },
    { key: 'domains', label: 'Блок-лист доменов', children: <BlockedDomainsTab /> },
    { key: 'agents', label: 'Агенты', children: <AgentsTab /> },
  ]

  return (
    <Modal
      open={open}
      onCancel={onClose}
      footer={null}
      width={900}
      title="Администрирование"
      styles={{ body: { padding: 0 } }}
    >
      <Tabs
        className="admin-tabs"
        items={items}
        style={{ padding: '0 16px 16px' }}
      />
    </Modal>
  )
}
