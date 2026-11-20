import { createContext, useContext, useState, ReactNode } from 'react'
import type { FilterState, EventType, Severity } from '../api/types'

interface FilterCtx {
  filters: FilterState
  setFilter: <K extends keyof FilterState>(key: K, value: FilterState[K]) => void
  resetFilters: () => void
  toggleEventType: (type: EventType) => void
}

const DEFAULT: FilterState = {
  eventTypes: [],
  agentId: null,
  username: null,
  severity: null,
  search: '',
  dateFrom: null,
  dateTo: null,
}

const Ctx = createContext<FilterCtx>({
  filters: DEFAULT,
  setFilter: () => {},
  resetFilters: () => {},
  toggleEventType: () => {},
})

export function FilterProvider({ children }: { children: ReactNode }) {
  const [filters, setFilters] = useState<FilterState>(DEFAULT)

  const setFilter = <K extends keyof FilterState>(key: K, value: FilterState[K]) =>
    setFilters(f => ({ ...f, [key]: value }))

  const toggleEventType = (type: EventType) => {
    setFilters(f => {
      const cur = f.eventTypes
      const next = cur.includes(type) ? cur.filter(t => t !== type) : [...cur, type]
      return { ...f, eventTypes: next }
    })
  }

  const resetFilters = () => setFilters(DEFAULT)

  return (
    <Ctx.Provider value={{ filters, setFilter, resetFilters, toggleEventType }}>
      {children}
    </Ctx.Provider>
  )
}

export const useFilters = () => useContext(Ctx)
