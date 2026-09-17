import { describe, it, expect, beforeEach, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import { setActivePinia, createPinia } from 'pinia'
import LobbySubview from '@/components/lobby/LobbySubview.vue'
import DraggablePlayerList from '@/components/player/DraggablePlayerList.vue'
import { useRoomStore } from '@/stores/room'
import { i18n } from '@/i18n'

function withHost(isHost: boolean) {
  const s = useRoomStore()
  s.me = { playerId: isHost ? 'p1' : 'p2', token: 't' }
  s._handle({
    type: 'room_state',
    room: {
      code: 'GR7K9P', state: 'lobby', locked: false, current_player_id: null, turn_elapsed_secs: null,
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

  it('host reordering the player list dispatches the new order via set_order', async () => {
    const s = useRoomStore()
    s.me = { playerId: 'p1', token: 't' }
    s._handle({
      type: 'room_state',
      room: {
        code: 'GR7K9P', state: 'lobby', locked: false, current_player_id: null, turn_elapsed_secs: null,
        players: [
          { id: 'p1', name: 'Sam', is_host: true, connected: true },
          { id: 'p2', name: 'Bob', is_host: false, connected: true },
          { id: 'p3', name: 'Eve', is_host: false, connected: true },
        ],
      },
    })
    const setOrder = vi.spyOn(s, 'setOrder')
    const w = mount(LobbySubview, { global: { plugins: [i18n] } })
    // The draggable list owns the pointer interaction (covered in its own spec);
    // here we assert the lobby wires its reorder event through to the store.
    w.findComponent(DraggablePlayerList).vm.$emit('reorder', ['p2', 'p3', 'p1'])
    expect(setOrder).toHaveBeenCalledWith(['p2', 'p3', 'p1'])
  })

  it('host can toggle the room lock from the lobby', async () => {
    const s = withHost(true)
    const setLocked = vi.spyOn(s, 'setLocked')
    const w = mount(LobbySubview, { global: { plugins: [i18n] } })
    const toggle = w.find('[data-test=lock-toggle]')
    expect(toggle.exists()).toBe(true)
    // Room starts unlocked → first click locks it.
    await toggle.trigger('click')
    expect(setLocked).toHaveBeenCalledWith(true)
  })

  it('non-host does not see the lock toggle', () => {
    withHost(false)
    const w = mount(LobbySubview, { global: { plugins: [i18n] } })
    expect(w.find('[data-test=lock-toggle]').exists()).toBe(false)
  })
})

describe('LobbySubview shuffle', () => {
  beforeEach(() => setActivePinia(createPinia()))

  it('host can shuffle the turn order', async () => {
    const s = withHost(true)
    const spy = vi.spyOn(s, 'shufflePlayers')
    const w = mount(LobbySubview, { global: { plugins: [i18n] } })
    await w.get('[data-test="shuffle"]').trigger('click')
    expect(spy).toHaveBeenCalled()
  })

  it('non-host is not offered the shuffle', () => {
    withHost(false)
    const w = mount(LobbySubview, { global: { plugins: [i18n] } })
    expect(w.find('[data-test="shuffle"]').exists()).toBe(false)
  })
})
