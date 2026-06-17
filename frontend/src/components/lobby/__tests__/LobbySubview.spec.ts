import { describe, it, expect, beforeEach, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import { setActivePinia, createPinia } from 'pinia'
import LobbySubview from '@/components/lobby/LobbySubview.vue'
import PlayerRow from '@/components/player/PlayerRow.vue'
import { useRoomStore } from '@/stores/room'
import { i18n } from '@/i18n'

function withHost(isHost: boolean) {
  const s = useRoomStore()
  s.me = { playerId: isHost ? 'p1' : 'p2', token: 't' }
  s._handle({
    type: 'room_state',
    room: {
      code: 'GR7K9P', state: 'lobby', current_player_id: null,
      players: [
        { id: 'p1', name: 'Sam', is_host: true, connected: true },
        { id: 'p2', name: 'Bob', is_host: false, connected: true },
      ],
    },
  })
  return s
}

describe('LobbySubview', () => {
  beforeEach(() => setActivePinia(createPinia()))

  it('host sees Start button', () => {
    withHost(true)
    const w = mount(LobbySubview, { global: { plugins: [i18n] } })
    expect(w.text()).toContain('Start game')
  })

  it('non-host sees waiting message', () => {
    withHost(false)
    const w = mount(LobbySubview, { global: { plugins: [i18n] } })
    expect(w.text()).toContain('Waiting for Sam')
  })

  it('host dragging a player to a new slot dispatches the reordered list', async () => {
    const s = useRoomStore()
    s.me = { playerId: 'p1', token: 't' }
    s._handle({
      type: 'room_state',
      room: {
        code: 'GR7K9P', state: 'lobby', current_player_id: null,
        players: [
          { id: 'p1', name: 'Sam', is_host: true, connected: true },
          { id: 'p2', name: 'Bob', is_host: false, connected: true },
          { id: 'p3', name: 'Eve', is_host: false, connected: true },
        ],
      },
    })
    const setOrder = vi.spyOn(s, 'setOrder')
    const w = mount(LobbySubview, { global: { plugins: [i18n] } })
    const rows = w.findAllComponents(PlayerRow)
    // Drag the first player (p1) and drop it onto the third slot (p3).
    await rows[0].trigger('dragstart')
    await rows[2].trigger('drop')
    expect(setOrder).toHaveBeenCalledWith(['p2', 'p3', 'p1'])
  })
})
