import { defineStore } from 'pinia'
import type {
  ClientMessage, PlayerId, PublicPlayer, PublicRoom, ServerMessage,
} from '@/types/wire'
import { RoomSocket, type ConnStatus } from '@/services/socket'
import { saveToken, loadToken, clearToken } from '@/services/tokenStore'
import { shuffle } from '@/services/reorder'
import { formatElapsed } from '@/services/duration'

const NUDGE_COOLDOWN_MS = 10_000
const TURN_CLOCK_TICK_MS = 1_000

interface Me { playerId: PlayerId; token: string }
interface SocketLike {
  connect: (code: string, cb: (m: ServerMessage) => void) => void
  send: (m: ClientMessage) => void
  close: () => void
  onStatusChange: (cb: (s: ConnStatus) => void) => void
}

interface State {
  socket: SocketLike | null
  codeInView: string
  me: Me | null
  room: PublicRoom | null
  connStatus: ConnStatus
  /**
   * The last error, as a code only — the view translates it. Kept as an object
   * rather than a bare string so each arrival is a fresh identity for
   * ToastHost's watcher.
   */
  lastError: { code: string } | null
  nudgeCooldownEndsAt: number | null
  nudgeCooldownRemaining: number
  nudgeTimer: ReturnType<typeof setInterval> | null
  joiningWithToken: boolean
  roomGone: boolean
  /**
   * Error CODE set when a brand-new join was refused (room locked or full);
   * routes home. Deliberately the code and not the server's message, which is
   * untranslated English the view would otherwise render verbatim.
   */
  joinRejectedCode: string | null
  nudgeReceivedAt: number | null
  /** Seconds the server reported for the current turn in the last frame. */
  turnElapsedBase: number | null
  /** Local `Date.now()` when that frame landed — the extrapolation origin. */
  turnSyncedAt: number | null
  /** Ticked once a second so the derived clock re-renders. */
  turnClockNow: number
  turnTimer: ReturnType<typeof setInterval> | null
}

export const useRoomStore = defineStore('room', {
  state: (): State => ({
    socket: null,
    codeInView: '',
    me: null,
    room: null,
    connStatus: 'idle',
    lastError: null,
    nudgeCooldownEndsAt: null,
    nudgeCooldownRemaining: 0,
    nudgeTimer: null,
    joiningWithToken: false,
    roomGone: false,
    joinRejectedCode: null,
    nudgeReceivedAt: null,
    turnElapsedBase: null,
    turnSyncedAt: null,
    turnClockNow: 0,
    turnTimer: null,
  }),

  getters: {
    isHost: (s): boolean =>
      !!s.me && !!s.room?.players.find((p) => p.id === s.me!.playerId)?.is_host,
    currentPlayer: (s): PublicPlayer | null =>
      s.room?.players.find((p) => p.id === s.room?.current_player_id) ?? null,
    isMyTurn: (s): boolean =>
      !!s.me && !!s.room && s.room.current_player_id === s.me.playerId,
    myIndex(s): number {
      if (!s.me || !s.room) return -1
      return s.room.players.findIndex((p) => p.id === s.me!.playerId)
    },
    currentIndex(s): number {
      if (!s.room) return -1
      return s.room.players.findIndex((p) => p.id === s.room!.current_player_id)
    },
    playersAway(): number {
      const n = this.room?.players.length ?? 0
      if (n === 0 || this.myIndex < 0 || this.currentIndex < 0) return 0
      return (this.myIndex - this.currentIndex + n) % n
    },
    amINext(): boolean {
      return this.playersAway === 1
    },
    upNext(s): PublicPlayer[] {
      const players = s.room?.players ?? []
      const n = players.length
      if (n === 0 || this.currentIndex < 0) return []
      const out: PublicPlayer[] = []
      for (let i = 1; i < n; i++) out.push(players[(this.currentIndex + i) % n])
      return out
    },
    phase(s): 'landing' | 'lobby' | 'active' {
      if (!s.room) return 'landing'
      return s.room.state === 'active' ? 'active' : 'lobby'
    },
    locked: (s): boolean => !!s.room?.locked,
    /**
     * Seconds the current player has held the turn: the server's number plus
     * the time since it arrived. Extrapolating from a server-resolved count
     * (rather than from a shared timestamp) keeps every phone in agreement
     * without trusting any of their clocks against each other.
     */
    turnElapsedSecs: (s): number | null => {
      if (s.turnElapsedBase === null || s.turnSyncedAt === null) return null
      const drift = Math.max(0, Math.floor((s.turnClockNow - s.turnSyncedAt) / 1000))
      return s.turnElapsedBase + drift
    },
    turnElapsedLabel(): string | null {
      return this.turnElapsedSecs === null ? null : formatElapsed(this.turnElapsedSecs)
    },
  },

  actions: {
    _setSocket(sock: SocketLike) {
      this.socket = sock
      sock.onStatusChange((st) => {
        this.connStatus = st
        // A reconnect opens a FRESH server-side connection that starts
        // unauthenticated (me = None), so we must re-send `join` or every
        // subsequent action fails with `not_joined`. Guard on `me`: the very
        // first open is driven by the view (me still null until Welcome),
        // which avoids a double-join here. Rejoin-by-token is idempotent.
        // Also require a saved token: the auto-rejoin re-authenticates an
        // already-welcomed player, which is ALWAYS by token (saved on welcome).
        // Without one we'd send a nameless join and the server would mint a
        // fresh default-named "Player" — a phantom duplicate. Never do that.
        if (st === 'open' && this.me && loadToken(this.codeInView)) this.join()
        // Terminal 'gone' = reconnect cap exhausted against a room that no
        // longer exists (e.g. server restart). Trigger room-gone recovery.
        if (st === 'gone') this.roomGone = true
      })
    },

    ensureSocket() {
      if (!this.socket) this._setSocket(new RoomSocket())
    },

    connect(code: string) {
      this.codeInView = code.toUpperCase()
      this.roomGone = false
      this.joinRejectedCode = null
      // Entering a room is a fresh session. Drop any identity/room carried over
      // from a previously-viewed room in this SPA session — otherwise the
      // socket 'open' handler above would see the stale `me` as "already
      // joined" and auto-send a nameless join, spawning a phantom "Player".
      this.me = null
      this.room = null
      // The turn clock belongs to the room being left; the first room_state
      // from the new one re-seeds it. Without this its ticker outlives the room.
      this._syncTurnClock(null)
      this.ensureSocket()
      this.socket!.connect(this.codeInView, (m) => this._handle(m))
    },

    /** After ws open, the view calls this to (re)join. */
    join(name?: string) {
      const token = loadToken(this.codeInView)
      this.joiningWithToken = !!token
      const msg: ClientMessage = token
        ? { type: 'join', player_token: token }
        : { type: 'join', player_name: name }
      this.socket!.send(msg)
    },

    _handle(m: ServerMessage) {
      switch (m.type) {
        case 'welcome':
          this.me = { playerId: m.player_id, token: m.token }
          saveToken(this.codeInView, m.token)
          this.joiningWithToken = false
          break
        case 'room_state':
          this.room = m.room
          if (!this.codeInView) this.codeInView = m.room.code
          this._syncTurnClock(m.room.turn_elapsed_secs ?? null)
          break
        case 'nudged':
          this.nudgeReceivedAt = Date.now()
          break
        case 'error':
          this.lastError = { code: m.code }
          if (m.code === 'nudge_cooldown') this._startNudgeCooldown()
          if (m.code === 'not_found' && this.joiningWithToken) {
            clearToken(this.codeInView)
            this.joiningWithToken = false
            this.roomGone = true
          }
          // A brand-new join we never completed (no `me` yet) was refused
          // because the room is locked or full → bounce back to landing.
          if ((m.code === 'room_locked' || m.code === 'room_full') && !this.me) {
            this.joinRejectedCode = m.code
          }
          break
      }
    },

    /** Re-anchor the turn clock to the server's number, starting or stopping
     *  the local ticker to match whether a turn is in progress. */
    _syncTurnClock(elapsed: number | null) {
      this.turnElapsedBase = elapsed
      if (elapsed === null) {
        this.turnSyncedAt = null
        this._stopTurnClock()
        return
      }
      const now = Date.now()
      this.turnSyncedAt = now
      this.turnClockNow = now
      if (!this.turnTimer) {
        this.turnTimer = setInterval(() => {
          this.turnClockNow = Date.now()
        }, TURN_CLOCK_TICK_MS)
      }
    },

    _stopTurnClock() {
      if (this.turnTimer) clearInterval(this.turnTimer)
      this.turnTimer = null
    },

    _startNudgeCooldown() {
      this.nudgeCooldownEndsAt = Date.now() + NUDGE_COOLDOWN_MS
      this.nudgeCooldownRemaining = NUDGE_COOLDOWN_MS / 1000
      if (this.nudgeTimer) clearInterval(this.nudgeTimer)
      this.nudgeTimer = setInterval(() => {
        const left = (this.nudgeCooldownEndsAt! - Date.now()) / 1000
        if (left <= 0) {
          clearInterval(this.nudgeTimer!)
          this.nudgeTimer = null
          this.nudgeCooldownRemaining = 0
          this.nudgeCooldownEndsAt = null
        } else {
          this.nudgeCooldownRemaining = left
        }
      }, 100)
    },

    dismissError() { this.lastError = null },

    // --- client actions ---
    startGame() { this.socket?.send({ type: 'start_game' }) },
    setOrder(ids: PlayerId[]) { this.socket?.send({ type: 'set_order', player_ids: ids }) },
    /** Randomise the rotation. Rides the existing `set_order` message — the
     *  host can already drag the list into any order, so there is nothing a
     *  server-side draw would protect. */
    shufflePlayers() {
      const players = this.room?.players
      if (!players?.length) return
      this.setOrder(shuffle(players.map((p) => p.id)))
    },
    endTurn() { this.socket?.send({ type: 'end_turn' }) },
    claimTurn() { this.socket?.send({ type: 'claim_turn' }) },
    undoTurn() { this.socket?.send({ type: 'undo_turn' }) },
    skipPlayer(id: PlayerId) { this.socket?.send({ type: 'skip_player', player_id: id }) },
    removePlayer(id: PlayerId) { this.socket?.send({ type: 'remove_player', player_id: id }) },
    setLocked(locked: boolean) { this.socket?.send({ type: 'set_locked', locked }) },
    nudge() {
      if (this.nudgeCooldownRemaining > 0) return
      this.socket?.send({ type: 'nudge' })
      this._startNudgeCooldown()
    },
  },
})
