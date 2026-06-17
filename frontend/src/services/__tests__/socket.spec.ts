import { describe, it, expect } from 'vitest'
import { RoomSocket } from '@/services/socket'
import type { ServerMessage } from '@/types/wire'

class FakeWS {
  static OPEN = 1
  static CLOSED = 3
  readyState = 1
  onopen: (() => void) | null = null
  onmessage: ((e: { data: string }) => void) | null = null
  onclose: (() => void) | null = null
  onerror: (() => void) | null = null
  sent: string[] = []
  constructor(public url: string) {}
  send(d: string) { this.sent.push(d) }
  close() { this.readyState = 3; this.onclose?.() }
  emitOpen() { this.readyState = 1; this.onopen?.() }
  emitMessage(m: ServerMessage) { this.onmessage?.({ data: JSON.stringify(m) }) }
}

function make() {
  const created: FakeWS[] = []
  const factory = (url: string) => {
    const ws = new FakeWS(url)
    created.push(ws)
    return ws as unknown as WebSocket
  }
  return { created, factory }
}

describe('RoomSocket', () => {
  it('builds a secure URL when page is https', () => {
    const { created, factory } = make()
    const s = new RoomSocket(factory, { protocol: 'https:', host: 'app.example' })
    s.connect('GR7K9P', () => {})
    expect(created[0].url).toBe('wss://app.example/ws/GR7K9P') // nosemgrep
  })

  it('builds a plaintext URL for local http dev', () => {
    const { created, factory } = make()
    const s = new RoomSocket(factory, { protocol: 'http:', host: 'localhost:5173' })
    s.connect('GR7K9P', () => {})
    expect(created[0].url.startsWith('ws://localhost:5173/ws/')).toBe(true) // nosemgrep
  })

  it('parses inbound messages and forwards them', () => {
    const { created, factory } = make()
    const got: ServerMessage[] = []
    const s = new RoomSocket(factory, { protocol: 'https:', host: 'h' })
    s.connect('C', (m) => got.push(m))
    created[0].emitOpen()
    created[0].emitMessage({ type: 'nudged' })
    expect(got).toEqual([{ type: 'nudged' }])
  })

  it('send() frames JSON', () => {
    const { created, factory } = make()
    const s = new RoomSocket(factory, { protocol: 'https:', host: 'h' })
    s.connect('C', () => {})
    created[0].emitOpen()
    s.send({ type: 'end_turn' })
    expect(created[0].sent).toEqual(['{"type":"end_turn"}'])
  })

  it('send() buffers messages while CONNECTING and flushes them in order on open', () => {
    const { created, factory } = make()
    const s = new RoomSocket(factory, { protocol: 'https:', host: 'h' })
    s.connect('C', () => {})
    // Force CONNECTING state so send() queues instead of sending immediately.
    created[0].readyState = 0
    s.send({ type: 'end_turn' })
    // Nothing sent yet — it was buffered.
    expect(created[0].sent).toEqual([])
    // Opening the socket flushes the queue.
    created[0].emitOpen()
    expect(created[0].sent).toEqual(['{"type":"end_turn"}'])
  })

  it('send() flushes multiple queued messages in order on open', () => {
    const { created, factory } = make()
    const s = new RoomSocket(factory, { protocol: 'https:', host: 'h' })
    s.connect('C', () => {})
    created[0].readyState = 0
    s.send({ type: 'end_turn' })
    s.send({ type: 'start_game' })
    // Both buffered — nothing sent yet.
    expect(created[0].sent).toEqual([])
    created[0].emitOpen()
    expect(created[0].sent).toEqual(['{"type":"end_turn"}', '{"type":"start_game"}'])
  })

  it('reconnecting to a different room tears down the old socket without spawning extras', () => {
    const { created, factory } = make()
    const s = new RoomSocket(factory, { protocol: 'https:', host: 'h' })
    s.connect('A', () => {})
    created[0].emitOpen()
    s.connect('B', () => {})
    const count = created.length // 2 sockets created so far
    created[0].close() // old socket closes late; its handlers were detached -> no reconnect
    expect(created.length).toBe(count) // no spurious third socket
    expect(created[1].url).toContain('/ws/B')
  })
})
