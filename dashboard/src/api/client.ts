import axios from 'axios'
import type { Agent, Incident, Screenshot, Policy, BlockedDomain, TimelineResponse } from './types'

export const TOKEN_KEY = 'qorgau_token'

const http = axios.create({ baseURL: '/api/v1' })

// Attach JWT synchronously on every request so page reload never races with 401
http.interceptors.request.use((config) => {
  const tok = localStorage.getItem(TOKEN_KEY)
  if (tok && !config.headers.Authorization) {
    config.headers.Authorization = `Bearer ${tok}`
  }
  return config
})

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

export const fetchIncidentById = (id: number) =>
  http.get<Incident>(`/incidents/${id}`).then(r => r.data)

export const createManualIncident = (data: {
  username: string
  channel: string
  severity: string
  snippet: string
  hostname?: string
  agent_id?: string
  group_tag?: string
  assigned_to?: string
  legal_hold?: boolean
}) => http.post<Incident>('/incidents', data).then(r => r.data)

export const patchIncident = (
  id: number,
  statusOrBody: string | {
    status?: string
    legal_hold?: boolean
    assigned_to?: string
    group_tag?: string
    resolution_note?: string
  },
) =>
  http.patch<Incident>(
    `/incidents/${id}`,
    typeof statusOrBody === 'string' ? { status: statusOrBody } : statusOrBody,
  ).then(r => r.data)

export interface IncidentComment {
  id: number
  incident_id: number
  author: string
  author_role: string
  body: string
  created_at: string
}

export const fetchIncidentComments = (id: number) =>
  http.get<IncidentComment[]>(`/incidents/${id}/comments`).then(r => r.data)

export const addIncidentComment = (id: number, body: string) =>
  http.post<IncidentComment>(`/incidents/${id}/comments`, { body }).then(r => r.data)

export interface IncidentEvidence {
  incident_id: number
  shadow_copies: {
    id: number
    file_name: string
    original_path: string
    sha256: string
    size_bytes: number
    created_at: string
  }[]
  screenshots: {
    id: number
    hostname: string
    file_size: number
    created_at: string
  }[]
}

export const fetchIncidentEvidence = (id: number) =>
  http.get<IncidentEvidence>(`/incidents/${id}/evidence`).then(r => r.data)

// ── Timeline ─────────────────────────────────────────────────────────────────
// GET /events/timeline → { items, total }
export const fetchTimeline = (params?: Record<string, unknown>) =>
  http.get<TimelineResponse>('/events/timeline', { params }).then(r => r.data)

export const fetchAgentStats = () =>
  http.get('/events/agent-stats').then(r => r.data)

// ── Screenshots ──────────────────────────────────────────────────────────────
// GET /screenshots → { total, items }
export const fetchScreenshotsPaged = (params?: Record<string, unknown>) =>
  http.get<{ total: number; items: Screenshot[] }>('/screenshots', { params }).then(r => r.data)

export const fetchScreenshots = (params?: Record<string, unknown>) =>
  fetchScreenshotsPaged(params).then(d => d.items)

export const fetchLatestScreenshot = (agentId: string) =>
  http.get<Screenshot[]>('/screenshots/latest', { params: { agent_id: agentId } }).then(r => r.data[0] ?? null)

export const deleteScreenshot = (id: number) => http.delete(`/screenshots/${id}`)

export const screenshotPath = (id: number) => `/screenshots/${id}/image`

export const triggerScreenshot = (agentId: string) =>
  http.post('/screenshots/trigger', null, { params: { agent_id: agentId } })

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
  mod_watermark: boolean
  watermark_opacity: number
  watermark_text: string
  watermark_apps: string
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
  usb_whitelist_mode: boolean
  mod_file_monitor: boolean
  mod_discovery: boolean
  discovery_paths: string
  mod_inventory: boolean
  mod_app_blocker: boolean
  mod_web_blocker: boolean
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

export const audioDownloadPath = (id: number) => `/audio/${id}/download`

export const downloadAudioRecording = (id: number) =>
  downloadFile(`/audio/${id}/download`, `recording_${id}.wav`)

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

export const webcamPhotoPath = (id: number) => `/webcam/${id}/image`

export const deleteWebcamPhoto = (id: number) =>
  http.delete(`/webcam/${id}`)

export const sendAgentCommand = (agentId: string, command: string) =>
  http.post('/webcam/command', { agent_id: agentId, command })

// ── Quadrator (Live Wall) ────────────────────────────────────────────────────
export const triggerLiveFrames = (agentIds: string[]) =>
  http.post('/screenshots/live-trigger', { agent_ids: agentIds }).then(r => r.data)

export const liveFramePath = (agentId: string) => `/screenshots/live/${encodeURIComponent(agentId)}`

// ── Employees & Risk Threshold ───────────────────────────────────────────────
export interface EmployeeSummary {
  username: string
  hostnames: string[]
  is_online: boolean
  last_seen: string | null
  total_incidents: number
  critical_incidents: number
  open_incidents: number
  risk_score: number
  active_sec: number
  productivity_pct: number
}

export interface EmployeeProfile {
  username: string
  period_days: number
  risk_score: number
  risk_breakdown: { critical: number; high: number; medium: number; low: number }
  agents: { agent_id: string; hostname: string; ip_address: string; is_online: boolean; last_seen: string | null }[]
  incidents: Incident[]
  top_apps: { app_name: string; total_sec: number; productivity: string }[]
  top_sites: { domain: string; category: string; visits: number; blocked: number }[]
  recent_usb: { id: number; device_name: string; action: string; file_path: string | null; created_at: string }[]
  worktime: WorktimeRow[]
}

export interface RiskThresholdSettings {
  risk_threshold: number
  notify_on_risk_growth: boolean
  updated_by: string | null
  updated_at: string | null
}

export const fetchEmployees = (days = 30) =>
  http.get<EmployeeSummary[]>('/users', { params: { days } }).then(r => r.data)

export const fetchEmployeeProfile = (username: string, days = 30) =>
  http.get<EmployeeProfile>(`/users/${encodeURIComponent(username)}/profile`, { params: { days } }).then(r => r.data)

export const fetchRiskThreshold = () =>
  http.get<RiskThresholdSettings>('/settings/risk-threshold').then(r => r.data)

export const updateRiskThreshold = (data: { risk_threshold: number; notify_on_risk_growth: boolean }) =>
  http.put<RiskThresholdSettings>('/settings/risk-threshold', data).then(r => r.data)

// ── Audit Log & Hash-Chain Verification ──────────────────────────────────────
export interface AuditEntry {
  id: number
  ts: string
  actor: string
  actor_role: string
  action: string
  resource: string
  details: Record<string, unknown>
  prev_hash: string
  entry_hash: string
}

export interface AuditVerifyResult {
  valid: boolean
  total_entries: number
  broken_at_id: number | null
  message: string
}

export const fetchAuditLogs = (params?: Record<string, unknown>) =>
  http.get<{ total: number; items: AuditEntry[] }>('/audit', { params }).then(r => r.data)

export const verifyAuditChain = () =>
  http.get<AuditVerifyResult>('/audit/verify').then(r => r.data)

// ── Shadow Copies Archive ────────────────────────────────────────────────────
export interface ShadowCopyItem {
  id: number
  agent_id: string
  hostname: string
  username: string
  channel: string
  original_path: string
  file_name: string
  sha256: string
  size_bytes: number
  incident_id: number | null
  created_at: string
}

export const fetchShadowCopies = (params?: Record<string, unknown>) =>
  http.get<ShadowCopyItem[]>('/shadow', { params }).then(r => r.data)

export const downloadShadowCopy = (id: number, fileName: string) =>
  downloadFile(`/shadow/${id}/download`, fileName || `shadow_${id}.bin`)

// ── Variant B: USB Devices (ВНИ), Blocked Apps, Discovery, Inventory ─────────
export interface UsbDevice {
  id: number
  serial_number: string
  vendor_id: string
  product_id: string
  label: string
  status: 'allowed' | 'blocked' | 'readonly'
  owner: string | null
  comment: string | null
  created_by: string | null
  created_at: string
  updated_at: string
}

export const fetchUsbDevices = () =>
  http.get<UsbDevice[]>('/usb-devices').then(r => r.data)

export const saveUsbDevice = (data: Partial<UsbDevice>) =>
  http.post<UsbDevice>('/usb-devices', data).then(r => r.data)

export const patchUsbDevice = (id: number, data: Partial<UsbDevice>) =>
  http.patch<UsbDevice>(`/usb-devices/${id}`, data).then(r => r.data)

export const deleteUsbDevice = (id: number) =>
  http.delete(`/usb-devices/${id}`)

export interface BlockedApp {
  id: number
  pattern: string
  action: 'block' | 'alert'
  reason: string | null
  enabled: boolean
  created_by: string | null
  created_at: string
}

export const fetchBlockedApps = () =>
  http.get<BlockedApp[]>('/rules/apps').then(r => r.data)

export const createBlockedApp = (data: { pattern: string; action: string; reason?: string }) =>
  http.post<BlockedApp>('/rules/apps', data).then(r => r.data)

export const deleteBlockedApp = (id: number) =>
  http.delete(`/rules/apps/${id}`)

export interface DiscoveryFinding {
  id: number
  agent_id: string
  hostname: string
  username: string
  file_path: string
  file_size: number
  sha256: string
  matched_rules: string[]
  severity: string
  snippet: string
  status: 'open' | 'resolved' | 'ignored'
  scanned_at: string
}

export const fetchDiscoveryFindings = (params?: Record<string, unknown>) =>
  http.get<DiscoveryFinding[]>('/discovery', { params }).then(r => r.data)

export const patchDiscoveryStatus = (id: number, status: string) =>
  http.patch<DiscoveryFinding>(`/discovery/${id}`, { status }).then(r => r.data)

export const triggerDiscoveryScan = (agentId?: string) =>
  http.post('/discovery/trigger', { agent_id: agentId ?? null }).then(r => r.data)

export interface AgentInventory {
  agent_id: string
  hostname: string
  username: string
  cpu_model: string
  cpu_cores: number
  ram_total_mb: number
  disk_total_gb: number
  disk_free_gb: number
  os_build: string
  updated_at: string
  software_count: number
}

export interface SoftwareItem {
  id: number
  name: string
  version: string
  publisher: string
  install_date: string
  status: 'installed' | 'removed'
  first_seen: string
  last_seen: string
}

export const fetchInventoryList = () =>
  http.get<AgentInventory[]>('/inventory').then(r => r.data)

export const fetchAgentInventoryDetail = (agentId: string) =>
  http.get<{ hardware: AgentInventory | null; software: SoftwareItem[] }>(`/inventory/${encodeURIComponent(agentId)}`).then(r => r.data)

export const triggerInventoryScan = (agentId?: string) =>
  http.post('/inventory/trigger', { agent_id: agentId ?? null }).then(r => r.data)

export const downloadWindowsAgent = () =>
  downloadFile('/agent-download/windows', 'qorgau-agent.exe')

// ── SIEM & Alerting ──────────────────────────────────────────────────────────
export interface SiemSettings {
  enabled: boolean
  webhook_url: string
  has_webhook_token: boolean
  syslog_host: string
  syslog_port: number
  syslog_proto: 'udp' | 'tcp'
  telegram_enabled: boolean
  has_telegram_token: boolean
  telegram_chat_id: string
  min_severity: 'low' | 'medium' | 'high' | 'critical'
  env_fallback_active: boolean
  updated_by: string | null
  updated_at: string
}

export const fetchSiemSettings = () =>
  http.get<SiemSettings>('/settings/siem').then(r => r.data)

export const updateSiemSettings = (data: {
  enabled: boolean
  webhook_url: string
  webhook_token?: string
  syslog_host: string
  syslog_port: number
  syslog_proto: 'udp' | 'tcp'
  telegram_enabled: boolean
  telegram_token?: string
  telegram_chat_id: string
  min_severity: string
}) => http.put<SiemSettings>('/settings/siem', data).then(r => r.data)

export const testSiemDelivery = () =>
  http.post<{ webhook: string; syslog: string; telegram: string }>('/settings/siem/test').then(r => r.data)

// ── UEBA (Behavioral Anomaly Detection) ──────────────────────────────────────
export interface UebaAnomaly {
  id: number
  username: string
  anomaly_type: string
  severity: string
  score_delta: number
  title: string
  details: Record<string, unknown>
  status: 'open' | 'acknowledged' | 'resolved' | 'false_positive'
  detected_date: string
  created_at: string
}

export const fetchUebaAnomalies = (params?: { status?: string; username?: string; limit?: number }) =>
  http.get<UebaAnomaly[]>('/ueba/anomalies', { params }).then(r => r.data)

export const patchUebaAnomaly = (id: number, status: string) =>
  http.patch<UebaAnomaly>(`/ueba/anomalies/${id}`, { status }).then(r => r.data)

export const triggerUebaScan = () =>
  http.post<{ detected: number }>('/ueba/scan').then(r => r.data)

// ── Mail DLP (SMTP / IMAP / Outlook) ─────────────────────────────────────────
export interface MailEvent {
  id: number
  agent_id: string
  username: string
  direction: string
  protocol: string
  sender: string
  recipients: string[]
  subject: string
  body_preview: string
  attachments: { filename: string; size_bytes: number; sha256: string }[]
  blocked: boolean
  incident_id: number | null
  created_at: string
}

export const fetchMailEvents = (params?: { username?: string; direction?: string; blocked?: boolean; limit?: number }) =>
  http.get<MailEvent[]>('/mail/events', { params }).then(r => r.data)

// ── Active Directory / LDAP ──────────────────────────────────────────────────
export interface AdSettings {
  enabled: boolean
  server_url: string
  base_dn: string
  bind_dn: string
  corporate_domains: string
  sync_interval_min: number
  last_sync_at: string | null
  last_sync_status: string
  synced_count: number
}

export interface AdUser {
  username: string
  full_name: string
  email: string
  department: string
  title: string
  manager: string
  is_enabled: boolean
  distinguished_name: string
  synced_at: string
}

export const fetchAdSettings = () =>
  http.get<AdSettings>('/directory/settings').then(r => r.data)

export const updateAdSettings = (data: Partial<AdSettings>) =>
  http.put<AdSettings>('/directory/settings', data).then(r => r.data)

export const fetchAdUsers = (params?: { search?: string; department?: string }) =>
  http.get<AdUser[]>('/directory/users', { params }).then(r => r.data)

export const importAdUsers = (users: Partial<AdUser>[]) =>
  http.post<{ synced: number }>('/directory/import', { users }).then(r => r.data)
