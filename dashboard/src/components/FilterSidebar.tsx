import { useState } from 'react'
import { DatePicker } from 'antd'
import {
  WarningOutlined, UsbOutlined, MessageOutlined, AppstoreOutlined,
  GlobalOutlined, CameraOutlined, CaretDownOutlined, CaretRightOutlined,
  FilterOutlined,
} from '@ant-design/icons'
import { useFilters } from '../context/FilterContext'
import type { EventType, Severity } from '../api/types'
import dayjs from 'dayjs'

const EVENT_TYPES: { key: EventType; label: string; color: string; icon: React.ReactNode }[] = [
  { key: 'incident',   label: 'Инциденты',   color: 'var(--red)',    icon: <WarningOutlined /> },
  { key: 'usb',        label: 'USB',          color: 'var(--orange)', icon: <UsbOutlined /> },
  { key: 'messenger',  label: 'Мессенджеры',  color: 'var(--purple)', icon: <MessageOutlined /> },
  { key: 'app',        label: 'Приложения',   color: 'var(--blue)',   icon: <AppstoreOutlined /> },
  { key: 'web',        label: 'Браузер',      color: 'var(--cyan)',   icon: <GlobalOutlined /> },
  { key: 'screenshot', label: 'Скриншоты',    color: 'var(--green)',  icon: <CameraOutlined /> },
]

const SEVERITIES: { key: Severity; label: string; color: string }[] = [
  { key: 'critical', label: 'Критично',  color: 'var(--red)' },
  { key: 'high',     label: 'Высокая',   color: 'var(--orange)' },
  { key: 'medium',   label: 'Средняя',   color: 'var(--yellow)' },
  { key: 'low',      label: 'Низкая',    color: 'var(--green)' },
]

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  const [open, setOpen] = useState(true)
  return (
    <div className="filter-section">
      <div className="filter-section__title" onClick={() => setOpen(o => !o)}>
        {open ? <CaretDownOutlined /> : <CaretRightOutlined />}
        {title}
      </div>
      {open && children}
    </div>
  )
}

export default function FilterSidebar() {
  const { filters, toggleEventType, setFilter, resetFilters } = useFilters()
  const hasFilters = filters.eventTypes.length > 0 || filters.severity || filters.dateFrom || filters.dateTo

  return (
    <aside className="sidebar">
      <div className="sidebar__header">
        <span style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
          <FilterOutlined />
          Фильтры
        </span>
        {hasFilters && (
          <button
            onClick={resetFilters}
            style={{ background: 'none', border: 'none', cursor: 'pointer', color: 'var(--accent)', fontSize: 11 }}
          >
            Сбросить
          </button>
        )}
      </div>

      <div className="sidebar__scroll">
        {/* Event types */}
        <Section title="Тип события">
          {EVENT_TYPES.map(et => (
            <div
              key={et.key}
              className={`filter-item ${filters.eventTypes.includes(et.key) ? 'active' : ''}`}
              onClick={() => toggleEventType(et.key)}
            >
              <span style={{ color: et.color, fontSize: 12 }}>{et.icon}</span>
              <span>{et.label}</span>
            </div>
          ))}
        </Section>

        <div className="filter-divider" />

        {/* Severity */}
        <Section title="Серьёзность">
          {SEVERITIES.map(sv => (
            <div
              key={sv.key}
              className={`filter-item ${filters.severity === sv.key ? 'active' : ''}`}
              onClick={() => setFilter('severity', filters.severity === sv.key ? null : sv.key)}
            >
              <span className="filter-item__dot" style={{ background: sv.color }} />
              <span>{sv.label}</span>
            </div>
          ))}
        </Section>

        <div className="filter-divider" />

        {/* Date range */}
        <Section title="Период">
          <div style={{ padding: '6px 14px', display: 'flex', flexDirection: 'column', gap: 6 }}>
            <DatePicker
              size="small"
              placeholder="От"
              style={{ width: '100%', fontSize: 12 }}
              value={filters.dateFrom ? dayjs(filters.dateFrom) : null}
              onChange={d => setFilter('dateFrom', d?.toISOString() ?? null)}
            />
            <DatePicker
              size="small"
              placeholder="До"
              style={{ width: '100%', fontSize: 12 }}
              value={filters.dateTo ? dayjs(filters.dateTo) : null}
              onChange={d => setFilter('dateTo', d?.toISOString() ?? null)}
            />
          </div>
        </Section>

        <div className="filter-divider" />

        {/* Quick periods */}
        <Section title="Быстрый период">
          {[
            ['15 мин', 15],
            ['1 час', 60],
            ['8 часов', 480],
            ['24 часа', 1440],
            ['7 дней', 10080],
          ].map(([label, mins]) => (
            <div
              key={mins}
              className="filter-item"
              onClick={() => {
                const from = dayjs().subtract(mins as number, 'minute').toISOString()
                setFilter('dateFrom', from)
                setFilter('dateTo', null)
              }}
            >
              <span>{label}</span>
            </div>
          ))}
        </Section>
      </div>
    </aside>
  )
}
