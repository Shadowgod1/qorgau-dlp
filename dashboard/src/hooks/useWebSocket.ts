import { useEffect, useRef, useState } from 'react'
import type { WsMessage } from '../api/types'

/**
 * Authenticated WebSocket hook for `/ws/dashboard`.
 * Sends JWT in the first JSON auth frame (never in the URL), handles clean unmount
 * without zombie reconnect timers (StrictMode-safe), and uses exponential backoff.
 */
export function useWebSocket(token: string | null, onMessage: (msg: WsMessage) => void) {
  const onMessageRef = useRef(onMessage)
  onMessageRef.current = onMessage
  const [connected, setConnected] = useState(false)

  useEffect(() => {
    if (!token) {
      setConnected(false)
      return
    }

    let disposed = false
    let ws: WebSocket | null = null
    let reconnectTimer: ReturnType<typeof setTimeout> | null = null
    let backoffMs = 2000

    function connect() {
      if (disposed) return
      const proto = window.location.protocol === 'https:' ? 'wss:' : 'ws:'
      ws = new WebSocket(`${proto}//${window.location.host}/ws/dashboard`)

      ws.onopen = () => {
        if (disposed || !ws) return
        ws.send(JSON.stringify({ type: 'auth', token }))
        backoffMs = 2000
        setConnected(true)
      }

      ws.onmessage = (e) => {
        try {
          const msg: WsMessage = JSON.parse(e.data)
          onMessageRef.current(msg)
        } catch {
          // ignore non-JSON frames
        }
      }

      ws.onerror = () => {
        ws?.close()
      }

      ws.onclose = () => {
        setConnected(false)
        if (!disposed) {
          reconnectTimer = setTimeout(connect, backoffMs)
          backoffMs = Math.min(backoffMs * 2, 30_000)
        }
      }
    }

    connect()
    return () => {
      disposed = true
      if (reconnectTimer) clearTimeout(reconnectTimer)
      ws?.close()
    }
  }, [token])

  return { connected }
}
