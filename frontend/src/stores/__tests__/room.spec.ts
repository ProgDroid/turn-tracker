import { describe, it, expect, beforeEach, vi } from 'vitest'
import { setActivePinia, createPinia } from 'pinia'
import { useRoomStore } from '@/stores/room'
import type { ServerMessage, PublicRoom } from '@/types/wire'

function room(partial: Partial<PublicRoom> = {}): PublicRoom {
  return {
    code: 'GR7K9P',
    state: 'active',
    locked: false,
    players: [
      { id: 'p1', name: 'Sam', is_host: true, connected: true },
      { id: 'p2', name: 'Alice', is_host: false, connected: true },
      { id: 'p3', name: 'Bob', is_host: false, connected: true },
    ],
    current_player_id: 'p2',
    ...partial,
  }
}

const fakeSocket = () => ({
  connect: vi.fn(),
  send: vi.fn(),
  close: vi.fn(),
  onStatusChange: vi.fn(),
})

describe('room store', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    localStorage.clear()
  })

  it('welcome stores identity and persists token', () => {
    const s = useRoomStore()
    s._setSocket(fakeSocket() as any)
    s.codeInView = 'GR7K9P'
    s._handle({ type: 'welcome', player_id: 'p1', token: 'tok' } as ServerMessage)
    expect(s.me).toEqual({ playerId: 'p1', token: 'tok' })
    expect(localStorage.getItem('tt:token:GR7K9P')).toBe('tok')
  })

  it('room_state replaces the room', () => {
    const s = useRoomStore()
    s._handle({ type: 'room_state', room: room() })
    expect(s.room?.current_player_id).toBe('p2')
  })

  it('isMyTurn reflects current_player_id vs me', () => {
    const s = useRoomStore()
    s.me = { playerId: 'p2', token: 't' }
    s._handle({ type: 'room_state', room: room() })
    expect(s.isMyTurn).toBe(true)
    s.me = { playerId: 'p1', token: 't' }
    expect(s.isMyTurn).toBe(false)
  })

  it('playersAway counts seats from me to current', () => {
    const s = useRoomStore()
    s.me = { playerId: 'p1', token: 't' } // current is p2 (index 1), me p1 index 0
    s._handle({ type: 'room_state', room: room() })
    // order p1,p2,p3 ; current p2 ; from p2 to p1 wraps: p2->p3->p1 = 2 away
    expect(s.playersAway).toBe(2)
  })

  it('amINext is true when I am the seat after current', () => {
    const s = useRoomStore()
    s.me = { playerId: 'p3', token: 't' } // current p2 index1, next index2 = p3
    s._handle({ type: 'room_state', room: room() })
    expect(s.amINext).toBe(true)
  })

  it('upNext lists players after current in order, wrapping', () => {
    const s = useRoomStore()
    s._handle({ type: 'room_state', room: room() })
    expect(s.upNext.map((p) => p.id)).toEqual(['p3', 'p1'])
  })

  it('phase derives from connection + room state', () => {
    const s = useRoomStore()
    expect(s.phase).toBe('landing')
    s._handle({ type: 'room_state', room: room({ state: 'lobby' }) })
    expect(s.phase).toBe('lobby')
    s._handle({ type: 'room_state', room: room({ state: 'active' }) })
    expect(s.phase).toBe('active')
  })

  it('nudge_cooldown error starts a client-side timer', () => {
    vi.useFakeTimers()
    const s = useRoomStore()
    s._handle({ type: 'error', code: 'nudge_cooldown', message: 'x' })
    expect(s.nudgeCooldownRemaining).toBeGreaterThan(0)
    vi.advanceTimersByTime(10_000)
    expect(s.nudgeCooldownRemaining).toBe(0)
    vi.useRealTimers()
  })

  it('stale token (not_found) during reconnect clears token and resets', () => {
    const s = useRoomStore()
    s.codeInView = 'GR7K9P'
    localStorage.setItem('tt:token:GR7K9P', 'stale')
    s.joiningWithToken = true
    s._handle({ type: 'error', code: 'not_found', message: 'x' })
    expect(localStorage.getItem('tt:token:GR7K9P')).toBeNull()
    expect(s.roomGone).toBe(true)
  })

  it('re-sends join when the socket reopens after a reconnect', () => {
    const sock = fakeSocket()
    const s = useRoomStore()
    s._setSocket(sock as any)
    s.codeInView = 'GR7K9P'
    localStorage.setItem('tt:token:GR7K9P', 'tok')
    const onStatus = sock.onStatusChange.mock.calls[0][0] as (st: string) => void

    // First open, before Welcome: me is null → store must NOT auto-join
    // (the view owns the initial join; auto-joining here would double up).
    onStatus('open')
    expect(sock.send).not.toHaveBeenCalled()

    // Once joined, a reconnect (open again) must re-join by token so the
    // fresh server connection re-authenticates us.
    s.me = { playerId: 'p1', token: 'tok' }
    onStatus('reconnecting')
    onStatus('open')
    expect(sock.send).toHaveBeenCalledWith({ type: 'join', player_token: 'tok' })
  })

  it('does NOT auto-join on open when no token is saved (prevents phantom "Player")', () => {
    const sock = fakeSocket()
    const s = useRoomStore()
    s._setSocket(sock as any)
    s.codeInView = 'NEWRM1'
    // Stale identity left over from a previously-viewed room, but NO token for
    // the room now in view. The old bug auto-sent a nameless join here, which
    // the server turned into a default "Player" duplicate.
    s.me = { playerId: 'pOld', token: 'old' }
    const onStatus = sock.onStatusChange.mock.calls[0][0] as (st: string) => void
    onStatus('open')
    expect(sock.send).not.toHaveBeenCalled()
  })

  it('connect() resets stale identity/room from a prior room', () => {
    const sock = fakeSocket()
    const s = useRoomStore()
    s._setSocket(sock as any)
    s.me = { playerId: 'pOld', token: 'old' }
    s._handle({ type: 'room_state', room: room() })
    s.connect('newrm2')
    expect(s.me).toBeNull()
    expect(s.room).toBeNull()
    expect(s.codeInView).toBe('NEWRM2')
  })

  it("socket reporting 'gone' marks the room as gone (server-restart recovery)", () => {
    const sock = fakeSocket()
    const s = useRoomStore()
    s._setSocket(sock as any)
    // Grab the status callback the store registered, then simulate the socket
    // giving up after exhausting reconnects against a room that no longer exists.
    const onStatus = sock.onStatusChange.mock.calls[0][0] as (st: string) => void
    onStatus('gone')
    expect(s.roomGone).toBe(true)
  })

  it('actions send the right client messages', () => {
    const sock = fakeSocket()
    const s = useRoomStore()
    s._setSocket(sock as any)
    s.endTurn()
    s.skipPlayer('p3')
    s.setLocked(true)
    expect(sock.send).toHaveBeenCalledWith({ type: 'end_turn' })
    expect(sock.send).toHaveBeenCalledWith({ type: 'skip_player', player_id: 'p3' })
    expect(sock.send).toHaveBeenCalledWith({ type: 'set_locked', locked: true })
  })

  it('locked getter mirrors the room flag', () => {
    const s = useRoomStore()
    expect(s.locked).toBe(false)
    s._handle({ type: 'room_state', room: room({ locked: true }) })
    expect(s.locked).toBe(true)
  })

  it('room_locked on a brand-new join (no me) flags joinRejected', () => {
    const s = useRoomStore()
    s.me = null
    s._handle({ type: 'error', code: 'room_locked', message: 'This room is locked' })
    expect(s.joinRejected).toBe('This room is locked')
  })

  it('room_locked does NOT flag joinRejected once joined (host locking mid-game)', () => {
    const s = useRoomStore()
    s.me = { playerId: 'p1', token: 't' }
    s._handle({ type: 'error', code: 'room_locked', message: 'x' })
    expect(s.joinRejected).toBeNull()
  })
})
