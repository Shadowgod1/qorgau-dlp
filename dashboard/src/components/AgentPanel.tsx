import { useQuery } from '@tanstack/react-query'
import { fetchAgents, fetchAgentStats } from '../api/client'
import type { Agent, AgentStat } from '../api/types'
import { useFilters } from '../context/FilterContext'
import { Tooltip } from 'antd'
import { ReloadOutlined } from '@ant-design/icons'
import dayjs from 'dayjs'
import relativeTime from 'dayjs/plugin/relativeTime'
import 'dayjs/locale/ru'

dayjs.extend(relativeTime)
dayjs.locale('ru')

interface AgentCardProps {
  agent: Agent
  stat: AgentStat | undefined
  selected: boolean
  onClick: () => void
}

function AgentCard({ agent, stat, selected, onClick }: AgentCardProps) {
  const isOnline = agent.is_online
  const lastSeen = agent.last_seen ? dayjs(agent.last_seen).fromNow() : 'никогда'

  return (
    <div
      className={`agent-card ${selected ? 'selected' : ''}`}
      onClick={onClick}
    >
      <div className="agent-card__top">
        <div className={`agent-status-dot ${isOnline ? 'online' : 'offline'}`} />
        <span className="agent-card__hostname">{agent.hostname}</span>
      </div>
      <div className="agent-card__user">
        {agent.username}
        <span style={{ color: 'var(--text-muted)', marginLeft: 6, fontSize: 11 }}>
          {lastSeen}
        </span>
      </div>
      {stat && (
        <div className="agent-card__counters">
          {stat.incident > 0 && (
            <Tooltip title="Инциденты">
              <span className="agent-counter incident">⚠ {stat.incident}</span>
            </Tooltip>
          )}
          {stat.usb > 0 && (
            <Tooltip title="USB события">
              <span className="agent-counter usb">USB {stat.usb}</span>
            </Tooltip>
          )}
          {stat.messenger > 0 && (
            <Tooltip title="Мессенджеры">
              <span className="agent-counter messenger">MSG {stat.messenger}</span>
            </Tooltip>
          )}
          {stat.app > 0 && (
            <Tooltip title="Приложения">
              <span className="agent-counter app">APP {stat.app}</span>
            </Tooltip>
          )}
          {stat.web > 0 && (
            <Tooltip title="Браузер">
              <span className="agent-counter web">WEB {stat.web}</span>
            </Tooltip>
          )}
          {stat.screenshot > 0 && (
            <Tooltip title="Скриншоты">
              <span className="agent-counter screenshot">SCR {stat.screenshot}</span>
            </Tooltip>
          )}
        </div>
      )}
    </div>
  )
}

export default function AgentPanel() {
  const { filters, setFilter } = useFilters()

  const { data: agents = [], refetch } = useQuery({
    queryKey: ['agents'],
    queryFn: fetchAgents,
    refetchInterval: 30_000,
  })

  const { data: stats = [] } = useQuery({
    queryKey: ['agent-stats'],
    queryFn: fetchAgentStats,
    refetchInterval: 30_000,
  })

  const statMap = Object.fromEntries((stats as import('../api/types').AgentStat[]).map(s => [s.agent_id, s]))
  const online = agents.filter(a => a.is_online).length

  const handleSelect = (agentId: string) => {
    setFilter('agentId', filters.agentId === agentId ? null : agentId)
  }

  return (
    <div className="agents">
      <div className="agents__header">
        <span className="agents__title">Агенты</span>
        <span className="agents__count">
          <span style={{ color: 'var(--green)' }}>{online}</span>
          <span style={{ color: 'var(--text-muted)' }}> / {agents.length}</span>
        </span>
        <Tooltip title="Обновить">
          <button
            className="topbar__icon-btn"
            style={{ width: 24, height: 24 }}
            onClick={() => refetch()}
          >
            <ReloadOutlined style={{ fontSize: 11 }} />
          </button>
        </Tooltip>
      </div>

      <div className="agents__scroll">
        {agents.length === 0 && (
          <div style={{ padding: 20, color: 'var(--text-muted)', fontSize: 12, textAlign: 'center' }}>
            Нет зарегистрированных агентов
          </div>
        )}
        {agents.map(agent => (
          <AgentCard
            key={agent.agent_id}
            agent={agent}
            stat={statMap[agent.agent_id]}
            selected={filters.agentId === agent.agent_id}
            onClick={() => handleSelect(agent.agent_id)}
          />
        ))}
      </div>
    </div>
  )
}
