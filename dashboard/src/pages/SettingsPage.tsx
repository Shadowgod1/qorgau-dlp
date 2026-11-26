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
  fetchBlockedApps, createBlockedApp, deleteBlockedApp,
  fetchOperators, createOperator, updateOperator, deleteOperator,
  fetchSiemSettings, updateSiemSettings, testSiemDelivery,
  fetchAdSettings, updateAdSettings, fetchAdUsers,
  type Operator, type BlockedApp, type SiemSettings, type AdSettings, type AdUser,
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

const CHANNELS = ['usb','web','mail','messenger','clipboard','network','print','app','*'].map(c => ({ label: c, value: c }))
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
    mutationFn: (data: Partial<Policy>) =>
      editing ? updatePolicy(editing.id, { ...data, enabled: editing.enabled }) : createPolicy(data),
    onSuccess: () => {
      message.success(editing ? 'Политика обновлена' : 'Политика создана')
      qc.invalidateQueries({ queryKey: ['policies'] })
      setModalOpen(false)
      setEditing(null)
      form.resetFields()
    },
    onError: () => message.error('Ошибка сохранения политики'),
  })

  const toggleMutation = useMutation({
    mutationFn: ({ rec, enabled }: { rec: Policy; enabled: boolean }) =>
      updatePolicy(rec.id, {
        name: rec.name,
        channel: rec.channel,
        action: rec.action,
        severity: rec.severity,
        pattern: rec.pattern,
        enabled,
      }),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['policies'] })
    },
    onError: () => message.error('Ошибка переключения политики'),
  })

  const deleteMutation = useMutation({
    mutationFn: (id: number) => deletePolicy(id),
    onSuccess: () => { message.success('Удалено'); qc.invalidateQueries({ queryKey: ['policies'] }) },
    onError: () => message.error('Ошибка удаления'),
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
          onChange={en => toggleMutation.mutate({ rec, enabled: en })}
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
    mutationFn: (data: Record<string, unknown>) => {
      const payload = { ...data }
      if (editing && (!payload.password || String(payload.password).trim() === '')) {
        delete payload.password
      }
      return editing
        ? updateOperator(editing.id, payload)
        : createOperator(payload as { username: string; password: string; role: string })
    },
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
            rules={editing ? [] : [{ required: true, min: 8, message: 'Минимум 8 символов' }]}
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

function BlockedAppsTab() {
  const qc = useQueryClient()
  const [open, setOpen] = useState(false)
  const [form] = Form.useForm()

  const { data: apps = [], isLoading } = useQuery({
    queryKey: ['blocked-apps'],
    queryFn: fetchBlockedApps,
  })

  const addMut = useMutation({
    mutationFn: (data: { pattern: string; action: string; reason?: string }) => createBlockedApp(data),
    onSuccess: () => {
      message.success('Правило блокировки ПО добавлено и отправлено агентам')
      qc.invalidateQueries({ queryKey: ['blocked-apps'] })
      setOpen(false)
      form.resetFields()
    },
    onError: () => message.error('Ошибка добавления (возможно, паттерн уже существует)'),
  })

  const delMut = useMutation({
    mutationFn: deleteBlockedApp,
    onSuccess: () => {
      message.success('Удалено')
      qc.invalidateQueries({ queryKey: ['blocked-apps'] })
    },
  })

  return (
    <>
      <div style={{ display: 'flex', justifyContent: 'flex-end', marginBottom: 12 }}>
        <Button type="primary" icon={<PlusOutlined />} onClick={() => setOpen(true)}>
          Заблокировать приложение
        </Button>
      </div>
      <Table
        dataSource={apps}
        rowKey="id"
        loading={isLoading}
        size="small"
        pagination={{ pageSize: 25 }}
        columns={[
          { title: 'Паттерн процесса / пути (.exe)', dataIndex: 'pattern', render: (v: string) => <code>{v}</code> },
          {
            title: 'Действие', dataIndex: 'action', width: 110,
            render: (v: string) => <Tag color={v === 'block' ? 'red' : 'orange'}>{v}</Tag>,
          },
          { title: 'Причина', dataIndex: 'reason', ellipsis: true },
          {
            title: 'Добавлено', dataIndex: 'created_at', width: 130,
            render: (v: string) => dayjs(v).format('DD.MM.YYYY'),
          },
          {
            title: '', key: 'del', width: 55,
            render: (_: unknown, rec: BlockedApp) => (
              <Popconfirm title="Удалить правило?" onConfirm={() => delMut.mutate(rec.id)}>
                <Button size="small" danger icon={<DeleteOutlined />} />
              </Popconfirm>
            ),
          },
        ]}
      />
      <Modal
        title="Запретить запуск приложения"
        open={open}
        onCancel={() => setOpen(false)}
        onOk={() => form.submit()}
        confirmLoading={addMut.isPending}
      >
        <Form form={form} layout="vertical" onFinish={(d) => addMut.mutate(d)}>
          <Form.Item name="pattern" label="Имя процесса или часть пути" rules={[{ required: true }]}>
            <Input placeholder="Например: utorrent.exe, anydesk.exe, steam.exe" />
          </Form.Item>
          <Form.Item name="action" label="Действие" initialValue="block" rules={[{ required: true }]}>
            <Select options={[{ value: 'block', label: 'Завершить процесс (block)' }, { value: 'alert', label: 'Только алерт (alert)' }]} />
          </Form.Item>
          <Form.Item name="reason" label="Причина">
            <Input placeholder="Запрещено политикой ИБ" />
          </Form.Item>
        </Form>
      </Modal>
    </>
  )
}

function SiemTab() {
  const qc = useQueryClient()
  const [form] = Form.useForm()
  const [testResult, setTestResult] = useState<{ webhook: string; syslog: string; telegram: string } | null>(null)

  const { data, isLoading } = useQuery({
    queryKey: ['siem-settings'],
    queryFn: fetchSiemSettings,
  })

  const saveMut = useMutation({
    mutationFn: (vals: SiemSettings & { webhook_token?: string; telegram_token?: string }) =>
      updateSiemSettings({
        enabled: !!vals.enabled,
        webhook_url: vals.webhook_url || '',
        webhook_token: vals.webhook_token || undefined,
        syslog_host: vals.syslog_host || '',
        syslog_port: Number(vals.syslog_port || 514),
        syslog_proto: vals.syslog_proto || 'udp',
        telegram_enabled: !!vals.telegram_enabled,
        telegram_token: vals.telegram_token || undefined,
        telegram_chat_id: vals.telegram_chat_id || '',
        min_severity: vals.min_severity || 'low',
      }),
    onSuccess: () => {
      message.success('Настройки SIEM и оповещений сохранены')
      qc.invalidateQueries({ queryKey: ['siem-settings'] })
    },
    onError: () => message.error('Ошибка сохранения настроек SIEM'),
  })

  const testMut = useMutation({
    mutationFn: testSiemDelivery,
    onSuccess: (res) => {
      setTestResult(res)
      message.info('Тестовое событие отправлено')
    },
    onError: () => message.error('Ошибка отправки тестового события'),
  })

  if (isLoading && !data) {
    return <Card loading />
  }

  return (
    <Form
      form={form}
      layout="vertical"
      initialValues={data}
      onFinish={(v) => saveMut.mutate(v)}
      style={{ maxWidth: 680 }}
    >
      <Space style={{ marginBottom: 16 }}>
        <Form.Item name="enabled" label="SIEM-коннектор (Webhook / Syslog CEF)" valuePropName="checked" style={{ marginBottom: 0 }}>
          <Switch />
        </Form.Item>
        <Form.Item name="min_severity" label="Мин. уровень инцидента" style={{ marginBottom: 0, width: 180 }}>
          <Select options={SEVERITIES} />
        </Form.Item>
      </Space>

      <Form.Item name="webhook_url" label="SIEM JSON Webhook URL">
        <Input placeholder="https://siem.company.kz/api/events" />
      </Form.Item>
      <Form.Item
        name="webhook_token"
        label={`Bearer-токен Webhook ${data?.has_webhook_token ? '(установлен — оставьте пустым, чтобы не менять)' : ''}`}
      >
        <Input.Password placeholder="Опционально" />
      </Form.Item>

      <Space size={12} style={{ display: 'flex' }}>
        <Form.Item name="syslog_host" label="Syslog / ArcSight CEF Host" style={{ flex: 1, minWidth: 260 }}>
          <Input placeholder="10.10.0.50" />
        </Form.Item>
        <Form.Item name="syslog_port" label="Порт" style={{ width: 110 }}>
          <Input type="number" min={1} max={65535} />
        </Form.Item>
        <Form.Item name="syslog_proto" label="Протокол" style={{ width: 120 }}>
          <Select options={[{ value: 'udp', label: 'UDP' }, { value: 'tcp', label: 'TCP (RFC 5425)' }]} />
        </Form.Item>
      </Space>

      <Form.Item name="telegram_enabled" label="Оповещения в Telegram" valuePropName="checked">
        <Switch />
      </Form.Item>
      <Space size={12} style={{ display: 'flex' }}>
        <Form.Item
          name="telegram_token"
          label={`Bot Token ${data?.has_telegram_token ? '(установлен)' : ''}`}
          style={{ flex: 1, minWidth: 260 }}
        >
          <Input.Password placeholder="123456:ABC..." />
        </Form.Item>
        <Form.Item name="telegram_chat_id" label="Chat ID" style={{ width: 200 }}>
          <Input placeholder="-100..." />
        </Form.Item>
      </Space>

      {testResult && (
        <Card size="small" style={{ marginBottom: 16 }}>
          <Space direction="vertical" size={4}>
            <span>Webhook: <Tag>{testResult.webhook}</Tag></span>
            <span>Syslog CEF: <Tag>{testResult.syslog}</Tag></span>
            <span>Telegram: <Tag>{testResult.telegram}</Tag></span>
          </Space>
        </Card>
      )}

      <Space>
        <Button type="primary" htmlType="submit" loading={saveMut.isPending}>
          Сохранить
        </Button>
        <Button onClick={() => testMut.mutate()} loading={testMut.isPending}>
          Отправить тестовое событие
        </Button>
      </Space>
    </Form>
  )
}

function AdDirectoryTab() {
  const qc = useQueryClient()
  const [form] = Form.useForm()

  const { data, isLoading } = useQuery({
    queryKey: ['ad-settings'],
    queryFn: fetchAdSettings,
  })
  const { data: adUsers = [] } = useQuery({
    queryKey: ['ad-users'],
    queryFn: () => fetchAdUsers(),
  })

  const saveMut = useMutation({
    mutationFn: (vals: Partial<AdSettings>) => updateAdSettings(vals),
    onSuccess: () => {
      message.success('Настройки Active Directory / LDAP сохранены')
      qc.invalidateQueries({ queryKey: ['ad-settings'] })
    },
    onError: () => message.error('Ошибка сохранения настроек AD/LDAP'),
  })

  if (isLoading && !data) return <Card loading />

  return (
    <div>
      <Form
        form={form}
        layout="vertical"
        initialValues={data}
        onFinish={(v) => saveMut.mutate(v)}
        style={{ maxWidth: 680, marginBottom: 24 }}
      >
        <Form.Item name="enabled" label="Интеграция с Active Directory / LDAP" valuePropName="checked">
          <Switch />
        </Form.Item>
        <Space size={12} style={{ display: 'flex' }}>
          <Form.Item name="server_url" label="LDAP / LDAPS URL" style={{ flex: 1, minWidth: 260 }}>
            <Input placeholder="ldaps://dc01.company.kz:636" />
          </Form.Item>
          <Form.Item name="sync_interval_min" label="Интервал (мин)" style={{ width: 140 }}>
            <Input type="number" min={5} max={1440} />
          </Form.Item>
        </Space>
        <Form.Item name="base_dn" label="Base DN">
          <Input placeholder="DC=company,DC=kz" />
        </Form.Item>
        <Form.Item name="bind_dn" label="Bind DN (сервисная учётная запись)">
          <Input placeholder="CN=svc_qorgau,OU=ServiceAccounts,DC=company,DC=kz" />
        </Form.Item>
        <Form.Item name="corporate_domains" label="Корпоративные почтовые домены (через запятую)">
          <Input placeholder="company.kz, subsidiary.kz" />
        </Form.Item>
        <Space>
          <Button type="primary" htmlType="submit" loading={saveMut.isPending}>
            Сохранить настройки AD
          </Button>
          {data?.last_sync_at && (
            <Tag color="blue">
              Последняя синхронизация: {dayjs(data.last_sync_at).format('DD.MM.YY HH:mm')} ({data.synced_count} польз.)
            </Tag>
          )}
        </Space>
      </Form>

      <Table<AdUser>
        dataSource={adUsers}
        rowKey="username"
        size="small"
        pagination={{ pageSize: 15 }}
        locale={{ emptyText: 'Пользователи AD ещё не импортированы (POST /api/v1/directory/import)' }}
        columns={[
          { title: 'Логин', dataIndex: 'username', width: 140 },
          { title: 'ФИО', dataIndex: 'full_name', width: 200 },
          { title: 'Отдел', dataIndex: 'department', width: 160 },
          { title: 'Должность', dataIndex: 'title', width: 160 },
          { title: 'Email', dataIndex: 'email', ellipsis: true },
        ]}
      />
    </div>
  )
}

export default function SettingsPage() {
  const { user } = useAuth()
  const isAdmin = user?.role === 'admin'

  const tabItems = [
    { key: 'policies', label: 'DLP Политики', children: <PoliciesTab /> },
    { key: 'domains', label: 'Заблокированные домены', children: <DomainsTab /> },
    { key: 'apps', label: 'Запрещённые приложения', children: <BlockedAppsTab /> },
    ...(isAdmin ? [{ key: 'siem', label: 'SIEM и Оповещения', children: <SiemTab /> }] : []),
    ...(isAdmin ? [{ key: 'ad', label: 'Active Directory / LDAP', children: <AdDirectoryTab /> }] : []),
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
