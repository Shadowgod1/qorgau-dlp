import { useQuery } from '@tanstack/react-query'
import { fetchLatestScreenshot, screenshotUrl } from '../api/client'
import type { TimelineEvent } from '../api/types'
import { Image, Tooltip } from 'antd'
import { CloseOutlined, ZoomInOutlined } from '@ant-design/icons'
import dayjs from 'dayjs'

const TYPE_LABELS: Record<string, string> = {
  incident: 'Инцидент',
  usb: 'USB устройство',
  messenger: 'Мессенджер',
  app: 'Активное приложение',
  web: 'Браузер',
  screenshot: 'Скриншот',
}

const SEV_LABELS: Record<string, string> = {
  critical: 'Критично',
  high: 'Высокая',
  medium: 'Средняя',
  low: 'Низкая',
}

interface MetaRowProps { label: string; value: React.ReactNode; mono?: boolean }
function MetaRow({ label, value, mono }: MetaRowProps) {
  if (!value && value !== 0) return null
  return (
    <>
      <span className="meta-grid__label">{label}</span>
      <span className={`meta-grid__value ${mono ? 'mono' : ''}`}>{value}</span>
    </>
  )
}

interface EventDetailProps {
  event: TimelineEvent
  onClose: () => void
}

export default function EventDetail({ event, onClose }: EventDetailProps) {
  const ts = event.ts ? dayjs(event.ts).format('DD.MM.YYYY HH:mm:ss') : '—'

  // Fetch latest screenshot for this agent to show in detail panel
  const { data: screenshot } = useQuery({
    queryKey: ['latest-screenshot', event.agent_id],
    queryFn: () => fetchLatestScreenshot(event.agent_id),
    enabled: !!event.agent_id && event.type !== 'screenshot',
    retry: false,
  })

  const screenshotId = event.type === 'screenshot' ? event.id : screenshot?.id

  return (
    <div className="detail-panel">
      <div className="detail-panel__header">
        <span className={`type-chip ${event.type}`} style={{ fontSize: 11 }}>
          {TYPE_LABELS[event.type] ?? event.type}
        </span>
        {event.severity && (
          <span className={`sev-chip ${event.severity}`}>
            {SEV_LABELS[event.severity] ?? event.severity}
          </span>
        )}
        <span className="detail-panel__title" style={{ marginLeft: 4 }}>
          {event.summary || '—'}
        </span>
        <Tooltip title="Закрыть">
          <div className="detail-panel__close" onClick={onClose}>
            <CloseOutlined style={{ fontSize: 12 }} />
          </div>
        </Tooltip>
      </div>

      <div className="detail-panel__body">
        <div className="detail-panel__meta">
          <div className="meta-grid">
            <MetaRow label="Время" value={ts} mono />
            <MetaRow label="Тип" value={TYPE_LABELS[event.type] ?? event.type} />
            <MetaRow label="Агент ID" value={event.agent_id} mono />
            <MetaRow label="Хост" value={event.hostname || '—'} />
            <MetaRow label="Пользователь" value={event.username || '—'} />
            <MetaRow label="Подтип" value={event.subtype || '—'} />
            {event.severity && (
              <MetaRow label="Severity" value={SEV_LABELS[event.severity] ?? event.severity} />
            )}
            <MetaRow label="Описание" value={event.summary || '—'} />
          </div>
        </div>

        {/* Screenshot preview */}
        <div className="detail-panel__screenshot">
          {screenshotId ? (
            <Image
              src={screenshotUrl(screenshotId)}
              alt="screenshot"
              style={{ width: '100%', height: '100%', objectFit: 'cover' }}
              preview={{
                mask: <ZoomInOutlined />,
              }}
            />
          ) : (
            <div style={{ color: 'var(--text-muted)', fontSize: 12, textAlign: 'center', padding: 16 }}>
              Нет скриншота
            </div>
          )}
        </div>
      </div>
    </div>
  )
}
