import axios from 'axios'
import type { Agent, Incident, Screenshot, Policy, BlockedDomain, TimelineResponse } from './types'

const http = axios.create({ baseURL: '/api/v1' })
export default http

// ── Agents ──────────────────────────────────────────────────────────────────
// GET /agents → plain array
export const fetchAgents = () =>
  http.get<Agent[]>('/agents').then(r => r.data)

export const deleteAgent = (agentId: string) =>
  http.delete(`/agents/${agentId}`)

// ── Incidents ────────────────────────────────────────────────────────────────
// GET /incidents → { total, items }
export const fetchIncidents = (params?: Record<string, unknown>) =>
  http.get<{ total: number; items: Incident[] }>('/incidents', { params })
    .then(r => r.data.items)

export const fetchIncidentsPaged = (params?: Record<string, unknown>) =>
  http.get<{ total: number; items: Incident[] }>('/incidents', { params })
    .then(r => r.data)

export const fetchIncidentStats = () =>
  http.get<{
    by_severity: Record<string, number>
    by_status: Record<string, number>
    by_channel: Record<string, number>
  }>('/incidents/stats').then(r => r.data)

export const patchIncident = (id: number, status: string) =>
  http.patch(`/incidents/${id}`, { status })

// ── Timeline ─────────────────────────────────────────────────────────────────
// GET /events/timeline → { items, total }
export const fetchTimeline = (params?: Record<string, unknown>) =>
  http.get<TimelineResponse>('/events/timeline', { params }).then(r => r.data)

export const fetchAgentStats = () =>
  http.get('/events/agent-stats').then(r => r.data)

// ── Screenshots ──────────────────────────────────────────────────────────────
// GET /screenshots → { total, items }
export const fetchScreenshots = (params?: Record<string, unknown>) =>
  http.get<{ total: number; items: Screenshot[] }>('/screenshots', { params })
    .then(r => r.data.items)

export const fetchLatestScreenshot = (agentId: string) =>
  http.get<Screenshot>(`/screenshots/latest?agent_id=${agentId}`).then(r => r.data)

export const deleteScreenshot = (id: number) => http.delete(`/screenshots/${id}`)

export const screenshotUrl = (id: number) => {
  // <img> не шлёт заголовок Authorization — передаём JWT через query-параметр
  const token = localStorage.getItem('qorgau_token')
  return `/api/v1/screenshots/${id}/image${token ? `?token=${encodeURIComponent(token)}` : ''}`
}

export const triggerScreenshot = (agentId: string) =>
  http.post(`/screenshots/trigger?agent_id=${agentId}`)

// ── Policies ─────────────────────────────────────────────────────────────────
// GET /rules → plain array
// POST /rules → create
// PUT /rules/{id} → update
// DELETE /rules/{id} → delete
export const fetchPolicies = () =>
  http.get<Policy[]>('/rules').then(r => r.data)

export const createPolicy = (data: Partial<Policy>) =>
  http.post('/rules', data)

export const updatePolicy = (id: number, data: Partial<Policy>) =>
  http.put(`/rules/${id}`, data)

export const deletePolicy = (id: number) =>
  http.delete(`/rules/${id}`)

// ── Blocked domains ──────────────────────────────────────────────────────────
// GET /rules/domains → plain array
// POST /rules/domains → create
// DELETE /rules/domains/{id} → delete
export const fetchBlockedDomains = () =>
  http.get<BlockedDomain[]>('/rules/domains').then(r => r.data)

export const createBlockedDomain = (data: Partial<BlockedDomain>) =>
  http.post('/rules/domains', data)

export const deleteBlockedDomain = (id: number) =>
  http.delete(`/rules/domains/${id}`)

// ── Configs ──────────────────────────────────────────────────────────────────
export interface AgentConfig {
  id: number
  name: string
  description: string | null
  mod_sessions: boolean
  mod_apps: boolean
  mod_web: boolean
  mod_messenger: boolean
  mod_clipboard: boolean
  mod_network: boolean
  mod_usb: boolean
  mod_printer: boolean
  mod_screenshots: boolean
  mod_microphone: boolean
  mod_webcam: boolean
  screenshot_interval: number
  screenshot_quality: number
  screenshot_max_w: number
  microphone_duration: number
  microphone_sample_rate: number
  webcam_interval: number
  webcam_quality: number
  webcam_max_w: number
  net_alert_sensitive_ports: boolean
  net_alert_unknown_ext: boolean
  net_block_tor: boolean
  web_alert_social: boolean
  web_alert_adult: boolean
  web_alert_gambling: boolean
  web_alert_streaming: boolean
  web_alert_crypto: boolean
  usb_alert_on_connect: boolean
  usb_analyze_content: boolean
  usb_alert_executables: boolean
  created_by: string | null
  created_at: string | null
  updated_at: string | null
  agent_count?: number
  agents?: { agent_id: string; hostname: string }[]
}

export const fetchConfigs = () =>
  http.get<AgentConfig[]>('/configs').then(r => r.data)

export const fetchConfig = (id: number) =>
  http.get<AgentConfig>(`/configs/${id}`).then(r => r.data)

export const createConfig = (data: Partial<AgentConfig>) =>
  http.post<AgentConfig>('/configs', data).then(r => r.data)

export const updateConfig = (id: number, data: Partial<AgentConfig>) =>
  http.put<AgentConfig>(`/configs/${id}`, data).then(r => r.data)

export const deleteConfig = (id: number) =>
  http.delete(`/configs/${id}`)

export const assignConfig = (configId: number, agentIds: string[]) =>
  http.post(`/configs/${configId}/assign`, { agent_ids: agentIds })

export const unassignConfig = (configId: number, agentIds: string[]) =>
  http.delete(`/configs/${configId}/assign`, { data: { agent_ids: agentIds } })

// ── Operators (RBAC) ─────────────────────────────────────────────────────────
export interface Operator {
  id: number
  username: string
  role: 'admin' | 'security_officer' | 'viewer'
  full_name: string | null
  is_active: boolean
  created_at: string | null
  last_login: string | null
}

export const fetchOperators = () =>
  http.get<Operator[]>('/auth/users').then(r => r.data)

export const createOperator = (data: {
  username: string; password: string; role: string; full_name?: string
}) => http.post<Operator>('/auth/users', data).then(r => r.data)

export const updateOperator = (
  id: number,
  data: { role?: string; password?: string; is_active?: boolean; full_name?: string },
) => http.patch<Operator>(`/auth/users/${id}`, data).then(r => r.data)

export const deleteOperator = (id: number) =>
  http.delete(`/auth/users/${id}`)

// ── Activity (контроль активности) ───────────────────────────────────────────
export interface WorktimeRow {
  username: string
  day: string
  first_seen: string | null
  last_seen: string | null
  total_sec: number
  active_sec: number
  idle_sec: number
  productive_sec: number
  unproductive_sec: number
  neutral_sec: number
  productivity_pct: number
}

export interface ActivitySummaryRow {
  username: string
  work_days: number
  active_sec: number
  idle_sec: number
  productive_sec: number
  unproductive_sec: number
  productivity_pct: number
}

export interface ActivityCategory {
  id: number
  pattern: string
  productivity: 'productive' | 'unproductive' | 'neutral'
  label: string | null
}

export const fetchWorktime = (params?: Record<string, unknown>) =>
  http.get<WorktimeRow[]>('/activity/worktime', { params }).then(r => r.data)

export const fetchActivitySummary = (days = 30) =>
  http.get<ActivitySummaryRow[]>('/activity/summary', { params: { days } }).then(r => r.data)

export const fetchActivityCategories = () =>
  http.get<ActivityCategory[]>('/activity/categories').then(r => r.data)

export const createActivityCategory = (data: { pattern: string; productivity: string; label?: string }) =>
  http.post('/activity/categories', data)

export const updateActivityCategory = (id: number, data: { pattern: string; productivity: string; label?: string }) =>
  http.patch(`/activity/categories/${id}`, data)

export const deleteActivityCategory = (id: number) =>
  http.delete(`/activity/categories/${id}`)

// ── Reports ──────────────────────────────────────────────────────────────────
export const fetchIncidentReport = (days: number) =>
  http.get('/reports/incidents', { params: { days } }).then(r => r.data)

export const fetchComplianceReport = () =>
  http.get('/reports/compliance').then(r => r.data)

export const fetchAgentsReport = () =>
  http.get('/reports/agents').then(r => r.data)

export const exportReportUrl = (format: 'csv' | 'json', days: number) =>
  `/api/v1/reports/export?format=${format}&days=${days}`

// Скачивание через axios (с токеном) → blob → download. <a href> не шлёт заголовок.
async function downloadFile(url: string, filename: string) {
  const resp = await http.get(url, { responseType: 'blob' })
  const blobUrl = URL.createObjectURL(resp.data as Blob)
  const a = document.createElement('a')
  a.href = blobUrl
  a.download = filename
  a.click()
  setTimeout(() => URL.revokeObjectURL(blobUrl), 1000)
}

export const downloadIncidentsExport = (format: 'csv' | 'json' | 'xlsx', days: number) =>
  downloadFile(`/reports/export?format=${format}&days=${days}`, `incidents_${days}d.${format}`)

export const downloadCdr = (format: 'csv' | 'xlsx', days: number) =>
  downloadFile(`/reports/cdr?format=${format}&days=${days}`, `cdr_${days}d.${format}`)

export const downloadWorktimeExport = (days: number) =>
  downloadFile(`/reports/activity/export?days=${days}`, `worktime_${days}d.xlsx`)

// ── Audio (microphone recordings) ─────────────────────────────────────────────
export interface AudioRecording {
  id: number
  agent_id: string
  hostname: string
  username: string
  file_size: number
  duration_sec: number
  sample_rate: number
  created_at: string | null
}

export const fetchAudioRecordings = (params?: Record<string, unknown>) =>
  http.get<{ total: number; items: AudioRecording[] }>('/audio', { params }).then(r => r.data)

export const audioDownloadUrl = (id: number) => {
  const token = localStorage.getItem('qorgau_token')
  return `/api/v1/audio/${id}/download${token ? `?token=${encodeURIComponent(token)}` : ''}`
}

export const deleteAudioRecording = (id: number) =>
  http.delete(`/audio/${id}`)

// ── Webcam photos ──────────────────────────────────────────────────────────
export interface WebcamPhotoItem {
  id: number
  agent_id: string
  hostname: string
  username: string
  file_size: number
  created_at: string | null
}

export const fetchWebcamPhotos = (params?: Record<string, unknown>) =>
  http.get<{ total: number; items: WebcamPhotoItem[] }>('/webcam', { params }).then(r => r.data)

export const webcamPhotoUrl = (id: number) => {
  const token = localStorage.getItem('qorgau_token')
  return `/api/v1/webcam/${id}/image${token ? `?token=${encodeURIComponent(token)}` : ''}`
}

export const deleteWebcamPhoto = (id: number) =>
  http.delete(`/webcam/${id}`)

export const sendAgentCommand = (agentId: string, command: string) =>
  http.post('/webcam/command', { agent_id: agentId, command })
