import { useState } from 'react'
import {
  Tabs, Card, Table, Button, Space, Tag, Switch, Modal,
  Form, Input, Select, Popconfirm, message, Typography,
} from 'antd'
import { PlusOutlined, DeleteOutlined, EditOutlined } from '@ant-design/icons'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import dayjs from 'dayjs'
import {
  fetchPolicies, createPolicy, updatePolicy, deletePolicy,
  fetchBlockedDomains, createBlockedDomain, deleteBlockedDomain,
  fetchOperators, createOperator, updateOperator, deleteOperator,
  type Operator,
} from '../api/client'
import type { Policy, BlockedDomain } from '../api/types'
import { useAuth } from '../context/AuthContext'

const { Title } = Typography

const ROLES = [
  { label: 'Администратор', value: 'admin' },
  { label: 'Офицер ИБ', value: 'security_officer' },
  { label: 'Наблюдатель', value: 'viewer' },
]
const ROLE_LABEL: Record<string, string> = {
  admin: 'Администратор', security_officer: 'Офицер ИБ', viewer: 'Наблюдатель',
}
const ROLE_COLOR: Record<string, string> = {
  admin: 'red', security_officer: 'orange', viewer: 'blue',
}

const CHANNELS = ['usb','web','messenger','clipboard','network','print','app','*'].map(c => ({ label: c, value: c }))
const ACTIONS = ['alert','block','log'].map(a => ({ label: a, value: a }))
const SEVERITIES = ['low','medium','high','critical'].map(s => ({ label: s, value: s }))
const CATEGORIES = ['social','adult','gambling','streaming','crypto','storage','productivity','other']
  .map(c => ({ label: c, value: c }))

function PoliciesTab() {
  const qc = useQueryClient()
  const [modalOpen, setModalOpen] = useState(false)
  const [editing, setEditing] = useState<Policy | null>(null)
  const [form] = Form.useForm()

  const { data: policies = [], isLoading } = useQuery({
    queryKey: ['policies'],
    queryFn: fetchPolicies,
  })

  const createMutation = useMutation({
    mutationFn: (data: Partial<Policy>) => editing ? updatePolicy(editing.id, data) : createPolicy(data),
    onSuccess: () => {
      message.success(editing ? 'Политика обновлена' : 'Политика создана')
      qc.invalidateQueries({ queryKey: ['policies'] })
      setModalOpen(false)
      setEditing(null)
      form.resetFields()
    },
    onError: () => message.error('Ошибка сохранения'),
  })

  const deleteMutation = useMutation({
    mutationFn: (id: number) => deletePolicy(id),
    onSuccess: () => { message.success('Удалено'); qc.invalidateQueries({ queryKey: ['policies'] }) },
  })

  const openCreate = () => { setEditing(null); form.resetFields(); setModalOpen(true) }
  const openEdit = (p: Policy) => {
    setEditing(p)
    form.setFieldsValue(p)
    setModalOpen(true)
  }

  const columns = [
    { title: 'Название', dataIndex: 'name', key: 'name' },
    { title: 'Канал', dataIndex: 'channel', key: 'channel', width: 100 },
    {
      title: 'Паттерн', dataIndex: 'pattern', key: 'pattern', ellipsis: true,
      render: (v: string) => <code style={{ fontSize: 11 }}>{v}</code>,
    },
    {
      title: 'Серьёзность', dataIndex: 'severity', key: 'severity', width: 100,
      render: (v: string) => <Tag color={{ critical: 'red', high: 'orange', medium: 'gold', low: 'green' }[v]}>{v}</Tag>,
    },
    {
      title: 'Действие', dataIndex: 'action', key: 'action', width: 90,
      render: (v: string) => <Tag color={{ block: 'red', alert: 'orange', log: 'blue' }[v] || 'default'}>{v}</Tag>,
    },
    {
      title: 'Включена', dataIndex: 'enabled', key: 'enabled', width: 90,
      render: (v: boolean, rec: Policy) => (
        <Switch
          checked={v}
          size="small"
          onChange={en => createMutation.mutate({ ...rec, enabled: en })}
        />
      ),
    },
    {
      title: '', key: 'actions', width: 80,
      render: (_: unknown, rec: Policy) => (
        <Space>
          <Button size="small" icon={<EditOutlined />} onClick={() => openEdit(rec)} />
          <Popconfirm title="Удалить?" onConfirm={() => deleteMutation.mutate(rec.id)}>
            <Button size="small" danger icon={<DeleteOutlined />} />
          </Popconfirm>
        </Space>
      ),
    },
  ]

  return (
    <>
      <div style={{ display: 'flex', justifyContent: 'flex-end', marginBottom: 12 }}>
        <Button type="primary" icon={<PlusOutlined />} onClick={openCreate}>Добавить политику</Button>
      </div>
      <Table
        dataSource={policies}
        columns={columns}
        rowKey="id"
        loading={isLoading}
        size="small"
        pagination={{ pageSize: 20 }}
      />

      <Modal
        title={editing ? 'Редактировать политику' : 'Новая политика'}
        open={modalOpen}
        onCancel={() => { setModalOpen(false); setEditing(null) }}
        onOk={() => form.submit()}
        confirmLoading={createMutation.isPending}
      >
        <Form form={form} layout="vertical" onFinish={data => createMutation.mutate(data)}>
          <Form.Item name="name" label="Название" rules={[{ required: true }]}>
            <Input />
          </Form.Item>
          <Form.Item name="channel" label="Канал" rules={[{ required: true }]}>
            <Select options={CHANNELS} />
          </Form.Item>
          <Form.Item name="pattern" label="Паттерн (regex)" rules={[{ required: true }]}>
            <Input.TextArea rows={2} style={{ fontFamily: 'monospace', fontSize: 12 }} />
          </Form.Item>
          <Form.Item name="severity" label="Серьёзность" initialValue="medium" rules={[{ required: true }]}>
            <Select options={SEVERITIES} />
          </Form.Item>
          <Form.Item name="action" label="Действие" initialValue="alert" rules={[{ required: true }]}>
            <Select options={ACTIONS} />
          </Form.Item>
        </Form>
      </Modal>
    </>
  )
}

function DomainsTab() {
  const qc = useQueryClient()
  const [modalOpen, setModalOpen] = useState(false)
  const [form] = Form.useForm()

  const { data: domains = [], isLoading } = useQuery({
    queryKey: ['blocked-domains'],
    queryFn: fetchBlockedDomains,
  })

  const createMutation = useMutation({
    mutationFn: (data: Partial<BlockedDomain>) => createBlockedDomain(data),
    onSuccess: () => {
      message.success('Домен добавлен')
      qc.invalidateQueries({ queryKey: ['blocked-domains'] })
      setModalOpen(false)
      form.resetFields()
    },
  })

  const deleteMutation = useMutation({
    mutationFn: (id: number) => deleteBlockedDomain(id),
    onSuccess: () => { message.success('Удалено'); qc.invalidateQueries({ queryKey: ['blocked-domains'] }) },
  })

  const columns = [
    { title: 'Домен', dataIndex: 'domain', key: 'domain' },
    {
      title: 'Категория', dataIndex: 'category', key: 'category', width: 120,
      render: (v: string) => <Tag>{v}</Tag>,
    },
    { title: 'Причина', dataIndex: 'reason', key: 'reason', ellipsis: true },
    {
      title: 'Добавлен', dataIndex: 'created_at', key: 'created_at', width: 140,
      render: (v: string | null) => v ? dayjs(v).format('DD.MM.YYYY') : '—',
    },
    {
      title: '', key: 'actions', width: 60,
      render: (_: unknown, rec: BlockedDomain) => (
        <Popconfirm title="Удалить?" onConfirm={() => deleteMutation.mutate(rec.id)}>
          <Button size="small" danger icon={<DeleteOutlined />} />
        </Popconfirm>
      ),
    },
  ]

  return (
    <>
      <div style={{ display: 'flex', justifyContent: 'flex-end', marginBottom: 12 }}>
        <Button type="primary" icon={<PlusOutlined />} onClick={() => setModalOpen(true)}>
          Заблокировать домен
        </Button>
      </div>
      <Table
        dataSource={domains}
        columns={columns}
        rowKey="id"
        loading={isLoading}
        size="small"
        pagination={{ pageSize: 25 }}
      />

      <Modal
        title="Заблокировать домен"
        open={modalOpen}
        onCancel={() => setModalOpen(false)}
        onOk={() => form.submit()}
        confirmLoading={createMutation.isPending}
      >
        <Form form={form} layout="vertical" onFinish={data => createMutation.mutate(data)}>
          <Form.Item name="domain" label="Домен" rules={[{ required: true }]}>
            <Input placeholder="example.com" />
          </Form.Item>
          <Form.Item name="category" label="Категория" rules={[{ required: true }]}>
            <Select options={CATEGORIES} />
          </Form.Item>
          <Form.Item name="reason" label="Причина">
            <Input />
          </Form.Item>
        </Form>
      </Modal>
    </>
  )
}

function OperatorsTab() {
  const qc = useQueryClient()
  const { user } = useAuth()
  const [modalOpen, setModalOpen] = useState(false)
  const [editing, setEditing] = useState<Operator | null>(null)
  const [form] = Form.useForm()

  const { data: operators = [], isLoading } = useQuery({
    queryKey: ['operators'],
    queryFn: fetchOperators,
  })

  const saveMutation = useMutation({
    mutationFn: (data: Record<string, unknown>) =>
      editing
        ? updateOperator(editing.id, data)
        : createOperator(data as { username: string; password: string; role: string }),
    onSuccess: () => {
      message.success(editing ? 'Оператор обновлён' : 'Оператор создан')
      qc.invalidateQueries({ queryKey: ['operators'] })
      setModalOpen(false)
      setEditing(null)
      form.resetFields()
    },
    onError: () => message.error('Ошибка сохранения (проверьте права и уникальность логина)'),
  })

  const deleteMutation = useMutation({
    mutationFn: (id: number) => deleteOperator(id),
    onSuccess: () => { message.success('Удалено'); qc.invalidateQueries({ queryKey: ['operators'] }) },
    onError: () => message.error('Не удалось удалить'),
  })

  const openCreate = () => { setEditing(null); form.resetFields(); setModalOpen(true) }
  const openEdit = (o: Operator) => {
    setEditing(o)
    form.setFieldsValue({ role: o.role, full_name: o.full_name, is_active: o.is_active })
    setModalOpen(true)
  }

  const columns = [
    { title: 'Логин', dataIndex: 'username', key: 'username' },
    { title: 'Имя', dataIndex: 'full_name', key: 'full_name', render: (v: string | null) => v || '—' },
    {
      title: 'Роль', dataIndex: 'role', key: 'role', width: 150,
      render: (v: string) => <Tag color={ROLE_COLOR[v]}>{ROLE_LABEL[v] || v}</Tag>,
    },
    {
      title: 'Активен', dataIndex: 'is_active', key: 'is_active', width: 90,
      render: (v: boolean) => <Tag color={v ? 'success' : 'default'}>{v ? 'Да' : 'Нет'}</Tag>,
    },
    {
      title: 'Вход', dataIndex: 'last_login', key: 'last_login', width: 140,
      render: (v: string | null) => v ? dayjs(v).format('DD.MM.YY HH:mm') : '—',
    },
    {
      title: '', key: 'actions', width: 80,
      render: (_: unknown, rec: Operator) => (
        <Space>
          <Button size="small" icon={<EditOutlined />} onClick={() => openEdit(rec)} />
          <Popconfirm
            title="Удалить оператора?"
            onConfirm={() => deleteMutation.mutate(rec.id)}
            disabled={rec.username === user?.username}
          >
            <Button size="small" danger icon={<DeleteOutlined />} disabled={rec.username === user?.username} />
          </Popconfirm>
        </Space>
      ),
    },
  ]

  return (
    <>
      <div style={{ display: 'flex', justifyContent: 'flex-end', marginBottom: 12 }}>
        <Button type="primary" icon={<PlusOutlined />} onClick={openCreate}>Добавить оператора</Button>
      </div>
      <Table
        dataSource={operators}
        columns={columns}
        rowKey="id"
        loading={isLoading}
        size="small"
        pagination={{ pageSize: 20 }}
      />

      <Modal
        title={editing ? `Оператор: ${editing.username}` : 'Новый оператор'}
        open={modalOpen}
        onCancel={() => { setModalOpen(false); setEditing(null) }}
        onOk={() => form.submit()}
        confirmLoading={saveMutation.isPending}
      >
        <Form form={form} layout="vertical" onFinish={data => saveMutation.mutate(data)}>
          {!editing && (
            <Form.Item name="username" label="Логин" rules={[{ required: true }]}>
              <Input autoComplete="off" />
            </Form.Item>
          )}
          <Form.Item name="full_name" label="Имя (ФИО)">
            <Input />
          </Form.Item>
          <Form.Item
            name="password"
            label={editing ? 'Новый пароль (оставьте пустым, чтобы не менять)' : 'Пароль'}
            rules={editing ? [] : [{ required: true, min: 6 }]}
          >
            <Input.Password autoComplete="new-password" />
          </Form.Item>
          <Form.Item name="role" label="Роль" initialValue="viewer" rules={[{ required: true }]}>
            <Select options={ROLES} />
          </Form.Item>
          {editing && (
            <Form.Item name="is_active" label="Активен" valuePropName="checked">
              <Switch />
            </Form.Item>
          )}
        </Form>
      </Modal>
    </>
  )
}

export default function SettingsPage() {
  const { user } = useAuth()
  const isAdmin = user?.role === 'admin'

  const tabItems = [
    { key: 'policies', label: 'DLP Политики', children: <PoliciesTab /> },
    { key: 'domains', label: 'Заблокированные домены', children: <DomainsTab /> },
    ...(isAdmin ? [{ key: 'operators', label: 'Операторы', children: <OperatorsTab /> }] : []),
  ]

  return (
    <div>
      <Title level={4} style={{ marginBottom: 20 }}>Настройки</Title>
      <Card>
        <Tabs items={tabItems} />
      </Card>
    </div>
  )
}
