import { useEffect, useState } from 'react'
import {
  Button, Card, Col, Divider, Form, Input, InputNumber, Modal,
  Row, Space, Switch, Table, Tag, Tooltip, Typography, Popconfirm, Select,
  Segmented, Tabs, Alert, message,
} from 'antd'
import {
  PlusOutlined, EditOutlined, DeleteOutlined, UsergroupAddOutlined,
  CopyOutlined, AppstoreOutlined, UnorderedListOutlined, SafetyCertificateOutlined,
  ThunderboltOutlined, LockOutlined, ClockCircleOutlined, GlobalOutlined,
  CheckCircleOutlined,
} from '@ant-design/icons'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import {
  fetchConfigs, fetchAgents, createConfig, updateConfig, deleteConfig,
  assignConfig, unassignConfig, AgentConfig,
} from '../api/client'
import type { Agent } from '../api/types'

const { Title, Text } = Typography

const CORE_MODULES: { key: keyof AgentConfig; label: string; desc: string }[] = [
  { key: 'mod_sessions',     label: 'Сессии входа / выхода',       desc: 'Фиксация logon/logoff и блокировки экрана' },
  { key: 'mod_apps',         label: 'Активные приложения',         desc: 'Учёт времени работы в программах и заголовков окон' },
  { key: 'mod_web',          label: 'Веб-активность',              desc: 'История браузеров Chrome, Edge и Firefox' },
  { key: 'mod_messenger',    label: 'Мессенджеры',                 desc: 'Контроль активности и перехват в мессенджерах' },
  { key: 'mod_clipboard',    label: 'Буфер обмена',                desc: 'Анализ скопированного текста на ПДн и ИИН/БИН' },
  { key: 'mod_network',      label: 'Сетевые соединения',          desc: 'Мониторинг TCP-портов, RDP/SSH/SMB и Tor' },
  { key: 'mod_file_monitor', label: 'Файловые операции (FileModule)', desc: 'Отслеживание создания и изменения файлов + SHA-256' },
  { key: 'mod_printer',      label: 'Контроль печати',             desc: 'Аудит заданий печати на локальных и сетевых принтерах' },
  { key: 'mod_inventory',    label: 'Инвентаризация ПО и железа',  desc: 'Сбор CPU/RAM/дисков и списка установленных программ' },
]

const PREVENTION_MODULES: { key: keyof AgentConfig; label: string; desc: string }[] = [
  { key: 'mod_usb',            label: 'Мониторинг USB-носителей',     desc: 'Чтение VID/PID/Serial флешек и анализ файлов' },
  { key: 'usb_whitelist_mode', label: 'Белый список ВНИ (Eject)',     desc: 'Принудительно извлекать любые неразрешённые флешки' },
  { key: 'mod_app_blocker',    label: 'Блокировка приложений',        desc: 'Принудительно завершать запрещённые процессы' },
  { key: 'mod_web_blocker',    label: 'Блокировка сайтов (hosts)',    desc: 'Синхронизация чёрного списка доменов в hosts' },
]

const NET_TOGGLES: { key: keyof AgentConfig; label: string }[] = [
  { key: 'net_alert_sensitive_ports', label: 'Алерт на RDP / SSH / SMB порты' },
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
  { key: 'usb_analyze_content',   label: 'Анализировать содержимое файлов на USB' },
  { key: 'usb_alert_executables', label: 'Алерт при наличии исполняемых (.exe) файлов' },
]

const DEFAULT_VALUES: Partial<AgentConfig> = {
  mod_sessions: true, mod_apps: true, mod_web: true, mod_messenger: true,
  mod_clipboard: true, mod_network: true, mod_usb: true, mod_file_monitor: true,
  mod_discovery: false, discovery_paths: '', mod_inventory: true,
  mod_app_blocker: false, mod_web_blocker: false,
  mod_printer: true, mod_screenshots: true, mod_watermark: false,
  mod_microphone: false, mod_webcam: false,
  screenshot_interval: 60, screenshot_quality: 60, screenshot_max_w: 1920,
  microphone_duration: 30, microphone_sample_rate: 16000,
  webcam_interval: 120, webcam_quality: 60, webcam_max_w: 1280,
  watermark_opacity: 25, watermark_text: '{username} • {hostname} • {time}', watermark_apps: '',
  net_alert_sensitive_ports: true, net_alert_unknown_ext: false, net_block_tor: true,
  web_alert_social: false, web_alert_adult: true, web_alert_gambling: true,
  web_alert_streaming: false, web_alert_crypto: false,
  usb_alert_on_connect: true, usb_analyze_content: true, usb_alert_executables: true,
  usb_whitelist_mode: false,
}

interface StandardTemplate {
  key: string
  name: string
  description: string
  icon: React.ReactNode
  values: Partial<AgentConfig>
}

const STANDARD_TEMPLATES: StandardTemplate[] = [
  {
    key: 'office',
    name: 'Стандартный (Офис)',
    icon: <SafetyCertificateOutlined />,
    description: 'Сбалансированный аудит каналов утечек, скриншоты 60с, файловый мониторинг и инвентаризация ПО без жёстких блокировок.',
    values: { ...DEFAULT_VALUES, screenshot_interval: 60, mod_watermark: false, usb_whitelist_mode: false },
  },
  {
    key: 'finance',
    name: 'Усиленный ИБ (Бухгалтерия / Финансы)',
    icon: <LockOutlined />,
    description: 'Скриншоты 30с, водяные знаки на экране, белый список USB (ВНИ), краулер ПДн и блокировка сайтов.',
    values: {
      ...DEFAULT_VALUES,
      screenshot_interval: 30, screenshot_quality: 65,
      mod_watermark: true, watermark_opacity: 26,
      usb_whitelist_mode: true, mod_discovery: true, mod_web_blocker: true,
      net_alert_unknown_ext: true, web_alert_social: true, web_alert_crypto: true,
    },
  },
  {
    key: 'strict',
    name: 'Максимальная защита (Гриф ДСП)',
    icon: <ThunderboltOutlined />,
    description: 'Активная блокировка ПО и сайтов, белый список ВНИ, заметные водяные знаки (32%), скриншоты 20с и краулер.',
    values: {
      ...DEFAULT_VALUES,
      screenshot_interval: 20, screenshot_quality: 70,
      mod_watermark: true, watermark_opacity: 32,
      watermark_text: 'КОНФИДЕНЦИАЛЬНО • {username} • {hostname} • {time}',
      usb_whitelist_mode: true, mod_discovery: true,
      mod_app_blocker: true, mod_web_blocker: true,
      net_alert_unknown_ext: true, web_alert_social: true, web_alert_streaming: true, web_alert_crypto: true,
    },
  },
  {
    key: 'urv',
    name: 'Только УРВ (Учёт рабочего времени)',
    icon: <ClockCircleOutlined />,
    description: 'Только сессии, приложения, веб-активность и инвентаризация. Без скриншотов, буфера и перехвата файлов.',
    values: {
      ...DEFAULT_VALUES,
      mod_sessions: true, mod_apps: true, mod_web: true, mod_inventory: true,
      mod_messenger: false, mod_clipboard: false, mod_network: false, mod_usb: false,
      mod_file_monitor: false, mod_printer: false, mod_screenshots: false, mod_watermark: false,
      mod_discovery: false, mod_app_blocker: false, mod_web_blocker: false,
      net_alert_sensitive_ports: false, net_block_tor: false,
      usb_alert_on_connect: false, usb_analyze_content: false, usb_alert_executables: false,
    },
  },
  {
    key: 'remote',
    name: 'Удалённый сотрудник (VPN / Home Office)',
    icon: <GlobalOutlined />,
    description: 'Водяные знаки от фото экрана дома, контроль мессенджеров, файлов и Tor/RDP без извлечения домашних USB.',
    values: {
      ...DEFAULT_VALUES,
      screenshot_interval: 45, screenshot_quality: 55, screenshot_max_w: 1600,
      mod_watermark: true, watermark_opacity: 22,
      watermark_text: 'REMOTE • {username} • {hostname} • {time}',
      mod_web_blocker: true, usb_whitelist_mode: false,
      net_alert_unknown_ext: true, web_alert_crypto: true,
    },
  },
]

function getFeatureBadges(c: AgentConfig): { label: string; color?: string }[] {
  const list: { label: string; color?: string }[] = []
  if (c.mod_screenshots) list.push({ label: `Экран ${c.screenshot_interval}с` })
  if (c.mod_watermark) list.push({ label: `Водяной знак ${c.watermark_opacity}%`, color: 'blue' })
  if (c.usb_whitelist_mode) list.push({ label: 'USB Whitelist', color: 'red' })
  else if (c.mod_usb) list.push({ label: 'USB аудит' })
  if (c.mod_app_blocker) list.push({ label: 'Блок ПО', color: 'red' })
  if (c.mod_web_blocker) list.push({ label: 'Блок сайтов', color: 'gold' })
  if (c.mod_discovery) list.push({ label: 'Краулер ПДн', color: 'blue' })
  if (c.mod_file_monitor) list.push({ label: 'Файлы' })
  if (c.mod_clipboard) list.push({ label: 'Буфер' })
  if (c.mod_inventory) list.push({ label: 'Инвентаризация' })
  if (c.mod_microphone) list.push({ label: 'Микрофон', color: 'gold' })
  if (c.mod_webcam) list.push({ label: 'Камера', color: 'gold' })
  return list
}

function ModuleSwitchList({ items }: { items: { key: keyof AgentConfig; label: string; desc?: string }[] }) {
  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
      {items.map(t => (
        <div
          key={String(t.key)}
          style={{
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'space-between',
            padding: '10px 12px',
            borderRadius: 6,
            border: '1px solid var(--p-hairline)',
            gap: 12,
          }}
        >
          <div>
            <div style={{ fontWeight: 500, fontSize: 13.5 }}>{t.label}</div>
            {t.desc && <div style={{ fontSize: 12, color: 'var(--p-muted)' }}>{t.desc}</div>}
          </div>
          <Form.Item name={t.key} valuePropName="checked" noStyle>
            <Switch />
          </Form.Item>
        </div>
      ))}
    </div>
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
      message.success(isEdit ? 'Профиль обновлён и разослан агентам' : 'Профиль создан')
      onClose()
    },
    onError: () => message.error('Ошибка сохранения (проверьте уникальность названия)'),
  })

  return (
    <Modal
      open={open}
      title={isEdit ? `Редактирование: ${initial?.name}` : 'Новый профиль конфигурации'}
      width={720}
      onCancel={onClose}
      onOk={() => form.submit()}
      okText="Сохранить профиль"
      cancelText="Отмена"
      confirmLoading={save.isPending}
      destroyOnClose
    >
      <Form
        form={form}
        layout="vertical"
        initialValues={initial ?? DEFAULT_VALUES}
        onFinish={save.mutate}
      >
        <Tabs
          defaultActiveKey="general"
          items={[
            {
              key: 'general',
              label: 'Основное и шаблоны',
              forceRender: true,
              children: (
                <div>
                  <Text type="secondary" style={{ fontSize: 12.5, display: 'block', marginBottom: 8 }}>
                    Заполнить форму по стандартному шаблону:
                  </Text>
                  <Space wrap size={6} style={{ marginBottom: 16 }}>
                    {STANDARD_TEMPLATES.map(tpl => (
                      <Button
                        key={tpl.key}
                        size="small"
                        icon={tpl.icon}
                        onClick={() => {
                          form.setFieldsValue({
                            ...tpl.values,
                            name: form.getFieldValue('name') || tpl.name,
                            description: form.getFieldValue('description') || tpl.description,
                          })
                          message.info(`Шаблон «${tpl.name}» применён`)
                        }}
                      >
                        {tpl.name.split(' ')[0]}
                      </Button>
                    ))}
                  </Space>

                  <Form.Item name="name" label="Название профиля" rules={[{ required: true, message: 'Укажите название' }]}>
                    <Input placeholder="Например: Бухгалтерия — Алматы" />
                  </Form.Item>
                  <Form.Item name="description" label="Описание">
                    <Input.TextArea rows={3} placeholder="Для каких подразделений и задач предназначен этот профиль..." />
                  </Form.Item>
                </div>
              ),
            },
            {
              key: 'dlp',
              label: 'Модули DLP',
              forceRender: true,
              children: (
                <div style={{ maxHeight: 420, overflowY: 'auto', paddingRight: 4 }}>
                  <ModuleSwitchList items={CORE_MODULES} />
                  <Divider orientation="left" style={{ fontSize: 12.5 }}>Сетевые и веб-алерты</Divider>
                  <Row gutter={12}>
                    <Col span={12}><ModuleSwitchList items={NET_TOGGLES} /></Col>
                    <Col span={12}><ModuleSwitchList items={WEB_TOGGLES} /></Col>
                  </Row>
                </div>
              ),
            },
            {
              key: 'prevention',
              label: 'Активная защита',
              forceRender: true,
              children: (
                <div style={{ maxHeight: 420, overflowY: 'auto', paddingRight: 4 }}>
                  <ModuleSwitchList items={PREVENTION_MODULES} />
                  <Divider orientation="left" style={{ fontSize: 12.5 }}>Политика USB</Divider>
                  <ModuleSwitchList items={USB_TOGGLES} />
                </div>
              ),
            },
            {
              key: 'screen',
              label: 'Экран и водяные знаки',
              forceRender: true,
              children: (
                <div style={{ maxHeight: 420, overflowY: 'auto', paddingRight: 4 }}>
                  <ModuleSwitchList items={[
                    { key: 'mod_screenshots', label: 'Скриншоты экрана', desc: 'Регулярные снимки экрана с отправкой на OCR (RU/KZ/EN)' },
                    { key: 'mod_watermark', label: 'Водяные знаки поверх экрана', desc: 'Полупрозрачный защитный оверлей от фото экрана (не мешает кликам)' },
                  ]} />
                  <Divider orientation="left" style={{ fontSize: 12.5 }}>Параметры скриншотов</Divider>
                  <Row gutter={12}>
                    <Col span={8}>
                      <Form.Item name="screenshot_interval" label="Интервал (сек)">
                        <InputNumber min={10} max={3600} style={{ width: '100%' }} />
                      </Form.Item>
                    </Col>
                    <Col span={8}>
                      <Form.Item name="screenshot_quality" label="Качество JPEG">
                        <InputNumber min={1} max={95} style={{ width: '100%' }} />
                      </Form.Item>
                    </Col>
                    <Col span={8}>
                      <Form.Item name="screenshot_max_w" label="Макс. ширина (px)">
                        <InputNumber min={800} max={3840} style={{ width: '100%' }} />
                      </Form.Item>
                    </Col>
                  </Row>

                  <Divider orientation="left" style={{ fontSize: 12.5 }}>Параметры водяных знаков</Divider>
                  <Row gutter={12}>
                    <Col span={8}>
                      <Form.Item name="watermark_opacity" label="Прозрачность (5–80%)">
                        <InputNumber min={5} max={80} style={{ width: '100%' }} />
                      </Form.Item>
                    </Col>
                    <Col span={16}>
                      <Form.Item name="watermark_text" label="Шаблон ({username}, {hostname}, {time})">
                        <Input placeholder="{username} • {hostname} • {time}" />
                      </Form.Item>
                    </Col>
                  </Row>
                  <Form.Item
                    name="watermark_apps"
                    label="Условие показа (приложения через запятую; пусто = всегда)"
                  >
                    <Input placeholder="Например: excel, 1cv8, bitrix, chrome" />
                  </Form.Item>
                </div>
              ),
            },
            {
              key: 'advanced',
              label: 'Краулер и аудио/видео',
              forceRender: true,
              children: (
                <div style={{ maxHeight: 420, overflowY: 'auto', paddingRight: 4 }}>
                  <ModuleSwitchList items={[
                    { key: 'mod_discovery', label: 'Краулер ПДн (Data Discovery)', desc: 'Поиск файлов с ИИН/БИН, IBAN, картами и ДСП на дисках' },
                  ]} />
                  <Form.Item
                    name="discovery_paths"
                    label="Папки сканирования (через запятую; пусто = Desktop, Documents, Downloads)"
                    style={{ marginTop: 10 }}
                  >
                    <Input placeholder="Например: C:\Work, D:\Shared" />
                  </Form.Item>

                  <Divider orientation="left" style={{ fontSize: 12.5 }}>Контроль микрофона и камеры</Divider>
                  <Alert
                    type="warning"
                    showIcon
                    style={{ marginBottom: 12 }}
                    message="Требование ТК РК ст. 23"
                    description="Запись аудио и фото с веб-камеры отключены по умолчанию (fail-closed). Включайте только при наличии уведомления сотрудников и регламента ИБ."
                  />
                  <ModuleSwitchList items={[
                    { key: 'mod_microphone', label: 'Микрофон (запись аудиофрагментов)' },
                    { key: 'mod_webcam', label: 'Веб-камера (снимки и стрим)' },
                  ]} />
                  <Row gutter={12} style={{ marginTop: 12 }}>
                    <Col span={8}>
                      <Form.Item name="microphone_duration" label="Аудио фрагмент (сек)">
                        <InputNumber min={5} max={300} style={{ width: '100%' }} />
                      </Form.Item>
                    </Col>
                    <Col span={8}>
                      <Form.Item name="microphone_sample_rate" label="Частота (Hz)">
                        <InputNumber min={8000} max={48000} step={8000} style={{ width: '100%' }} />
                      </Form.Item>
                    </Col>
                    <Col span={8}>
                      <Form.Item name="webcam_interval" label="Интервал камеры (сек)">
                        <InputNumber min={10} max={3600} style={{ width: '100%' }} />
                      </Form.Item>
                    </Col>
                  </Row>
                  <Row gutter={12}>
                    <Col span={12}>
                      <Form.Item name="webcam_quality" label="Качество камеры (1–95)">
                        <InputNumber min={1} max={95} style={{ width: '100%' }} />
                      </Form.Item>
                    </Col>
                    <Col span={12}>
                      <Form.Item name="webcam_max_w" label="Макс. ширина камеры (px)">
                        <InputNumber min={320} max={3840} style={{ width: '100%' }} />
                      </Form.Item>
                    </Col>
                  </Row>
                </div>
              ),
            },
          ]}
        />
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
      qc.invalidateQueries({ queryKey: ['agents'] })
      message.success('Профиль назначен выбранным станциям')
      onClose()
    },
    onError: () => message.error('Ошибка назначения'),
  })

  return (
    <Modal
      open={!!config}
      title={`Назначить станции — ${config?.name}`}
      onCancel={onClose}
      onOk={() => assign.mutate()}
      okText="Применить профиль"
      cancelText="Отмена"
      confirmLoading={assign.isPending}
      destroyOnClose
    >
      <Select
        mode="multiple"
        style={{ width: '100%' }}
        placeholder="Выберите рабочие станции..."
        value={selected}
        onChange={setSelected}
        options={agents.map(a => ({
          value: a.agent_id,
          label: `${a.hostname} — ${a.username || '—'} (${a.is_online ? 'онлайн' : 'офлайн'})`,
        }))}
        filterOption={(input, opt) =>
          (opt?.label as string ?? '').toLowerCase().includes(input.toLowerCase())
        }
      />
      <Text type="secondary" style={{ fontSize: 12.5, display: 'block', marginTop: 10 }}>
        Выбранные ПК получат новый профиль по WebSocket без перезагрузки службы агента.
      </Text>
    </Modal>
  )
}

export default function ConfigsPage() {
  const qc = useQueryClient()
  const [viewMode, setViewMode] = useState<'cards' | 'table'>('cards')
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
      qc.invalidateQueries({ queryKey: ['agents'] })
      message.success('Конфигурация удалена')
    },
  })

  const quickCreate = useMutation({
    mutationFn: (tpl: StandardTemplate) =>
      createConfig({ ...tpl.values, name: tpl.name, description: tpl.description }),
    onSuccess: (_, tpl) => {
      qc.invalidateQueries({ queryKey: ['configs'] })
      message.success(`Стандартный профиль «${tpl.name}» добавлен`)
    },
    onError: () => message.warning('Конфигурация с таким названием уже существует'),
  })

  const openCreate = () => { setEditTarget(null); setModalOpen(true) }
  const openEdit = (c: AgentConfig) => { setEditTarget(c); setModalOpen(true) }
  const openDuplicate = (c: AgentConfig) => {
    const copy: Partial<AgentConfig> = { ...c, id: undefined, name: `${c.name} (копия)` }
    setEditTarget(copy)
    setModalOpen(true)
  }

  const existingNames = new Set(configs.map(c => c.name))
  const missingTemplates = STANDARD_TEMPLATES.filter(t => !existingNames.has(t.name))

  const columns = [
    {
      title: 'Профиль',
      dataIndex: 'name',
      key: 'name',
      render: (v: string, r: AgentConfig) => (
        <Space direction="vertical" size={2}>
          <Space size={8}>
            <Text strong>{v}</Text>
            {r.created_by === 'system' && <Tag bordered={false}>Стандартный</Tag>}
          </Space>
          {r.description && <Text type="secondary" style={{ fontSize: 12.5 }}>{r.description}</Text>}
        </Space>
      ),
    },
    {
      title: 'Станции',
      dataIndex: 'agent_count',
      key: 'agent_count',
      width: 95,
      render: (v: number) => <span className="qorgau-mono">{v ?? 0} ПК</span>,
    },
    {
      title: 'Активные модули',
      key: 'modules',
      render: (_: unknown, r: AgentConfig) => (
        <Space wrap size={4}>
          {getFeatureBadges(r).map(b => (
            <Tag key={b.label} color={b.color} bordered={false} style={{ fontSize: 11.5, margin: 0 }}>{b.label}</Tag>
          ))}
        </Space>
      ),
    },
    {
      title: 'Действия',
      key: 'actions',
      width: 155,
      render: (_: unknown, r: AgentConfig) => (
        <Space size={4}>
          <Tooltip title="Назначить агентам">
            <Button size="small" aria-label="Назначить агентам" icon={<UsergroupAddOutlined />} onClick={() => setAssignTarget(r)} />
          </Tooltip>
          <Tooltip title="Дублировать">
            <Button size="small" aria-label="Дублировать" icon={<CopyOutlined />} onClick={() => openDuplicate(r)} />
          </Tooltip>
          <Tooltip title="Редактировать">
            <Button size="small" aria-label="Редактировать" icon={<EditOutlined />} onClick={() => openEdit(r)} />
          </Tooltip>
          <Popconfirm title="Удалить профиль?" description="Назначенные ПК вернутся на профиль по умолчанию." onConfirm={() => del.mutate(r.id)}>
            <Button size="small" danger aria-label="Удалить" icon={<DeleteOutlined />} />
          </Popconfirm>
        </Space>
      ),
    },
  ]

  return (
    <div className="ts-enter">
      <div className="ts-page-head">
        <div>
          <Title level={2} className="ts-page-title">Конфигурации и стандартные профили</Title>
          <span className="ts-page-sub">
            5 готовых профилей защиты для подразделений и конструктор политик агента
          </span>
        </div>
        <Space size={8}>
          <Segmented
            value={viewMode}
            onChange={v => setViewMode(v as 'cards' | 'table')}
            options={[
              { value: 'cards', icon: <AppstoreOutlined />, label: 'Карточки' },
              { value: 'table', icon: <UnorderedListOutlined />, label: 'Таблица' },
            ]}
          />
          <Button type="primary" icon={<PlusOutlined />} onClick={openCreate}>
            Создать профиль
          </Button>
        </Space>
      </div>

      {missingTemplates.length > 0 && (
        <Card
          size="small"
          style={{ marginBottom: 18 }}
          title={<Space><CheckCircleOutlined /><span>Восстановить стандартный профиль</span></Space>}
        >
          <Space wrap size={8}>
            {missingTemplates.map(tpl => (
              <Button
                key={tpl.key}
                size="small"
                icon={tpl.icon}
                loading={quickCreate.isPending}
                onClick={() => quickCreate.mutate(tpl)}
              >
                + {tpl.name}
              </Button>
            ))}
          </Space>
        </Card>
      )}

      {viewMode === 'cards' ? (
        <Row gutter={[16, 16]}>
          {configs.map(c => {
            const badges = getFeatureBadges(c)
            return (
              <Col xs={24} md={12} xl={8} key={c.id}>
                <Card
                  className="ts-card-link"
                  style={{ height: '100%', display: 'flex', flexDirection: 'column' }}
                  styles={{ body: { flex: 1, display: 'flex', flexDirection: 'column', justifyContent: 'space-between', padding: 20 } }}
                >
                  <div>
                    <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'flex-start', gap: 10, marginBottom: 8 }}>
                      <Title level={5} style={{ margin: 0, fontSize: 16, lineHeight: 1.3 }}>{c.name}</Title>
                      <span
                        className="qorgau-mono"
                        style={{
                          fontSize: 12,
                          color: (c.agent_count ?? 0) > 0 ? 'var(--p-accent)' : 'var(--p-muted)',
                          whiteSpace: 'nowrap',
                          paddingTop: 2,
                        }}
                      >
                        {c.agent_count ?? 0} ПК
                      </span>
                    </div>

                    {c.created_by === 'system' && (
                      <div style={{ marginBottom: 10 }}>
                        <Tag bordered={false} style={{ fontSize: 11.5, margin: 0 }}>Стандартный профиль</Tag>
                      </div>
                    )}

                    <Text style={{ fontSize: 13.5, color: 'var(--p-ink-soft)', display: 'block', marginBottom: 16, minHeight: 42, lineHeight: 1.5 }}>
                      {c.description || 'Пользовательская конфигурация агента Qorgau DLP.'}
                    </Text>

                    <Space wrap size={[6, 6]} style={{ marginBottom: 14 }}>
                      {badges.map(b => (
                        <Tag key={b.label} color={b.color} bordered={false} style={{ margin: 0, fontSize: 12 }}>{b.label}</Tag>
                      ))}
                    </Space>
                  </div>

                  <div>
                    <Divider style={{ margin: '12px 0' }} />
                    <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
                      <Button
                        size="small"
                        icon={<UsergroupAddOutlined />}
                        onClick={() => setAssignTarget(c)}
                      >
                        Назначить ПК
                      </Button>
                      <Space size={4}>
                        <Tooltip title="Дублировать профиль">
                          <Button size="small" aria-label="Дублировать профиль" icon={<CopyOutlined />} onClick={() => openDuplicate(c)} />
                        </Tooltip>
                        <Tooltip title="Редактировать">
                          <Button size="small" aria-label="Редактировать" icon={<EditOutlined />} onClick={() => openEdit(c)} />
                        </Tooltip>
                        <Popconfirm
                          title="Удалить профиль?"
                          description="Агенты перейдут на профиль по умолчанию."
                          onConfirm={() => del.mutate(c.id)}
                        >
                          <Button size="small" danger aria-label="Удалить" icon={<DeleteOutlined />} />
                        </Popconfirm>
                      </Space>
                    </div>
                  </div>
                </Card>
              </Col>
            )
          })}
        </Row>
      ) : (
        <Card styles={{ body: { padding: 0 } }}>
          <Table
            columns={columns}
            dataSource={configs}
            rowKey="id"
            loading={isLoading}
            pagination={false}
          />
        </Card>
      )}

      <ConfigModal open={modalOpen} initial={editTarget} onClose={() => setModalOpen(false)} />
      <AssignModal config={assignTarget} onClose={() => setAssignTarget(null)} agents={agents} />
    </div>
  )
}
