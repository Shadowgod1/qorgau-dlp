import { useState } from 'react'
import {
  Card, Row, Col, Table, Tag, Typography, Progress, Tabs, Button, Space,
  Modal, Form, Input, Select, Popconfirm, message, DatePicker,
} from 'antd'
import { PlusOutlined, DeleteOutlined, EditOutlined } from '@ant-design/icons'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import dayjs from 'dayjs'
import {
  fetchWorktime, fetchActivitySummary,
  fetchActivityCategories, createActivityCategory, updateActivityCategory, deleteActivityCategory,
  type WorktimeRow, type ActivitySummaryRow, type ActivityCategory,
} from '../api/client'
import { useAuth } from '../context/AuthContext'

const { Title } = Typography

const fmtHM = (sec: number) => {
  const h = Math.floor((sec || 0) / 3600)
  const m = Math.floor(((sec || 0) % 3600) / 60)
  return h > 0 ? `${h}ч ${m}м` : `${m}м`
}

const PROD_COLOR: Record<string, string> = {
  productive: 'green', unproductive: 'red', neutral: 'default',
}
const PROD_LABEL: Record<string, string> = {
  productive: 'Продуктивные', unproductive: 'Непродуктивные', neutral: 'Нейтральные',
}
const prodPctColor = (pct: number) => (pct >= 70 ? '#52c41a' : pct >= 40 ? '#fa8c16' : '#f5222d')

// ── Сводка по пользователям ───────────────────────────────────────────────────
function SummaryTab() {
  const { data: summary = [], isLoading } = useQuery({
    queryKey: ['activity-summary'],
    queryFn: () => fetchActivitySummary(30),
    refetchInterval: 60_000,
  })

  const cols = [
    { title: 'Пользователь', dataIndex: 'username', key: 'username' },
    { title: 'Раб. дней', dataIndex: 'work_days', key: 'work_days', width: 90 },
    {
      title: 'Активно', dataIndex: 'active_sec', key: 'active', width: 110,
      render: (v: number) => fmtHM(v),
      sorter: (a: ActivitySummaryRow, b: ActivitySummaryRow) => a.active_sec - b.active_sec,
      defaultSortOrder: 'descend' as const,
    },
    { title: 'Простой', dataIndex: 'idle_sec', key: 'idle', width: 100, render: (v: number) => fmtHM(v) },
    { title: 'Продуктивно', dataIndex: 'productive_sec', key: 'prod', width: 110, render: (v: number) => fmtHM(v) },
    {
      title: 'Продуктивность', dataIndex: 'productivity_pct', key: 'pct', width: 180,
      render: (v: number) => (
        <Progress percent={v} size="small" strokeColor={prodPctColor(v)} format={() => `${v}%`} />
      ),
      sorter: (a: ActivitySummaryRow, b: ActivitySummaryRow) => a.productivity_pct - b.productivity_pct,
    },
  ]

  return (
    <Table
      dataSource={summary}
      columns={cols}
      rowKey="username"
      loading={isLoading}
      size="small"
      pagination={{ pageSize: 20 }}
      scroll={{ x: 700 }}
    />
  )
}

// ── Рабочее время по дням ─────────────────────────────────────────────────────
function WorktimeTab() {
  const [range, setRange] = useState<[dayjs.Dayjs, dayjs.Dayjs] | null>(null)
  const [username, setUsername] = useState('')

  const params: Record<string, unknown> = {}
  if (range) {
    params.date_from = range[0].startOf('day').toISOString()
    params.date_to = range[1].endOf('day').toISOString()
  }
  if (username) params.username = username

  const { data: rows = [], isLoading } = useQuery({
    queryKey: ['worktime', params],
    queryFn: () => fetchWorktime(params),
    refetchInterval: 60_000,
  })

  const cols = [
    {
      title: 'День', dataIndex: 'day', key: 'day', width: 110,
      render: (v: string) => v ? dayjs(v).format('DD.MM.YYYY') : '—',
    },
    { title: 'Пользователь', dataIndex: 'username', key: 'username' },
    {
      title: 'Начало', dataIndex: 'first_seen', key: 'first', width: 90,
      render: (v: string | null) => v ? dayjs(v).format('HH:mm') : '—',
    },
    {
      title: 'Конец', dataIndex: 'last_seen', key: 'last', width: 90,
      render: (v: string | null) => v ? dayjs(v).format('HH:mm') : '—',
    },
    { title: 'Активно', dataIndex: 'active_sec', key: 'active', width: 100, render: (v: number) => fmtHM(v) },
    { title: 'Простой', dataIndex: 'idle_sec', key: 'idle', width: 100, render: (v: number) => fmtHM(v) },
    {
      title: 'Продуктивность', dataIndex: 'productivity_pct', key: 'pct', width: 160,
      render: (v: number) => (
        <Progress percent={v} size="small" strokeColor={prodPctColor(v)} format={() => `${v}%`} />
      ),
    },
  ]

  return (
    <>
      <Space style={{ marginBottom: 12 }} wrap>
        <DatePicker.RangePicker
          onChange={(v) => setRange(v as [dayjs.Dayjs, dayjs.Dayjs] | null)}
          format="DD.MM.YYYY"
        />
        <Input.Search
          placeholder="Пользователь"
          allowClear
          onSearch={setUsername}
          style={{ width: 200 }}
        />
      </Space>
      <Table
        dataSource={rows}
        columns={cols}
        rowKey={(r: WorktimeRow) => `${r.username}-${r.day}`}
        loading={isLoading}
        size="small"
        pagination={{ pageSize: 25 }}
        scroll={{ x: 760 }}
      />
    </>
  )
}

// ── Категории продуктивности ──────────────────────────────────────────────────
function CategoriesTab() {
  const qc = useQueryClient()
  const [open, setOpen] = useState(false)
  const [editing, setEditing] = useState<ActivityCategory | null>(null)
  const [form] = Form.useForm()

  const { data: cats = [], isLoading } = useQuery({
    queryKey: ['activity-categories'],
    queryFn: fetchActivityCategories,
  })

  const save = useMutation({
    mutationFn: (data: { pattern: string; productivity: string; label?: string }) =>
      editing ? updateActivityCategory(editing.id, data) : createActivityCategory(data),
    onSuccess: () => {
      message.success('Сохранено')
      qc.invalidateQueries({ queryKey: ['activity-categories'] })
      setOpen(false); setEditing(null); form.resetFields()
    },
    onError: () => message.error('Ошибка сохранения'),
  })

  const remove = useMutation({
    mutationFn: deleteActivityCategory,
    onSuccess: () => { message.success('Удалено'); qc.invalidateQueries({ queryKey: ['activity-categories'] }) },
  })

  const openNew = () => { setEditing(null); form.resetFields(); setOpen(true) }
  const openEdit = (c: ActivityCategory) => { setEditing(c); form.setFieldsValue(c); setOpen(true) }

  const cols = [
    { title: 'Шаблон (подстрока имени)', dataIndex: 'pattern', key: 'pattern', render: (v: string) => <code>{v}</code> },
    { title: 'Название', dataIndex: 'label', key: 'label', render: (v: string | null) => v || '—' },
    {
      title: 'Класс', dataIndex: 'productivity', key: 'productivity', width: 160,
      render: (v: string) => <Tag color={PROD_COLOR[v]}>{PROD_LABEL[v] || v}</Tag>,
    },
    {
      title: '', key: 'actions', width: 80,
      render: (_: unknown, rec: ActivityCategory) => (
        <Space>
          <Button size="small" icon={<EditOutlined />} onClick={() => openEdit(rec)} />
          <Popconfirm title="Удалить?" onConfirm={() => remove.mutate(rec.id)}>
            <Button size="small" danger icon={<DeleteOutlined />} />
          </Popconfirm>
        </Space>
      ),
    },
  ]

  return (
    <>
      <div style={{ display: 'flex', justifyContent: 'flex-end', marginBottom: 12 }}>
        <Button type="primary" icon={<PlusOutlined />} onClick={openNew}>Добавить правило</Button>
      </div>
      <Table dataSource={cats} columns={cols} rowKey="id" loading={isLoading} size="small" pagination={{ pageSize: 20 }} />

      <Modal
        title={editing ? 'Редактировать правило' : 'Новое правило'}
        open={open}
        onCancel={() => { setOpen(false); setEditing(null) }}
        onOk={() => form.submit()}
        confirmLoading={save.isPending}
      >
        <Form form={form} layout="vertical" onFinish={(v) => save.mutate(v)}>
          <Form.Item name="pattern" label="Шаблон (подстрока имени процесса, напр. winword)" rules={[{ required: true }]}>
            <Input placeholder="winword" />
          </Form.Item>
          <Form.Item name="label" label="Название">
            <Input placeholder="MS Word" />
          </Form.Item>
          <Form.Item name="productivity" label="Класс" initialValue="productive" rules={[{ required: true }]}>
            <Select options={[
              { value: 'productive', label: 'Продуктивные' },
              { value: 'unproductive', label: 'Непродуктивные' },
              { value: 'neutral', label: 'Нейтральные' },
            ]} />
          </Form.Item>
        </Form>
      </Modal>
    </>
  )
}

export default function ActivityPage() {
  const { user } = useAuth()
  const isAdmin = user?.role === 'admin'

  const items = [
    { key: 'summary', label: 'Сводка по сотрудникам', children: <SummaryTab /> },
    { key: 'worktime', label: 'Рабочее время по дням', children: <WorktimeTab /> },
    ...(isAdmin ? [{ key: 'categories', label: 'Категории продуктивности', children: <CategoriesTab /> }] : []),
  ]

  return (
    <div>
      <Title level={4} style={{ marginBottom: 20 }}>Контроль активности</Title>
      <Row gutter={[16, 16]}>
        <Col span={24}>
          <Card>
            <Tabs items={items} />
          </Card>
        </Col>
      </Row>
    </div>
  )
}
