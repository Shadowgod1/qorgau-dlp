export type Severity = 'low' | 'medium' | 'high' | 'critical'
export type IncidentStatus = 'open' | 'reviewed' | 'resolved' | 'false_positive'
export type EventType = 'incident' | 'usb' | 'messenger' | 'app' | 'web' | 'screenshot' | 'session'

export interface Agent {
  id: number
  agent_id: string
  hostname: string
  username: string
  ip_address: string
  os_version: string
  agent_version: string
  is_online: boolean
  last_seen: string | null
  registered_at: string | null
  config_id?: number | null
}

export interface Incident {
  id: number
  agent_id: string
  hostname: string
  username: string
  channel: string
  severity: Severity
  status: IncidentStatus
  matched_rules: string[]
  snippet: string
  meta: Record<string, unknown>
  created_at: string | null
  updated_at: string | null
  legal_hold?: boolean
  assigned_to?: string | null
  group_tag?: string | null
  resolution_note?: string | null
}

export interface Screenshot {
  id: number
  agent_id: string
  hostname: string
  username: string
  file_size: number
  created_at: string
  image_url: string
}

export interface Policy {
  id: number
  name: string
  channel: string
  action: string
  severity: string
  pattern: string
  enabled: boolean
  created_by: string | null
  created_at: string | null
}

export interface BlockedDomain {
  id: number
  domain: string
  category: string
  reason: string | null
  enabled: boolean
  created_by: string | null
  created_at: string | null
}

export interface UsbEvent {
  id: number
  agent_id: string
  username: string
  device_name: string
  action: string
  drive_letter: string | null
  file_path: string | null
  size_gb: number | null
  created_at: string
}

export interface MessengerEvent {
  id: number
  agent_id: string
  username: string
  messenger: string
  direction: string
  contact: string | null
  content: string | null
  flagged: boolean
  matched_rules: string | null
  created_at: string
}

export interface AppEvent {
  id: number
  agent_id: string
  username: string
  app_name: string
  window_title: string
  exe_path: string | null
  duration_sec: number | null
  created_at: string
}

export interface WebEvent {
  id: number
  agent_id: string
  username: string
  url: string
  domain: string
  category: string
  blocked: boolean
  browser: string | null
  created_at: string
}

export interface TimelineEvent {
  id: number
  type: EventType
  agent_id: string
  hostname: string
  username: string
  ts: string
  summary: string
  severity: Severity | null
  subtype: string
}

export interface TimelineResponse {
  items: TimelineEvent[]
  total: number
}

export interface AgentStat {
  agent_id: string
  incident: number
  usb: number
  messenger: number
  app: number
  web: number
  screenshot: number
  total: number
}

export interface WsMessage {
  type: 'incident' | 'screenshot'
  id?: number
  severity?: Severity
  channel?: string
  hostname?: string
  username?: string
  snippet?: string
  agent_id?: string
  screenshot_id?: number
}

export interface FilterState {
  eventTypes: EventType[]
  agentId: string | null
  username: string | null
  severity: Severity | null
  search: string
  dateFrom: string | null
  dateTo: string | null
}
