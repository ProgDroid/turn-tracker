import { describe, it, expect } from 'vitest'
import type { ServerMessage, ClientMessage, PublicRoom } from '@/types/wire'

describe('wire types', () => {
  it('parses a room_state message shape', () => {
    const msg = JSON.parse(
      '{"type":"room_state","room":{"code":"GR7K9P","state":"lobby","players":[{"id":"p1","name":"Sam","is_host":true,"connected":true}],"current_player_id":null}}',
    ) as ServerMessage
    expect(msg.type).toBe('room_state')
    if (msg.type === 'room_state') {
      const room: PublicRoom = msg.room
      expect(room.players[0].is_host).toBe(true)
      expect(room.current_player_id).toBeNull()
    }
  })

  it('builds a typed client message', () => {
    const m: ClientMessage = { type: 'skip_player', player_id: 'p2' }
    expect(JSON.stringify(m)).toContain('skip_player')
  })
})
