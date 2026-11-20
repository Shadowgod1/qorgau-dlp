import { useState, useEffect, useRef } from 'react'
import { useQuery } from '@tanstack/react-query'
import { fetchTimeline } from '../api/client'
import { useFilters } from '../context/FilterContext'
import type { TimelineEvent, EventType } from '../api/types'
import { Spin, Tooltip } from 'antd'
import {
  WarningOutlined, UsbOutlined, MessageOutlined, AppstoreOutlined,
  GlobalOutlined, CameraOutlined, ReloadOutlined, PauseOutlined, CaretRightOutlined,
} from '@ant-design/icons'
import EventDetail from './EventDetail'
import dayjs from 'dayjs'

const TYPE_META: Record<EventType | string, { label: string; color: string; icon: React.ReactNode }> = {
  incident:   { label: 'ИНЦИДЕНТ',  color: 'var(--red)',    icon: <WarningOutlined /> },
  usb:        { label: 'USB',       color: 'var(--orange)', icon: <UsbOutlined /> },
  messenger:  { label: 'MSG',       color: 'var(--purple)', icon: <MessageOutlined /> },
  app:        { label: 'APP',       color: 'var(--blue)',   icon: <AppstoreOutlined /> },
  web:        { label: 'WEB',       color: 'var(--cyan)',   icon: <GlobalOutlined /> },
  screenshot: { label: 'SCR',       color: 'var(--green)',  icon: <CameraOutlined /> },
}

function TypeChip({ type }: { type: string }) {
  const m = TYPE_META[type] ?? { label: type.toUpperCase(), color: 'var(--gray)', icon: null }
  return (
    <span className={`type-chip ${type}`}>
      {m.icon}
      {m.label}
    </span>
  )
}

function SevChip({ sev }: { sev: string | null }) {
  if (!sev) return null
  const labels: Record<string, string> = { critical: 'КРИТ', high: 'ВЫСОК', medium: 'СРЕДН', low: 'НИЗКАЯ' }
  return <span className={`sev-chip ${sev}`}>{labels[sev] ?? sev}</span>
}

function EventRow({
  event,
  selected,
  onClick,
}: {
  event: TimelineEvent
  selected: boolean
  onClick: () => void
}) {
  const ts = event.ts ? dayjs(event.ts).format('DD.MM HH:mm:ss') : '—'
  return (
    <div
      className={`event-row ${selected ? 'selected' : ''}`}
      onClick={onClick}
    >
      <span className="event-row__ts">{ts}</span>
      <span className="event-row__type">
        <TypeChip type={event.type} />
      </span>
      <span className="event-row__agent" title={event.agent_id}>
        {event.hostname || event.agent_id.slice(0, 8)}
      </span>
      <span className="event-row__user" title={event.username}>
        {event.username || '—'}
      </span>
      <span className="event-row__summary" title={event.summary}>
        {event.summary || '—'}
      </span>
      <span className="event-row__badge">
        <SevChip sev={event.severity} />
      </span>
    </div>
  )
}

const PAGE_SIZE = 200

export default function EventLog() {
  const { filters } = useFilters()
  const [selected, setSelected] = useState<TimelineEvent | null>(null)
  const [autoRefresh, setAutoRefresh] = useState(true)
  const scrollRef = useRef<HTMLDivElement>(null)
  const [offset, setOffset] = useState(0)

  const params: Record<string, unknown> = {
    limit: PAGE_SIZE,
    offset,
  }
  if (filters.eventTypes.length > 0) params.event_types = filters.eventTypes.join(',')
  if (filters.agentId) params.agent_id = filters.agentId
  if (filters.username) params.username = filters.username
  if (filters.severity) params.severity = filters.severity
  if (filters.search) params.search = filters.search
  if (filters.dateFrom) params.date_from = filters.dateFrom
  if (filters.dateTo) params.date_to = filters.dateTo

  const { data, isFetching, refetch } = useQuery({
    queryKey: ['timeline', params],
    queryFn: () => fetchTimeline(params),
    refetchInterval: autoRefresh ? 10_000 : false,
    placeholderData: prev => prev,
  })

  // Reset offset when filters change
  useEffect(() => { setOffset(0) }, [
    filters.agentId, filters.username, filters.severity, filters.search,
    filters.dateFrom, filters.dateTo, filters.eventTypes.join(','),
  ])

  const items = data?.items ?? []
  const total = data?.total ?? 0

  return (
    <div className="eventlog">
      {/* Header */}
      <div className="eventlog__header">
        <span className="eventlog__title">Лента событий</span>
        {isFetching && <Spin size="small" />}
        <span className="eventlog__total">
          {total.toLocaleString()} событий
        </span>
        <div className="eventlog__actions">
          <Tooltip title={autoRefresh ? 'Пауза авто-обновления' : 'Включить авто-обновление'}>
            <button
              className="topbar__icon-btn"
              onClick={() => setAutoRefresh(a => !a)}
            >
              {autoRefresh
                ? <PauseOutlined style={{ fontSize: 12 }} />
                : <CaretRightOutlined style={{ fontSize: 12 }} />
              }
            </button>
          </Tooltip>
          <Tooltip title="Обновить">
            <button className="topbar__icon-btn" onClick={() => refetch()}>
              <ReloadOutlined style={{ fontSize: 12 }} />
            </button>
          </Tooltip>
        </div>
      </div>

      {/* Column headers */}
      <div className="event-header">
        <span className="event-header__col">Время</span>
        <span className="event-header__col">Тип</span>
        <span className="event-header__col">Агент</span>
        <span className="event-header__col">Пользователь</span>
        <span className="event-header__col">Событие</span>
        <span className="event-header__col" style={{ textAlign: 'right' }}>Severity</span>
      </div>

      {/* Rows */}
      <div className="eventlog__table-wrap" ref={scrollRef}>
        {items.length === 0 && !isFetching && (
          <div style={{ padding: 32, textAlign: 'center', color: 'var(--text-muted)' }}>
            Нет событий по заданным фильтрам
          </div>
        )}
        {items.map(ev => (
          <EventRow
            key={`${ev.type}-${ev.id}`}
            event={ev}
            selected={selected?.id === ev.id && selected?.type === ev.type}
            onClick={() => setSelected(prev =>
              prev?.id === ev.id && prev?.type === ev.type ? null : ev
            )}
          />
        ))}

        {/* Pagination */}
        {total > offset + PAGE_SIZE && (
          <div style={{ padding: '12px 16px', display: 'flex', gap: 8, alignItems: 'center' }}>
            <button className="btn-ghost" onClick={() => setOffset(o => o + PAGE_SIZE)}>
              Загрузить ещё ({total - offset - PAGE_SIZE} остались)
            </button>
          </div>
        )}
        {offset > 0 && (
          <div style={{ padding: '0 16px 12px' }}>
            <button className="btn-ghost" onClick={() => setOffset(0)}>
              ↑ В начало
            </button>
          </div>
        )}
      </div>

      {/* Detail panel */}
      {selected && (
        <EventDetail event={selected} onClose={() => setSelected(null)} />
      )}
    </div>
  )
}
