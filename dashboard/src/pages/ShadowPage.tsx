import { useState } from 'react'
import { Button, Card, Input, Space, Table, Tag, Tooltip, Typography, message } from 'antd'
import { DownloadOutlined, FileProtectOutlined, ReloadOutlined, SearchOutlined } from '@ant-design/icons'
import { useQuery } from '@tanstack/react-query'
import dayjs from 'dayjs'
import { downloadShadowCopy, fetchShadowCopies, type ShadowCopyItem } from '../api/client'

const { Title, Text } = Typography

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
  return `${(bytes / (1024 * 1024)).toFixed(2)} MB`
}

export default function ShadowPage() {
  const [search, setSearch] = useState('')

  const { data: items = [], isLoading, refetch } = useQuery({
    queryKey: ['shadow-copies'],
    queryFn: () => fetchShadowCopies({ limit: 200 }),
  })

  const filtered = items.filter((s) => {
    if (!search.trim()) return true
    const q = search.toLowerCase()
    return (
      s.file_name.toLowerCase().includes(q) ||
      s.original_path.toLowerCase().includes(q) ||
      s.username.toLowerCase().includes(q) ||
      s.sha256.toLowerCase().includes(q)
    )
  })

  const columns = [
    {
      title: 'Дата', dataIndex: 'created_at', key: 'date', width: 150,
      render: (v: string) => dayjs(v).format('DD.MM.YYYY HH:mm'),
    },
    {
      title: 'Файл', dataIndex: 'file_name', key: 'file_name',
      render: (v: string, r: ShadowCopyItem) => (
        <Space direction="vertical" size={0}>
          <Text strong style={{ fontSize: 13 }}>{v}</Text>
          <Text type="secondary" style={{ fontSize: 11 }}>{r.original_path}</Text>
        </Space>
      ),
    },
    { title: 'Сотрудник', dataIndex: 'username', key: 'username', width: 130 },
    { title: 'Хост', dataIndex: 'hostname', key: 'hostname', width: 130 },
    {
      title: 'Канал', dataIndex: 'channel', key: 'channel', width: 90,
      render: (v: string) => <Tag color="blue">{v}</Tag>,
    },
    {
      title: 'Размер', dataIndex: 'size_bytes', key: 'size', width: 100,
      render: (v: number) => formatSize(v),
    },
    {
      title: 'Инцидент', dataIndex: 'incident_id', key: 'inc', width: 100,
      render: (v: number | null) => (v ? <Tag color="red">#{v}</Tag> : <Tag>—</Tag>),
    },
    {
      title: 'SHA-256', dataIndex: 'sha256', key: 'sha', width: 120,
      render: (v: string) => (
        <Tooltip title={v}>
          <code style={{ fontSize: 11 }}>{v.slice(0, 12)}…</code>
        </Tooltip>
      ),
    },
    {
      title: '', key: 'dl', width: 60,
      render: (_: unknown, r: ShadowCopyItem) => (
        <Tooltip title="Скачать теневую копию (фиксируется в аудите)">
          <Button
            size="small"
            icon={<DownloadOutlined />}
            onClick={() =>
              downloadShadowCopy(r.id, r.file_name).catch(() =>
                message.error('Ошибка скачивания файла'),
              )
            }
          />
        </Tooltip>
      ),
    },
  ]

  return (
    <div>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 16, flexWrap: 'wrap', gap: 12 }}>
        <div>
          <Title level={4} style={{ margin: 0 }}>
            <FileProtectOutlined style={{ marginRight: 8 }} />
            Теневые копии (доказательная база)
          </Title>
          <Text type="secondary" style={{ fontSize: 13 }}>
            Архив перехваченных файлов с контрольными суммами SHA-256 и привязкой к инцидентам
          </Text>
        </div>
        <Button icon={<ReloadOutlined />} onClick={() => refetch()}>Обновить</Button>
      </div>

      <Card style={{ marginBottom: 16 }} styles={{ body: { padding: '12px 16px' } }}>
        <Input
          placeholder="Поиск по имени файла, пути, пользователю или SHA-256..."
          prefix={<SearchOutlined />}
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          style={{ maxWidth: 440 }}
          allowClear
        />
      </Card>

      <Card>
        <Table
          dataSource={filtered}
          columns={columns}
          rowKey="id"
          loading={isLoading}
          size="small"
          scroll={{ x: 960 }}
          pagination={{ pageSize: 25, showTotal: (t) => `Всего файлов: ${t}` }}
        />
      </Card>
    </div>
  )
}
