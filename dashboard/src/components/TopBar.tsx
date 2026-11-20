import { Input, Tooltip, notification } from 'antd'
import {
  SafetyCertificateOutlined, SearchOutlined, MoonOutlined, SunOutlined,
  SettingOutlined, ExportOutlined, ReloadOutlined,
} from '@ant-design/icons'
import { useTheme } from '../context/ThemeContext'
import { useFilters } from '../context/FilterContext'
import { useQueryClient } from '@tanstack/react-query'
import { useWebSocket } from '../hooks/useWebSocket'
import type { WsMessage } from '../api/types'

interface TopBarProps {
  onAdmin: () => void
  onExport: () => void
}

export default function TopBar({ onAdmin, onExport }: TopBarProps) {
  const { theme, toggle } = useTheme()
  const { filters, setFilter } = useFilters()
  const qc = useQueryClient()
  const [api, ctx] = notification.useNotification()

  useWebSocket((msg: WsMessage) => {
    qc.invalidateQueries({ queryKey: ['timeline'] })
    qc.invalidateQueries({ queryKey: ['agents'] })
    qc.invalidateQueries({ queryKey: ['agent-stats'] })
    if (msg.type === 'incident') {
      api.warning({
        message: `Инцидент: ${msg.channel ?? ''}`,
        description: `${msg.hostname} / ${msg.username} — ${msg.snippet ?? ''}`,
        placement: 'topRight',
        duration: 5,
        style: { background: 'var(--bg-surface)', border: '1px solid var(--border)', color: 'var(--text-primary)' },
      })
    }
  })

  return (
    <>
      {ctx}
      <header className="topbar">
        <a className="topbar__logo" href="/">
          <div className="topbar__logo-icon">
            <SafetyCertificateOutlined style={{ fontSize: 16 }} />
          </div>
          <div>
            <span>Qorgau</span>
            <span className="topbar__logo-sub">DLP Console</span>
          </div>
        </a>

        <div className="topbar__divider" />

        <div className="topbar__search">
          <Input
            prefix={<SearchOutlined style={{ color: 'var(--text-muted)' }} />}
            placeholder="Поиск по событиям, агентам, пользователям..."
            value={filters.search}
            onChange={e => setFilter('search', e.target.value)}
            allowClear
            style={{ fontSize: 12 }}
          />
        </div>

        <div className="topbar__right">
          <div className="topbar__live-dot" title="Real-time подключен" />

          <Tooltip title="Обновить данные">
            <button
              className="topbar__icon-btn"
              onClick={() => qc.invalidateQueries()}
            >
              <ReloadOutlined style={{ fontSize: 13 }} />
            </button>
          </Tooltip>

          <Tooltip title="Экспорт отчёта">
            <button className="topbar__icon-btn" onClick={onExport}>
              <ExportOutlined style={{ fontSize: 13 }} />
            </button>
          </Tooltip>

          <Tooltip title={theme === 'dark' ? 'Светлая тема' : 'Тёмная тема'}>
            <button className="topbar__icon-btn" onClick={toggle}>
              {theme === 'dark'
                ? <SunOutlined style={{ fontSize: 13 }} />
                : <MoonOutlined style={{ fontSize: 13 }} />
              }
            </button>
          </Tooltip>

          <button className="topbar__btn" onClick={onAdmin}>
            <SettingOutlined style={{ fontSize: 12 }} />
            Администрирование
          </button>
        </div>
      </header>
    </>
  )
}
