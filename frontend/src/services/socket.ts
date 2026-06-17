import type { ClientMessage, ServerMessage } from '@/types/wire'

export type ConnStatus = 'idle' | 'connecting' | 'open' | 'reconnecting' | 'closed'
type Loc = Pick<Location, 'protocol' | 'host'>
type WSFactory = (url: string) => WebSocket
type OnMessage = (msg: ServerMessage) => void
type OnStatus = (status: ConnStatus) => void

const BACKOFF_BASE = 500
const BACKOFF_MAX = 8000

export class RoomSocket {
  private ws: WebSocket | null = null
  private code = ''
  private onMessage: OnMessage = () => {}
  private onStatus: OnStatus = () => {}
  private attempts = 0
  private closedByUs = false
  private timer: ReturnType<typeof setTimeout> | null = null
  /** Messages queued while the socket is still in CONNECTING state. */
  private sendQueue: string[] = []

  constructor(
    private factory: WSFactory = (url) => new WebSocket(url),
    private loc: Loc = window.location,
  ) {}

  onStatusChange(cb: OnStatus) { this.onStatus = cb }

  private url(code: string): string {
    const scheme = this.loc.protocol === 'https:' ? 'wss' : 'ws'
    return `${scheme}://${this.loc.host}/ws/${code}`
  }

  connect(code: string, onMessage: OnMessage) {
    if (this.ws) {
      this.ws.onopen = null
      this.ws.onmessage = null
      this.ws.onclose = null
      this.ws.onerror = null
      this.ws.close()
      this.ws = null
    }
    if (this.timer) { clearTimeout(this.timer); this.timer = null }
    this.attempts = 0
    this.code = code
    this.onMessage = onMessage
    this.closedByUs = false
    this.sendQueue = []
    this.open()
  }

  private open() {
    this.onStatus(this.attempts === 0 ? 'connecting' : 'reconnecting')
    const ws = this.factory(this.url(this.code))
    this.ws = ws
    ws.onopen = () => {
      this.attempts = 0
      this.onStatus('open')
      // Flush any messages that were queued while the socket was connecting.
      for (const raw of this.sendQueue) ws.send(raw)
      this.sendQueue = []
    }
    ws.onmessage = (e: MessageEvent) => {
      try {
        this.onMessage(JSON.parse(String(e.data)) as ServerMessage)
      } catch {
        /* ignore malformed frame */
      }
    }
    ws.onclose = () => {
      if (this.closedByUs) {
        this.onStatus('closed')
        return
      }
      this.scheduleReconnect()
    }
    ws.onerror = () => ws.close()
  }

  private scheduleReconnect() {
    this.attempts += 1
    const delay = Math.min(BACKOFF_MAX, BACKOFF_BASE * 2 ** (this.attempts - 1))
    this.timer = setTimeout(() => this.open(), delay)
  }

  send(msg: ClientMessage) {
    const raw = JSON.stringify(msg)
    if (this.ws && this.ws.readyState === WebSocket.OPEN) {
      this.ws.send(raw)
    } else {
      // Socket is still connecting (or reconnecting) — queue the message.
      this.sendQueue.push(raw)
    }
  }

  close() {
    this.closedByUs = true
    if (this.timer) clearTimeout(this.timer)
    this.ws?.close()
  }
}
