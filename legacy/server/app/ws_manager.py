import json
from fastapi import WebSocket


class ConnectionManager:
    def __init__(self):
        self._connections: set[WebSocket] = set()

    async def connect(self, ws: WebSocket):
        await ws.accept()
        self._connections.add(ws)

    def disconnect(self, ws: WebSocket):
        self._connections.discard(ws)

    async def broadcast(self, data: dict):
        if not self._connections:
            return
        message = json.dumps(data, default=str)
        dead: set[WebSocket] = set()
        for ws in list(self._connections):
            try:
                await ws.send_text(message)
            except Exception:
                dead.add(ws)
        self._connections -= dead


manager = ConnectionManager()
