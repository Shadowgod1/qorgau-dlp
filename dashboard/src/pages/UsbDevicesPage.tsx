import { useState } from 'react'
import {
  Button, Card, Form, Input, Modal, Popconfirm, Select, Space, Table, Tag, Typography, message,
} from 'antd'
import { DeleteOutlined, PlusOutlined, ReloadOutlined, UsbOutlined } from '@ant-design/icons'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import dayjs from 'dayjs'
import {
  deleteUsbDevice, fetchUsbDevices, patchUsbDevice, saveUsbDevice, type UsbDevice,
} from '../api/client'
import { useAuth } from '../context/AuthContext'

const { Title, Text } = Typography

const STATUS_OPTS = [
  { value: 'allowed', label: 'Разрешено (белый список)' },
  { value: 'blocked', label: 'Заблокировано' },
  { value: 'readonly', label: 'Только чтение' },
]

export default function UsbDevicesPage() {
  const qc = useQueryClient()
  const { user } = useAuth()
  const isAdmin = user?.role === 'admin'
  const [open, setOpen] = useState(false)
  const [form] = Form.useForm()

  const { data: devices = [], isLoading, refetch } = useQuery({
    queryKey: ['usb-devices'],
    queryFn: fetchUsbDevices,
  })

  const createMut = useMutation({
    mutationFn: saveUsbDevice,
    onSuccess: () => {
      message.success('Устройство ВНИ сохранено, правила отправлены агентам')
      qc.invalidateQueries({ queryKey: ['usb-devices'] })
      setOpen(false)
      form.resetFields()
    },
    onError: () => message.error('Ошибка сохранения устройства'),
  })

  const statusMut = useMutation({
    mutationFn: ({ id, status }: { id: number; status: UsbDevice['status'] }) =>
      patchUsbDevice(id, { status }),
    onSuccess: () => {
      message.success('Статус ВНИ обновлён')
      qc.invalidateQueries({ queryKey: ['usb-devices'] })
    },
  })

  const delMut = useMutation({
    mutationFn: deleteUsbDevice,
    onSuccess: () => {
      message.success('Удалено')
      qc.invalidateQueries({ queryKey: ['usb-devices'] })
    },
  })

  const columns = [
    {
      title: 'Серийный номер (S/N)', dataIndex: 'serial_number', key: 'sn',
      render: (v: string) => <code style={{ fontSize: 12 }}>{v}</code>,
    },
    { title: 'Название / Метка', dataIndex: 'label', key: 'label' },
    {
      title: 'VID / PID', key: 'vidpid', width: 150,
      render: (_: unknown, r: UsbDevice) => (
        <Text type="secondary" style={{ fontSize: 12 }}>
          {r.vendor_id || '—'} / {r.product_id || '—'}
        </Text>
      ),
    },
    { title: 'Владелец', dataIndex: 'owner', key: 'owner', width: 130, render: (v: string | null) => v || '—' },
    {
      title: 'Политика', dataIndex: 'status', key: 'status', width: 200,
      render: (v: UsbDevice['status'], rec: UsbDevice) =>
        isAdmin ? (
          <Select
            size="small"
            value={v}
            style={{ width: 185 }}
            options={STATUS_OPTS}
            onChange={(st) => statusMut.mutate({ id: rec.id, status: st })}
          />
        ) : (
          <Tag color={v === 'allowed' ? 'green' : v === 'blocked' ? 'red' : 'gold'}>{v}</Tag>
        ),
    },
    { title: 'Комментарий', dataIndex: 'comment', key: 'comment', ellipsis: true },
    {
      title: 'Обновлено', dataIndex: 'updated_at', key: 'upd', width: 130,
      render: (v: string) => dayjs(v).format('DD.MM.YY HH:mm'),
    },
    ...(isAdmin
      ? [
          {
            title: '', key: 'del', width: 55,
            render: (_: unknown, rec: UsbDevice) => (
              <Popconfirm title="Удалить устройство из реестра?" onConfirm={() => delMut.mutate(rec.id)}>
                <Button size="small" danger icon={<DeleteOutlined />} />
              </Popconfirm>
            ),
          },
        ]
      : []),
  ]

  return (
    <div>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 16, flexWrap: 'wrap', gap: 12 }}>
        <div>
          <Title level={4} style={{ margin: 0 }}>
            <UsbOutlined style={{ marginRight: 8 }} />
            Менеджер ВНИ (белые и чёрные списки USB)
          </Title>
          <Text type="secondary" style={{ fontSize: 13 }}>
            Реестр разрешённых и заблокированных USB-носителей по серийному номеру. В режиме «Белый список USB» в Конфигурациях все неизвестные флешки автоматически извлекаются агентом.
          </Text>
        </div>
        <Space>
          {isAdmin && (
            <Button type="primary" icon={<PlusOutlined />} onClick={() => setOpen(true)}>
              Добавить USB
            </Button>
          )}
          <Button icon={<ReloadOutlined />} onClick={() => refetch()}>Обновить</Button>
        </Space>
      </div>

      <Card>
        <Table
          dataSource={devices}
          columns={columns}
          rowKey="id"
          loading={isLoading}
          size="small"
          scroll={{ x: 900 }}
        />
      </Card>

      <Modal
        title="Добавить USB-устройство (ВНИ)"
        open={open}
        onCancel={() => setOpen(false)}
        onOk={() => form.submit()}
        confirmLoading={createMut.isPending}
        okText="Сохранить"
        cancelText="Отмена"
      >
        <Form form={form} layout="vertical" onFinish={(v) => createMut.mutate(v)}>
          <Form.Item name="serial_number" label="Серийный номер (S/N или VOL_...)" rules={[{ required: true }]}>
            <Input placeholder="Например: 0401E28A9B4C1234" />
          </Form.Item>
          <Form.Item name="label" label="Название / Модель">
            <Input placeholder="Kingston DataTraveler 32GB (Бухгалтерия)" />
          </Form.Item>
          <Space style={{ width: '100%' }}>
            <Form.Item name="vendor_id" label="Vendor ID"><Input placeholder="Kingston" /></Form.Item>
            <Form.Item name="product_id" label="Product ID"><Input placeholder="DT100G3" /></Form.Item>
          </Space>
          <Form.Item name="owner" label="Ответственный сотрудник">
            <Input placeholder="ivanov_a" />
          </Form.Item>
          <Form.Item name="status" label="Статус" initialValue="allowed" rules={[{ required: true }]}>
            <Select options={STATUS_OPTS} />
          </Form.Item>
          <Form.Item name="comment" label="Комментарий / Приказ">
            <Input.TextArea rows={2} />
          </Form.Item>
        </Form>
      </Modal>
    </div>
  )
}
