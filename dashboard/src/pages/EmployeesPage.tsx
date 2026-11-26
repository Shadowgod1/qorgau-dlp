import { useEffect, useState } from 'react'
import {
  Badge, Button, Card, Col, Descriptions, Drawer, Input, InputNumber,
  Progress, Row, Select, Space, Spin, Statistic, Switch, Table, Tabs, Tag, Typography, message,
} from 'antd'
import {
  AlertOutlined, BellOutlined, LaptopOutlined, ReloadOutlined, SearchOutlined, TeamOutlined, UserOutlined,
} from '@ant-design/icons'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import dayjs from 'dayjs'
import {
  fetchEmployeeProfile, fetchEmployees, fetchRiskThreshold, updateRiskThreshold,
  fetchUebaAnomalies, patchUebaAnomaly, triggerUebaScan,
  type EmployeeSummary, type UebaAnomaly,
} from '../api/client'
import { useAuth } from '../context/AuthContext'

const { Title, Text } = Typography

function fmtHours(sec: number): string {
  const h = Math.floor(sec / 3600)
  const m = Math.floor((sec % 3600) / 60)
  return `${h}ч ${m}м`
}

function riskColor(score: number): string {
  if (score >= 70) return '#f5222d'
  if (score >= 40) return '#fa8c16'
  if (score > 0) return '#fadb14'
  return '#52c41a'
}

function EmployeeCardDrawer({
  username,
  days,
  onClose,
}: {
  username: string | null
  days: number
  onClose: () => void
}) {
  const { data: profile, isLoading } = useQuery({
    queryKey: ['employee-profile', username, days],
    queryFn: () => fetchEmployeeProfile(username!, days),
    enabled: !!username,
  })

  return (
    <Drawer
      open={!!username}
      onClose={onClose}
      width={780}
      title={
        <Space>
          <UserOutlined />
          <span>Карточка сотрудника: <b>{username}</b></span>
          {profile && (
            <Tag color={profile.risk_score >= 70 ? 'red' : profile.risk_score >= 40 ? 'orange' : 'green'}>
              Риск: {profile.risk_score} / 100
            </Tag>
          )}
        </Space>
      }
    >
      {isLoading || !profile ? (
        <Spin style={{ display: 'block', margin: '48px auto' }} />
      ) : (
        <>
          <Row gutter={[12, 12]} style={{ marginBottom: 16 }}>
            <Col span={6}>
              <Card size="small">
                <Statistic
                  title="Risk Score"
                  value={profile.risk_score}
                  suffix="/ 100"
                  valueStyle={{ color: riskColor(profile.risk_score) }}
                />
              </Card>
            </Col>
            <Col span={6}>
              <Card size="small">
                <Statistic
                  title="Критических"
                  value={profile.risk_breakdown.critical}
                  valueStyle={{ color: profile.risk_breakdown.critical > 0 ? '#f5222d' : undefined }}
                />
              </Card>
            </Col>
            <Col span={6}>
              <Card size="small">
                <Statistic title="Высоких" value={profile.risk_breakdown.high} />
              </Card>
            </Col>
            <Col span={6}>
              <Card size="small">
                <Statistic title="ПК / Агентов" value={profile.agents.length} prefix={<LaptopOutlined />} />
              </Card>
            </Col>
          </Row>

          {profile.agents.length > 0 && (
            <Card size="small" title="Рабочие станции" style={{ marginBottom: 16 }}>
              <Descriptions size="small" column={2} bordered>
                {profile.agents.map((a) => (
                  <Descriptions.Item
                    key={a.agent_id}
                    label={<><Badge status={a.is_online ? 'success' : 'default'} /> {a.hostname}</>}
                  >
                    <Text type="secondary" style={{ fontSize: 12 }}>
                      {a.ip_address || '—'} · {a.last_seen ? dayjs(a.last_seen).format('DD.MM HH:mm') : '—'}
                    </Text>
                  </Descriptions.Item>
                ))}
              </Descriptions>
            </Card>
          )}

          <Tabs
            items={[
              {
                key: 'incidents',
                label: `Инциденты (${profile.incidents.length})`,
                children: (
                  <Table
                    size="small"
                    rowKey="id"
                    dataSource={profile.incidents}
                    pagination={{ pageSize: 10 }}
                    columns={[
                      { title: 'Время', dataIndex: 'created_at', width: 120, render: (v: string) => dayjs(v).format('DD.MM HH:mm') },
                      { title: 'Серьёзность', dataIndex: 'severity', width: 95, render: (v: string) => <Tag color={v === 'critical' ? 'red' : v === 'high' ? 'orange' : 'gold'}>{v}</Tag> },
                      { title: 'Канал', dataIndex: 'channel', width: 90 },
                      { title: 'Фрагмент', dataIndex: 'snippet', ellipsis: true },
                    ]}
                  />
                ),
              },
              {
                key: 'apps',
                label: `Приложения (${profile.top_apps.length})`,
                children: (
                  <Table
                    size="small"
                    rowKey="app_name"
                    dataSource={profile.top_apps}
                    pagination={false}
                    columns={[
                      { title: 'Приложение', dataIndex: 'app_name' },
                      { title: 'Время', dataIndex: 'total_sec', width: 110, render: (v: number) => fmtHours(v) },
                      {
                        title: 'Продуктивность', dataIndex: 'productivity', width: 130,
                        render: (v: string) => <Tag color={v === 'productive' ? 'green' : v === 'unproductive' ? 'red' : 'default'}>{v}</Tag>,
                      },
                    ]}
                  />
                ),
              },
              {
                key: 'sites',
                label: `Сайты (${profile.top_sites.length})`,
                children: (
                  <Table
                    size="small"
                    rowKey="domain"
                    dataSource={profile.top_sites}
                    pagination={false}
                    columns={[
                      { title: 'Домен', dataIndex: 'domain' },
                      { title: 'Категория', dataIndex: 'category', width: 110, render: (v: string) => <Tag>{v}</Tag> },
                      { title: 'Визиты', dataIndex: 'visits', width: 80, align: 'right' as const },
                      { title: 'Блок.', dataIndex: 'blocked', width: 80, align: 'right' as const },
                    ]}
                  />
                ),
              },
              {
                key: 'usb',
                label: `USB (${profile.recent_usb.length})`,
                children: (
                  <Table
                    size="small"
                    rowKey="id"
                    dataSource={profile.recent_usb}
                    pagination={{ pageSize: 10 }}
                    columns={[
                      { title: 'Время', dataIndex: 'created_at', width: 120, render: (v: string) => dayjs(v).format('DD.MM HH:mm') },
                      { title: 'Событие', dataIndex: 'action', width: 110, render: (v: string) => <Tag>{v}</Tag> },
                      { title: 'Устройство / Файл', render: (_: unknown, r) => r.file_path || r.device_name },
                    ]}
                  />
                ),
              },
              {
                key: 'worktime',
                label: `УРВ (${profile.worktime.length})`,
                children: (
                  <Table
                    size="small"
                    rowKey="day"
                    dataSource={profile.worktime}
                    pagination={{ pageSize: 14 }}
                    columns={[
                      { title: 'Дата', dataIndex: 'day', width: 110 },
                      { title: 'Активно', dataIndex: 'active_sec', render: (v: number) => fmtHours(v) },
                      { title: 'Простой', dataIndex: 'idle_sec', render: (v: number) => fmtHours(v) },
                      {
                        title: 'Продуктивность', dataIndex: 'productivity_pct',
                        render: (v: number) => <Progress percent={Math.round(v)} size="small" />,
                      },
                    ]}
                  />
                ),
              },
            ]}
          />
        </>
      )}
    </Drawer>
  )
}

export default function EmployeesPage() {
  const qc = useQueryClient()
  const { user } = useAuth()
  const isAdmin = user?.role === 'admin'
  const [days, setDays] = useState(30)
  const [search, setSearch] = useState('')
  const [onlyRisky, setOnlyRisky] = useState(false)
  const [selectedUser, setSelectedUser] = useState<string | null>(null)

  const { data: employees = [], isLoading, refetch } = useQuery({
    queryKey: ['employees', days],
    queryFn: () => fetchEmployees(days),
  })

  const { data: riskCfg } = useQuery({
    queryKey: ['risk-threshold'],
    queryFn: fetchRiskThreshold,
  })

  const [thresholdVal, setThresholdVal] = useState(60)
  const [notifyGrowth, setNotifyGrowth] = useState(true)

  useEffect(() => {
    if (riskCfg) {
      setThresholdVal(riskCfg.risk_threshold)
      setNotifyGrowth(riskCfg.notify_on_risk_growth)
    }
  }, [riskCfg])

  const saveThreshold = useMutation({
    mutationFn: () => updateRiskThreshold({ risk_threshold: thresholdVal, notify_on_risk_growth: notifyGrowth }),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['risk-threshold'] })
      message.success('Порог уведомлений по риску сохранён')
    },
    onError: () => message.error('Ошибка сохранения порога'),
  })

  const threshold = riskCfg?.risk_threshold ?? 60

  const filtered = employees.filter((e) => {
    if (onlyRisky && e.risk_score < threshold) return false
    if (search.trim()) {
      const q = search.toLowerCase()
      return e.username.toLowerCase().includes(q) || e.hostnames.some((h) => h.toLowerCase().includes(q))
    }
    return true
  })

  const columns = [
    {
      title: 'Сотрудник', dataIndex: 'username', key: 'username',
      render: (v: string, r: EmployeeSummary) => (
        <Space>
          <Badge status={r.is_online ? 'success' : 'default'} />
          <Text strong>{v}</Text>
          {r.risk_score >= threshold && <Tag color="red" icon={<AlertOutlined />}>Выше порога</Tag>}
        </Space>
      ),
    },
    {
      title: 'ПК', dataIndex: 'hostnames', key: 'hosts',
      render: (list: string[]) => (
        <Space wrap size={4}>
          {list.length ? list.map((h) => <Tag key={h}>{h}</Tag>) : <Text type="secondary">—</Text>}
        </Space>
      ),
    },
    {
      title: 'Risk Score', dataIndex: 'risk_score', key: 'risk', width: 170,
      sorter: (a: EmployeeSummary, b: EmployeeSummary) => a.risk_score - b.risk_score,
      defaultSortOrder: 'descend' as const,
      render: (v: number) => (
        <Progress
          percent={v}
          size="small"
          strokeColor={riskColor(v)}
          format={(p) => `${p}`}
        />
      ),
    },
    {
      title: 'Инциденты', key: 'incs', width: 150,
      render: (_: unknown, r: EmployeeSummary) => (
        <Space size={4}>
          <Tag>{r.total_incidents}</Tag>
          {r.critical_incidents > 0 && <Tag color="red">крит: {r.critical_incidents}</Tag>}
          {r.open_incidents > 0 && <Tag color="orange">откр: {r.open_incidents}</Tag>}
        </Space>
      ),
    },
    {
      title: 'Активное время', dataIndex: 'active_sec', key: 'active', width: 130,
      render: (v: number) => fmtHours(v),
    },
    {
      title: 'Продуктивность', dataIndex: 'productivity_pct', key: 'prod', width: 130,
      render: (v: number) => <Tag color={v >= 60 ? 'green' : v >= 30 ? 'gold' : 'default'}>{v}%</Tag>,
    },
    {
      title: 'Последняя активность', dataIndex: 'last_seen', key: 'seen', width: 150,
      render: (v: string | null) => (v ? dayjs(v).format('DD.MM.YY HH:mm') : '—'),
    },
  ]

  return (
    <div>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 16, flexWrap: 'wrap', gap: 12 }}>
        <div>
          <Title level={4} style={{ margin: 0 }}>
            <TeamOutlined style={{ marginRight: 8 }} />
            Сотрудники и профили риска
          </Title>
          <Text type="secondary" style={{ fontSize: 13 }}>
            Нажмите на строку, чтобы открыть полную карточку сотрудника (инциденты, УРВ, приложения, сайты, USB)
          </Text>
        </div>
        <Space>
          <Select
            value={days}
            onChange={setDays}
            style={{ width: 130 }}
            options={[
              { value: 7, label: 'За 7 дней' },
              { value: 30, label: 'За 30 дней' },
              { value: 90, label: 'За 90 дней' },
            ]}
          />
          <Button icon={<ReloadOutlined />} onClick={() => refetch()}>Обновить</Button>
        </Space>
      </div>

      {isAdmin && (
        <Card size="small" style={{ marginBottom: 16 }}>
          <Space wrap align="center">
            <BellOutlined style={{ color: '#fa8c16' }} />
            <Text strong>Пороговые уведомления по риску:</Text>
            <Text type="secondary">Алерт при Risk Score ≥</Text>
            <InputNumber min={10} max={100} value={thresholdVal} onChange={(v) => setThresholdVal(v ?? 60)} style={{ width: 76 }} />
            <Switch checked={notifyGrowth} onChange={setNotifyGrowth} size="small" />
            <Text type="secondary">Повторно при росте (+10)</Text>
            <Button size="small" type="primary" loading={saveThreshold.isPending} onClick={() => saveThreshold.mutate()}>
              Сохранить порог
            </Button>
          </Space>
        </Card>
      )}

      <Card style={{ marginBottom: 16 }} styles={{ body: { padding: '12px 16px' } }}>
        <Space wrap>
          <Input
            placeholder="Поиск по логину или ПК..."
            prefix={<SearchOutlined />}
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            style={{ width: 260 }}
            allowClear
          />
          <Switch checked={onlyRisky} onChange={setOnlyRisky} size="small" />
          <Text>Только выше порога риска (≥ {threshold})</Text>
        </Space>
      </Card>

      <Card>
        <Table
          dataSource={filtered}
          columns={columns}
          rowKey="username"
          loading={isLoading}
          size="small"
          scroll={{ x: 900 }}
          onRow={(rec) => ({ onClick: () => setSelectedUser(rec.username), style: { cursor: 'pointer' } })}
        />
      </Card>

      <UebaAnomaliesCard onSelectUser={setSelectedUser} />

      <EmployeeCardDrawer username={selectedUser} days={days} onClose={() => setSelectedUser(null)} />
    </div>
  )
}

function UebaAnomaliesCard({ onSelectUser }: { onSelectUser: (u: string) => void }) {
  const qc = useQueryClient()
  const { user } = useAuth()
  const canEdit = user?.role === 'admin' || user?.role === 'security_officer'

  const { data: anomalies = [], isLoading } = useQuery({
    queryKey: ['ueba-anomalies'],
    queryFn: () => fetchUebaAnomalies({ limit: 50 }),
  })

  const scanMut = useMutation({
    mutationFn: triggerUebaScan,
    onSuccess: (res) => {
      message.success(`UEBA-сканирование завершено: новых аномалий — ${res.detected}`)
      qc.invalidateQueries({ queryKey: ['ueba-anomalies'] })
    },
    onError: () => message.error('Ошибка запуска UEBA-сканирования'),
  })

  const statusMut = useMutation({
    mutationFn: ({ id, status }: { id: number; status: string }) => patchUebaAnomaly(id, status),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['ueba-anomalies'] }),
    onError: () => message.error('Ошибка обновления статуса'),
  })

  const openCount = anomalies.filter(a => a.status === 'open').length

  return (
    <Card
      style={{ marginTop: 16 }}
      title={
        <Space>
          <AlertOutlined style={{ color: '#fa8c16' }} />
          <span>Поведенческие аномалии (UEBA)</span>
          {openCount > 0 && <Tag color="red">{openCount} открытых</Tag>}
        </Space>
      }
      extra={
        canEdit && (
          <Button size="small" onClick={() => scanMut.mutate()} loading={scanMut.isPending}>
            Запустить UEBA-скан
          </Button>
        )
      }
    >
      <Table<UebaAnomaly>
        dataSource={anomalies}
        rowKey="id"
        loading={isLoading}
        size="small"
        pagination={{ pageSize: 10 }}
        locale={{ emptyText: 'Аномалий поведения не обнаружено' }}
        columns={[
          {
            title: 'Сотрудник',
            dataIndex: 'username',
            width: 140,
            render: (u: string) => (
              <a onClick={() => onSelectUser(u)} style={{ fontWeight: 500 }}>{u}</a>
            ),
          },
          {
            title: 'Тип',
            dataIndex: 'anomaly_type',
            width: 150,
            render: (t: string) => <Tag>{t}</Tag>,
          },
          {
            title: 'Уровень',
            dataIndex: 'severity',
            width: 100,
            render: (s: string) => (
              <Tag color={{ critical: 'red', high: 'orange', medium: 'gold', low: 'blue' }[s] || 'default'}>
                {s}
              </Tag>
            ),
          },
          { title: 'Описание', dataIndex: 'title', ellipsis: true },
          {
            title: 'Статус',
            dataIndex: 'status',
            width: 160,
            render: (st: string, rec: UebaAnomaly) =>
              canEdit ? (
                <Select
                  size="small"
                  value={st}
                  style={{ width: 145 }}
                  onChange={(val) => statusMut.mutate({ id: rec.id, status: val })}
                  options={[
                    { value: 'open', label: 'Открыта' },
                    { value: 'acknowledged', label: 'Принята' },
                    { value: 'resolved', label: 'Закрыта' },
                    { value: 'false_positive', label: 'Ложная' },
                  ]}
                />
              ) : (
                <Tag>{st}</Tag>
              ),
          },
          {
            title: 'Дата',
            dataIndex: 'created_at',
            width: 130,
            render: (v: string) => dayjs(v).format('DD.MM.YY HH:mm'),
          },
        ]}
      />
    </Card>
  )
}
