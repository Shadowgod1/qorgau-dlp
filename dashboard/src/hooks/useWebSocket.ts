import { useEffect, useRef } from 'react'
import type { WsMessage } from '../api/types'

export function useWebSocket(onMessage: (msg: WsMessage) => void) {
  const onMessageRef = useRef(onMessage)
  onMessageRef.current = onMessage

  useEffect(() => {
    let ws: WebSocket
    let reconnectTimer: ReturnType<typeof setTimeout>

    function connect() {
      const proto = window.location.protocol === 'https:' ? 'wss:' : 'ws:'
      ws = new WebSocket(`${proto}//${window.location.host}/ws`)

      ws.onmessage = (e) => {
        try {
          const msg: WsMessage = JSON.parse(e.data)
          onMessageRef.current(msg)
        } catch {
          // ignore malformed frames
        }
      }

      ws.onclose = () => {
        reconnectTimer = setTimeout(connect, 5000)
      }
    }

    connect()
    return () => {
      clearTimeout(reconnectTimer)
      ws?.close()
    }
  }, [])
}
