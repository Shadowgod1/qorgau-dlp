import { useEffect, useState } from 'react'
import {
  Badge, Button, Card, Col, Empty, Modal, Row, Select, Space, Switch, Tag, Tooltip, Typography, message,
} from 'antd'
import {
  AppstoreOutlined, CameraOutlined, ExpandOutlined, PauseCircleOutlined, PlayCircleOutlined, ReloadOutlined,
} from '@ant-design/icons'
import { useQuery } from '@tanstack/react-query'
import http, { fetchAgents, liveFramePath, triggerLiveFrames, triggerScreenshot } from '../api/client'
import type { Agent } from '../api/types'

const { Title, Text } = Typography

function LiveTile({
  agent,
  tick,
  onZoom,
}: {
  agent: Agent
  tick: number
  onZoom: (agent: Agent, url: string) => void
}) {
  const [imgUrl, setImgUrl] = useState<string | null>(null)
  const [updatedAt, setUpdatedAt] = useState<string>('')

  useEffect(() => {
    let alive = true
    http
      .get(liveFramePath(agent.agent_id), { responseType: 'blob' })
      .then((resp) => {
        if (!alive) return
        const next = URL.createObjectURL(resp.data as Blob)
        setImgUrl((prev) => {
          if (prev) URL.revokeObjectURL(prev)
          return next
        })
        setUpdatedAt(new Date().toLocaleTimeString('ru-RU'))
      })
      .catch(() => {
        // Frame not ready yet
      })
    return () => {
      alive = false
    }
  }, [agent.agent_id, tick])

  useEffect(() => {
    return () => {
      if (imgUrl) URL.revokeObjectURL(imgUrl)
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  return (
    <Card
      size="small"
      title={
        <Space size={6}>
          <Badge status={agent.is_online ? 'success' : 'default'} />
          <Text strong style={{ fontSize: 13 }}>{agent.hostname}</Text>
          <Text type="secondary" style={{ fontSize: 11 }}>({agent.username})</Text>
        </Space>
      }
      extra={
        <Space size={4}>
          {updatedAt && <Text type="secondary" style={{ fontSize: 11 }}>{updatedAt}</Text>}
          <Tooltip title="Сохранить скриншот в архив">
            <Button
              size="small"
              type="text"
              icon={<CameraOutlined />}
              onClick={() =>
                triggerScreenshot(agent.agent_id)
                  .then(() => message.success(`Скриншот запрошен у ${agent.hostname}`))
                  .catch(() => message.error('Ошибка отправки команды'))
              }
            />
          </Tooltip>
          {imgUrl && (
            <Tooltip title="На весь экран">
              <Button size="small" type="text" icon={<ExpandOutlined />} onClick={() => onZoom(agent, imgUrl)} />
            </Tooltip>
          )}
        </Space>
      }
      styles={{ body: { padding: 6 } }}
    >
      <div
        onClick={() => imgUrl && onZoom(agent, imgUrl)}
        style={{
          height: 220,
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'center',
          background: 'rgba(128,128,128,0.08)',
          borderRadius: 4,
          overflow: 'hidden',
          cursor: imgUrl ? 'pointer' : 'default',
        }}
      >
        {imgUrl ? (
          <img
            src={imgUrl}
            alt={agent.hostname}
            style={{ width: '100%', height: '100%', objectFit: 'contain' }}
          />
        ) : (
          <Text type="secondary" style={{ fontSize: 12 }}>
            Ожидание кадра от агента...
          </Text>
        )}
      </div>
    </Card>
  )
}

export default function QuadratorPage() {
  const [cols, setCols] = useState<number>(2)
  const [intervalSec, setIntervalSec] = useState<number>(5)
  const [running, setRunning] = useState<boolean>(true)
  const [selectedIds, setSelectedIds] = useState<string[]>([])
  const [tick, setTick] = useState(0)
  const [zoomed, setZoomed] = useState<{ agent: Agent; url: string } | null>(null)

  const { data: agents = [] } = useQuery({
    queryKey: ['agents'],
    queryFn: fetchAgents,
    refetchInterval: 15_000,
  })

  const onlineAgents = agents.filter((a) => a.is_online)
  const visibleAgents =
    selectedIds.length > 0
      ? onlineAgents.filter((a) => selectedIds.includes(a.agent_id))
      : onlineAgents.slice(0, cols * cols)

  const requestNow = async () => {
    if (visibleAgents.length === 0) return
    try {
      await triggerLiveFrames(visibleAgents.map((a) => a.agent_id))
      setTimeout(() => setTick((t) => t + 1), 900)
    } catch {
      // ignore transient errors
    }
  }

  useEffect(() => {
    if (!running || visibleAgents.length === 0) return
    requestNow()
    const timer = setInterval(() => {
      requestNow()
    }, intervalSec * 1000)
    return () => clearInterval(timer)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [running, intervalSec, visibleAgents.map((a) => a.agent_id).join(',')])

  const colSpan = Math.floor(24 / cols)

  return (
    <div>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 16, flexWrap: 'wrap', gap: 12 }}>
        <div>
          <Title level={4} style={{ margin: 0 }}>
            <AppstoreOutlined style={{ marginRight: 8 }} />
            Квадратор — онлайн-мониторинг экранов
          </Title>
          <Text type="secondary" style={{ fontSize: 13 }}>
            Живые кадры в памяти сервера (0 нагрузки на БД и диск), до 16 экранов одновременно
          </Text>
        </div>

        <Space wrap>
          <Tag color={onlineAgents.length > 0 ? 'green' : 'default'}>
            Онлайн: {onlineAgents.length}
          </Tag>
          <Select
            mode="multiple"
            allowClear
            placeholder="Выбрать агентов (по умолчанию все онлайн)"
            style={{ minWidth: 240, maxWidth: 340 }}
            value={selectedIds}
            onChange={setSelectedIds}
            options={onlineAgents.map((a) => ({ value: a.agent_id, label: `${a.hostname} (${a.username})` }))}
          />
          <Select
            value={cols}
            onChange={setCols}
            style={{ width: 100 }}
            options={[
              { value: 2, label: '2 × 2' },
              { value: 3, label: '3 × 3' },
              { value: 4, label: '4 × 4' },
            ]}
          />
          <Select
            value={intervalSec}
            onChange={setIntervalSec}
            style={{ width: 115 }}
            options={[
              { value: 3, label: 'Каждые 3с' },
              { value: 5, label: 'Каждые 5с' },
              { value: 10, label: 'Каждые 10с' },
            ]}
          />
          <Switch
            checked={running}
            onChange={setRunning}
            checkedChildren={<PlayCircleOutlined />}
            unCheckedChildren={<PauseCircleOutlined />}
          />
          <Button icon={<ReloadOutlined />} onClick={requestNow}>Обновить</Button>
        </Space>
      </div>

      {visibleAgents.length === 0 ? (
        <Card>
          <Empty description="Нет онлайн-агентов для отображения в Квадраторе. Запустите qorgau-agent.exe на рабочем ПК." />
        </Card>
      ) : (
        <Row gutter={[12, 12]}>
          {visibleAgents.map((agent) => (
            <Col key={agent.agent_id} xs={24} md={colSpan}>
              <LiveTile
                agent={agent}
                tick={tick}
                onZoom={(a, url) => setZoomed({ agent: a, url })}
              />
            </Col>
          ))}
        </Row>
      )}

      <Modal
        open={!!zoomed}
        onCancel={() => setZoomed(null)}
        footer={null}
        width="85vw"
        title={zoomed ? `${zoomed.agent.hostname} (${zoomed.agent.username})` : ''}
      >
        {zoomed && (
          <img
            src={zoomed.url}
            alt={zoomed.agent.hostname}
            style={{ width: '100%', maxHeight: '78vh', objectFit: 'contain' }}
          />
        )}
      </Modal>
    </div>
  )
}
