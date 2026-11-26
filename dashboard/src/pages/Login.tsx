import { useState } from 'react'
import { Form, Input, Button, Typography, Alert } from 'antd'
import { LockOutlined, UserOutlined } from '@ant-design/icons'
import { useNavigate } from 'react-router-dom'
import { useAuth } from '../context/AuthContext'

const { Title, Text } = Typography

const LIVE_PREVIEW_ROWS = [
  {
    time: '13:42:08',
    host: 'FIN-WS-014',
    user: 'a.serikova',
    channel: 'USB / ВНИ',
    detail: 'Извлечён неразрешённый носитель VID:0951 PID:1666 (Whitelist)',
    level: 'critical' as const,
  },
  {
    time: '13:39:51',
    host: 'ACC-WS-003',
    user: 'm.nurlanov',
    channel: 'Буфер обмена',
    detail: 'Совпадение правила ИИН РК (12 цифр, контрольный разряд подтверждён)',
    level: 'high' as const,
  },
  {
    time: '13:34:19',
    host: 'HR-NB-007',
    user: 'd.omarova',
    channel: 'Краулер ПДн',
    detail: 'Найден файл salary_2026_q4.xlsx (48 записей ИИН, IBAN KZ)',
    level: 'accent' as const,
  },
  {
    time: '13:28:04',
    host: 'SOC-SRV-01',
    user: 'system',
    channel: 'Журнал аудита',
    detail: 'Цепочка SHA-256 верифицирована: 1 428 записей без расхождений',
    level: 'ok' as const,
  },
]

const LEVEL_DOT: Record<string, string> = {
  critical: '#ef4444',
  high: '#f59e0b',
  accent: '#5ab9fc',
  ok: '#10b981',
}

export default function Login() {
  const { login } = useAuth()
  const navigate = useNavigate()
  const [error, setError] = useState<string | null>(null)
  const [loading, setLoading] = useState(false)

  const onFinish = async (values: { username: string; password: string }) => {
    setError(null)
    setLoading(true)
    try {
      await login(values.username, values.password)
      navigate('/dashboard', { replace: true })
    } catch {
      setError('Неверный логин или пароль')
    } finally {
      setLoading(false)
    }
  }

  return (
    <div
      style={{
        minHeight: '100vh',
        background: '#0a0d14',
        color: '#e9edf3',
        display: 'grid',
        gridTemplateColumns: 'minmax(340px, 480px) 1fr',
      }}
    >
      {/* Left column: crisp sign-in form */}
      <div
        className="ts-enter"
        style={{
          padding: '48px 44px',
          borderRight: '1px solid rgba(233, 237, 243, 0.11)',
          display: 'flex',
          flexDirection: 'column',
          justifyContent: 'space-between',
          background: '#0a0d14',
        }}
      >
        {/* Brand wordmark */}
        <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
          <svg width="24" height="24" viewBox="0 0 32 32" aria-hidden="true">
            <path
              fill="none"
              stroke="#5ab9fc"
              strokeWidth="2.2"
              strokeLinejoin="round"
              d="M16 4L6 8v8c0 6 4.2 11.2 10 12.6C21.8 27.2 26 22 26 16V8L16 4z"
            />
            <path
              fill="none"
              stroke="#e9edf3"
              strokeWidth="2.2"
              strokeLinecap="round"
              strokeLinejoin="round"
              d="M11.5 16.2l3 3 6-6.4"
            />
          </svg>
          <span style={{ fontWeight: 500, fontSize: 16, letterSpacing: '-0.02em', color: '#e9edf3' }}>
            Qorgau DLP
          </span>
          <span className="qorgau-mono" style={{ fontSize: 11.5, color: '#8891a1', marginLeft: 4 }}>
            v2.0
          </span>
        </div>

        {/* Form block */}
        <div style={{ width: '100%', maxWidth: 360, margin: '48px 0' }}>
          <Title level={2} style={{ color: '#e9edf3', fontSize: 26, marginBottom: 8 }}>
            Вход в консоль
          </Title>
          <Text style={{ color: '#8891a1', fontSize: 14, display: 'block', marginBottom: 28 }}>
            Консоль защиты данных, контроля рабочих станций и расследования инцидентов.
          </Text>

          {error && (
            <Alert
              message={error}
              type="error"
              showIcon
              style={{ marginBottom: 20, borderRadius: 4 }}
            />
          )}

          <Form layout="vertical" onFinish={onFinish} autoComplete="off" requiredMark={false}>
            <Form.Item
              name="username"
              label={<span style={{ color: '#e9edf3', fontSize: 13.5, fontWeight: 500 }}>Логин оператора</span>}
              rules={[{ required: true, message: 'Введите логин' }]}
            >
              <Input
                prefix={<UserOutlined style={{ color: '#8891a1' }} />}
                placeholder="admin"
                size="large"
                autoFocus
                style={{ height: 42, borderRadius: 4, background: '#131824', borderColor: 'rgba(233,237,243,0.14)', color: '#e9edf3' }}
              />
            </Form.Item>

            <Form.Item
              name="password"
              label={<span style={{ color: '#e9edf3', fontSize: 13.5, fontWeight: 500 }}>Пароль</span>}
              rules={[{ required: true, message: 'Введите пароль' }]}
            >
              <Input.Password
                prefix={<LockOutlined style={{ color: '#8891a1' }} />}
                placeholder="Введите пароль"
                size="large"
                style={{ height: 42, borderRadius: 4, background: '#131824', borderColor: 'rgba(233,237,243,0.14)', color: '#e9edf3' }}
              />
            </Form.Item>

            <Button
              type="primary"
              htmlType="submit"
              block
              size="large"
              loading={loading}
              style={{
                marginTop: 8,
                height: 44,
                fontSize: 14.5,
                background: '#5ab9fc',
                color: '#0a0d14',
                borderColor: '#5ab9fc',
              }}
            >
              Войти в консоль
            </Button>
          </Form>
        </div>

        {/* Footer legal note */}
        <div style={{ fontSize: 12.5, color: '#8891a1', lineHeight: 1.5 }}>
          Закон РК №94-V · ТК РК ст. 23 · Приказ №410. Все действия операторов фиксируются в журнале аудита.
        </div>
      </div>

      {/* Right column: real product UI frame (tasteskill app-frame principle) */}
      <div
        style={{
          padding: '48px 56px',
          display: 'flex',
          flexDirection: 'column',
          justifyContent: 'center',
          background: '#0d111a',
          overflow: 'hidden',
        }}
      >
        <div style={{ maxWidth: 680, width: '100%', margin: '0 auto' }}>
          <div style={{ marginBottom: 24 }}>
            <div style={{ fontSize: 13, color: '#5ab9fc', fontWeight: 500, marginBottom: 8 }}>
              Архитектура Rust · Автономный Debian 12 Appliance
            </div>
            <Title level={3} style={{ color: '#e9edf3', fontSize: 28, lineHeight: 1.2, marginBottom: 10 }}>
              Контроль каналов утечек и рабочих станций в реальном времени.
            </Title>
            <Text style={{ color: '#8891a1', fontSize: 15, display: 'block', maxWidth: 560 }}>
              Агент 2.9 МБ на конечной точке, 5 готовых профилей безопасности, блокировка USB по серийному номеру и отчёты по законодательству РК.
            </Text>
          </div>

          {/* Real product UI preview window */}
          <div
            role="img"
            aria-label="Пример ленты событий и защиты Qorgau DLP"
            style={{
              background: '#131824',
              border: '1px solid rgba(233, 237, 243, 0.12)',
              borderRadius: 8,
              boxShadow: 'inset 0 1px 0 rgba(255,255,255,0.06)',
              overflow: 'hidden',
            }}
          >
            {/* Window header bar */}
            <div
              style={{
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'space-between',
                padding: '11px 16px',
                borderBottom: '1px solid rgba(233, 237, 243, 0.10)',
                fontSize: 12.5,
              }}
            >
              <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                <span className="ts-dot ts-dot--live" />
                <span style={{ color: '#e9edf3', fontWeight: 500 }}>Журнал событий периметра</span>
              </div>
              <span className="qorgau-mono" style={{ color: '#8891a1', fontSize: 11.5 }}>
                WSS /ws/agent · OCR RU/KZ/EN
              </span>
            </div>

            {/* Event rows */}
            <div>
              {LIVE_PREVIEW_ROWS.map((row, idx) => (
                <div
                  key={row.time}
                  style={{
                    padding: '13px 16px',
                    borderBottom: idx < LIVE_PREVIEW_ROWS.length - 1 ? '1px solid rgba(233, 237, 243, 0.08)' : 'none',
                    background: row.level === 'accent' ? 'rgba(90, 185, 252, 0.05)' : 'transparent',
                    display: 'grid',
                    gridTemplateColumns: '72px 115px 1fr',
                    gap: 12,
                    alignItems: 'baseline',
                  }}
                >
                  <span className="qorgau-mono" style={{ fontSize: 12, color: '#8891a1' }}>
                    {row.time}
                  </span>
                  <div>
                    <div className="qorgau-mono" style={{ fontSize: 12, color: '#e9edf3', fontWeight: 500 }}>
                      {row.host}
                    </div>
                    <div style={{ fontSize: 11.5, color: '#8891a1' }}>{row.user}</div>
                  </div>
                  <div>
                    <div style={{ display: 'flex', alignItems: 'center', gap: 7, marginBottom: 2 }}>
                      <span className="ts-dot" style={{ background: LEVEL_DOT[row.level] }} />
                      <span style={{ fontSize: 12.5, fontWeight: 500, color: '#e9edf3' }}>{row.channel}</span>
                    </div>
                    <div style={{ fontSize: 12.5, color: 'rgba(233, 237, 243, 0.78)', lineHeight: 1.4 }}>
                      {row.detail}
                    </div>
                  </div>
                </div>
              ))}
            </div>

            {/* Footer telemetry bar */}
            <div
              style={{
                display: 'grid',
                gridTemplateColumns: 'repeat(3, 1fr)',
                borderTop: '1px solid rgba(233, 237, 243, 0.10)',
                background: 'rgba(10, 13, 20, 0.45)',
              }}
            >
              <div style={{ padding: '12px 16px', borderRight: '1px solid rgba(233, 237, 243, 0.08)' }}>
                <div style={{ fontSize: 11.5, color: '#8891a1' }}>Агент Windows</div>
                <div className="qorgau-mono" style={{ fontSize: 14, color: '#e9edf3', marginTop: 2 }}>2.94 МБ · ~6.6 МБ RAM</div>
              </div>
              <div style={{ padding: '12px 16px', borderRight: '1px solid rgba(233, 237, 243, 0.08)' }}>
                <div style={{ fontSize: 11.5, color: '#8891a1' }}>Профили защиты</div>
                <div className="qorgau-mono" style={{ fontSize: 14, color: '#e9edf3', marginTop: 2 }}>5 стандартных</div>
              </div>
              <div style={{ padding: '12px 16px' }}>
                <div style={{ fontSize: 11.5, color: '#8891a1' }}>Микрофон / Камера</div>
                <div className="qorgau-mono" style={{ fontSize: 14, color: '#10b981', marginTop: 2 }}>Fail-Closed OFF</div>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  )
}
