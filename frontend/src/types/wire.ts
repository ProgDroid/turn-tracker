export type PlayerId = string

export interface PublicPlayer {
  id: PlayerId
  name: string
  is_host: boolean
  connected: boolean
}

export type PublicState = 'lobby' | 'active'

export interface PublicRoom {
  code: string
  state: PublicState
  locked: boolean
  players: PublicPlayer[]
  current_player_id: PlayerId | null
  /** Whole seconds the current player has held the turn; null outside active play. */
  turn_elapsed_secs: number | null
}

export type ServerMessage =
  | { type: 'welcome'; player_id: PlayerId; token: string }
  | { type: 'room_state'; room: PublicRoom }
  | { type: 'nudged' }
  | { type: 'error'; code: string; message: string }

export type ClientMessage =
  | { type: 'join'; player_token?: string; player_name?: string }
  | { type: 'start_game' }
  | { type: 'set_order'; player_ids: PlayerId[] }
  | { type: 'end_turn' }
  | { type: 'claim_turn' }
  | { type: 'undo_turn' }
  | { type: 'skip_player'; player_id: PlayerId }
  | { type: 'remove_player'; player_id: PlayerId }
  | { type: 'set_locked'; locked: boolean }
  | { type: 'nudge' }

/** Known server error codes (see src/ws/dispatch.rs). */
export type ErrorCode =
  | 'not_authorized'
  | 'not_found'
  | 'wrong_state'
  | 'not_your_turn'
  | 'nudge_cooldown'
  | 'room_full'
  | 'room_locked'
