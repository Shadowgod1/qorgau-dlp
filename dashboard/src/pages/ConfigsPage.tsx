import { useEffect, useState } from 'react'
import {
  Button, Card, Col, Divider, Form, Input, InputNumber, Modal,
  Row, Space, Switch, Table, Tag, Tooltip, Typography, Popconfirm, Select, message,
} from 'antd'
import {
  PlusOutlined, EditOutlined, DeleteOutlined, UsergroupAddOutlined,
} from '@ant-design/icons'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import {
  fetchConfigs, fetchAgents, createConfig, updateConfig, deleteConfig,
  assignConfig, unassignConfig, AgentConfig,
} from '../api/client'
import type { Agent } from '../api/types'

const { Title, Text } = Typography

const BOOL_MODULES: { key: keyof AgentConfig; label: string }[] = [
  { key: 'mod_sessions',    label: 'Сессии (вход/выход)' },
  { key: 'mod_apps',        label: 'Активные приложения' },
  { key: 'mod_web',         label: 'Веб-активность' },
  { key: 'mod_messenger',   label: 'Мессенджеры' },
  { key: 'mod_clipboard',   label: 'Буфер обмена' },
  { key: 'mod_network',     label: 'Сетевые соединения' },
  { key: 'mod_usb',         label: 'USB-устройства' },
  { key: 'mod_printer',     label: 'Принтер / печать' },
  { key: 'mod_screenshots', label: 'Скриншоты' },
  { key: 'mod_microphone',  label: '🎙️ Микрофон (запись аудио)' },
  { key: 'mod_webcam',      label: '📷 Веб-камера (снимки + стрим)' },
]

const NET_TOGGLES: { key: keyof AgentConfig; label: string }[] = [
  { key: 'net_alert_sensitive_ports', label: 'Алерт на RDP/SSH/SMB порты' },
  { key: 'net_alert_unknown_ext',     label: 'Алерт на неизвестные внешние IP' },
  { key: 'net_block_tor',             label: 'Алерт при Tor-соединении' },
]

const WEB_TOGGLES: { key: keyof AgentConfig; label: string }[] = [
  { key: 'web_alert_social',    label: 'Социальные сети' },
  { key: 'web_alert_adult',     label: 'Контент 18+' },
  { key: 'web_alert_gambling',  label: 'Азартные игры' },
  { key: 'web_alert_streaming', label: 'Стриминг / видео' },
  { key: 'web_alert_crypto',    label: 'Криптовалюта' },
]

const USB_TOGGLES: { key: keyof AgentConfig; label: string }[] = [
  { key: 'usb_alert_on_connect',  label: 'Алерт при подключении USB' },
  { key: 'usb_analyze_content',   label: 'Анализировать файлы на USB' },
  { key: 'usb_alert_executables', label: 'Алерт при наличии .exe файлов' },
]

const DEFAULT_VALUES: Partial<AgentConfig> = {
  mod_sessions: true, mod_apps: true, mod_web: true, mod_messenger: true,
  mod_clipboard: true, mod_network: true, mod_usb: true, mod_printer: true,
  mod_screenshots: true, mod_microphone: false, mod_webcam: false,
  screenshot_interval: 60, screenshot_quality: 60, screenshot_max_w: 1920,
  microphone_duration: 30, microphone_sample_rate: 16000,
  webcam_interval: 120, webcam_quality: 60, webcam_max_w: 1280,
  net_alert_sensitive_ports: true, net_alert_unknown_ext: false, net_block_tor: true,
  web_alert_social: false, web_alert_adult: true, web_alert_gambling: true,
  web_alert_streaming: false, web_alert_crypto: false,
  usb_alert_on_connect: true, usb_analyze_content: true, usb_alert_executables: true,
}

function TogglesSection({ title, toggles }: {
  title: string
  toggles: { key: keyof AgentConfig; label: string }[]
}) {
  return (
    <>
      <Divider orientation="left" style={{ fontSize: 12, margin: '12px 0 8px' }}>{title}</Divider>
      {toggles.map(t => (
        <div key={String(t.key)} style={{ display: 'flex', alignItems: 'center', gap: 10, marginBottom: 6 }}>
          <Form.Item name={t.key} valuePropName="checked" noStyle>
            <Switch size="small" checkedChildren="ON" unCheckedChildren="OFF" />
          </Form.Item>
          <Text style={{ fontSize: 13 }}>{t.label}</Text>
        </div>
      ))}
    </>
  )
}

function ConfigModal({
  open, initial, onClose,
}: {
  open: boolean
  initial: Partial<AgentConfig> | null
  onClose: () => void
}) {
  const [form] = Form.useForm()
  const qc = useQueryClient()
  const isEdit = !!initial?.id

  useEffect(() => {
    if (open) {
      form.resetFields()
      form.setFieldsValue(initial ? { ...DEFAULT_VALUES, ...initial } : DEFAULT_VALUES)
    }
  }, [open, initial, form])

  const save = useMutation({
    mutationFn: (vals: Partial<AgentConfig>) =>
      isEdit ? updateConfig(initial!.id!, vals) : createConfig(vals),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['configs'] })
      message.success(isEdit ? 'Конфигурация обновлена' : 'Конфигурация создана')
      onClose()
    },
    onError: () => message.error('Ошибка сохранения'),
  })

  return (
    <Modal
      open={open}
      title={isEdit ? 'Редактировать конфигурацию' : 'Новая конфигурация'}
      width={600}
      onCancel={onClose}
      onOk={() => form.submit()}
      okText="Сохранить"
      cancelText="Отмена"
      confirmLoading={save.isPending}
      destroyOnClose
    >
      <Form
        form={form}
        layout="vertical"
        initialValues={initial ?? DEFAULT_VALUES}
        onFinish={save.mutate}
        style={{ maxHeight: '65vh', overflowY: 'auto', paddingRight: 8 }}
      >
        <Form.Item name="name" label="Название" rules={[{ required: true }]}>
          <Input placeholder="Например: Бухгалтерия" />
        </Form.Item>
        <Form.Item name="description" label="Описание">
          <Input.TextArea rows={2} />
        </Form.Item>

        <TogglesSection title="Модули мониторинга" toggles={BOOL_MODULES} />

        <Divider orientation="left" style={{ fontSize: 12, margin: '12px 0 8px' }}>Скриншоты</Divider>
        <Row gutter={12}>
          <Col span={8}>
            <Form.Item name="screenshot_interval" label="Интервал (сек)">
              <InputNumber min={10} max={3600} style={{ width: '100%' }} />
            </Form.Item>
          </Col>
          <Col span={8}>
            <Form.Item name="screenshot_quality" label="Качество (1-95)">
              <InputNumber min={1} max={95} style={{ width: '100%' }} />
            </Form.Item>
          </Col>
          <Col span={8}>
            <Form.Item name="screenshot_max_w" label="Макс. ширина px">
              <InputNumber min={800} max={3840} style={{ width: '100%' }} />
            </Form.Item>
          </Col>
        </Row>

        <Divider orientation="left" style={{ fontSize: 12, margin: '12px 0 8px' }}>🎙️ Микрофон</Divider>
        <Row gutter={12}>
          <Col span={12}>
            <Form.Item name="microphone_duration" label="Длительность фрагмента (сек)">
              <InputNumber min={5} max={300} style={{ width: '100%' }} />
            </Form.Item>
          </Col>
          <Col span={12}>
            <Form.Item name="microphone_sample_rate" label="Частота (Hz)">
              <InputNumber min={8000} max={48000} step={8000} style={{ width: '100%' }} />
            </Form.Item>
          </Col>
        </Row>

        <Divider orientation="left" style={{ fontSize: 12, margin: '12px 0 8px' }}>📷 Веб-камера</Divider>
        <Row gutter={12}>
          <Col span={8}>
            <Form.Item name="webcam_interval" label="Интервал снимков (сек)">
              <InputNumber min={10} max={3600} style={{ width: '100%' }} />
            </Form.Item>
          </Col>
          <Col span={8}>
            <Form.Item name="webcam_quality" label="Качество JPEG (1-95)">
              <InputNumber min={1} max={95} style={{ width: '100%' }} />
            </Form.Item>
          </Col>
          <Col span={8}>
            <Form.Item name="webcam_max_w" label="Макс. ширина px">
              <InputNumber min={320} max={3840} style={{ width: '100%' }} />
            </Form.Item>
          </Col>
        </Row>

        <TogglesSection title="Сетевые алерты" toggles={NET_TOGGLES} />
        <TogglesSection title="Веб-фильтрация (алерты)" toggles={WEB_TOGGLES} />
        <TogglesSection title="USB-политика" toggles={USB_TOGGLES} />
      </Form>
    </Modal>
  )
}

function AssignModal({
  config, onClose, agents,
}: {
  config: AgentConfig | null
  onClose: () => void
  agents: Agent[]
}) {
  const qc = useQueryClient()
  const [selected, setSelected] = useState<string[]>([])

  useEffect(() => {
    if (config) {
      setSelected(config.agents?.map(a => a.agent_id) ?? [])
    }
  }, [config])

  const assign = useMutation({
    mutationFn: async () => {
      const current = config?.agents?.map(a => a.agent_id) ?? []
      const toAdd = selected.filter(id => !current.includes(id))
      const toRemove = current.filter(id => !selected.includes(id))
      if (toAdd.length > 0) await assignConfig(config!.id, toAdd)
      if (toRemove.length > 0) await unassignConfig(config!.id, toRemove)
    },
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['configs'] })
      message.success('Назначение обновлено')
      onClose()
    },
    onError: () => message.error('Ошибка назначения'),
  })

  return (
    <Modal
      open={!!config}
      title={`Назначить агентов — ${config?.name}`}
      onCancel={onClose}
      onOk={() => assign.mutate()}
      okText="Применить"
      cancelText="Отмена"
      confirmLoading={assign.isPending}
      destroyOnClose
    >
      <Select
        mode="multiple"
        style={{ width: '100%' }}
        placeholder="Выберите агентов"
        value={selected}
        onChange={setSelected}
        options={agents.map(a => ({
          value: a.agent_id,
          label: `${a.hostname} (${a.agent_id})`,
        }))}
        filterOption={(input, opt) =>
          (opt?.label as string ?? '').toLowerCase().includes(input.toLowerCase())
        }
      />
      <Text type="secondary" style={{ fontSize: 12, display: 'block', marginTop: 8 }}>
        Выберите компьютеры, которым будет применена эта конфигурация.
        Предыдущая конфигурация на выбранных машинах будет заменена.
      </Text>
    </Modal>
  )
}

export default function ConfigsPage() {
  const qc = useQueryClient()
  const [editTarget, setEditTarget] = useState<Partial<AgentConfig> | null>(null)
  const [modalOpen, setModalOpen] = useState(false)
  const [assignTarget, setAssignTarget] = useState<AgentConfig | null>(null)

  const { data: configs = [], isLoading } = useQuery({
    queryKey: ['configs'],
    queryFn: fetchConfigs,
  })
  const { data: agents = [] } = useQuery({
    queryKey: ['agents'],
    queryFn: fetchAgents,
  })

  const del = useMutation({
    mutationFn: (id: number) => deleteConfig(id),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['configs'] })
      message.success('Конфигурация удалена')
    },
  })

  const openCreate = () => { setEditTarget(null); setModalOpen(true) }
  const openEdit = (c: AgentConfig) => { setEditTarget(c); setModalOpen(true) }
  const closeModal = () => setModalOpen(false)

  const columns = [
    {
      title: 'Название',
      dataIndex: 'name',
      key: 'name',
      render: (v: string, r: AgentConfig) => (
        <Space direction="vertical" size={2}>
          <Text strong>{v}</Text>
          {r.description && <Text type="secondary" style={{ fontSize: 12 }}>{r.description}</Text>}
        </Space>
      ),
    },
    {
      title: 'Агенты',
      dataIndex: 'agent_count',
      key: 'agent_count',
      width: 100,
      render: (v: number) => <Tag color={v > 0 ? 'blue' : 'default'}>{v} агент{v === 1 ? '' : 'а'}</Tag>,
    },
    {
      title: 'Активные модули',
      key: 'modules',
      render: (_: unknown, r: AgentConfig) => {
        const active = BOOL_MODULES.filter(m => r[m.key] === true)
        return (
          <Space wrap size={4}>
            {active.map(m => (
              <Tag key={String(m.key)} color="green" style={{ fontSize: 11 }}>
                {m.label.split(' ')[0]}
              </Tag>
            ))}
          </Space>
        )
      },
    },
    {
      title: 'Создана',
      dataIndex: 'created_at',
      key: 'created_at',
      width: 130,
      render: (v: string) => v ? new Date(v).toLocaleDateString('ru-RU') : '—',
    },
    {
      title: '',
      key: 'actions',
      width: 130,
      render: (_: unknown, r: AgentConfig) => (
        <Space>
          <Tooltip title="Назначить агентам">
            <Button size="small" icon={<UsergroupAddOutlined />} onClick={() => setAssignTarget(r)} />
          </Tooltip>
          <Tooltip title="Редактировать">
            <Button size="small" icon={<EditOutlined />} onClick={() => openEdit(r)} />
          </Tooltip>
          <Popconfirm
            title="Удалить конфигурацию?"
            description="Агенты перейдут на дефолтную конфигурацию."
            onConfirm={() => del.mutate(r.id)}
            okText="Удалить"
            cancelText="Отмена"
            okButtonProps={{ danger: true }}
          >
            <Button size="small" danger icon={<DeleteOutlined />} />
          </Popconfirm>
        </Space>
      ),
    },
  ]

  return (
    <div>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 20 }}>
        <div>
          <Title level={4} style={{ margin: 0 }}>Конфигурации агентов</Title>
          <Text type="secondary" style={{ fontSize: 13 }}>
            Настройте модули мониторинга для разных групп компьютеров
          </Text>
        </div>
        <Button type="primary" icon={<PlusOutlined />} onClick={openCreate}>
          Новая конфигурация
        </Button>
      </div>

      <Card>
        <Table
          columns={columns}
          dataSource={configs}
          rowKey="id"
          loading={isLoading}
          pagination={false}
        />
      </Card>

      <ConfigModal
        open={modalOpen}
        initial={editTarget}
        onClose={closeModal}
      />
      <AssignModal
        config={assignTarget}
        onClose={() => setAssignTarget(null)}
        agents={agents}
      />
    </div>
  )
}
