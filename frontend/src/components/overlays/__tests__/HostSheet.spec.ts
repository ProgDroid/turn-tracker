import { describe, it, expect, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import { setActivePinia, createPinia } from 'pinia'
import HostSheet from '@/components/overlays/HostSheet.vue'
import { useRoomStore } from '@/stores/room'
import { i18n } from '@/i18n'
import type { PublicRoom } from '@/types/wire'

function room(): PublicRoom {
  return {
    code: 'GR7K9P', state: 'active', locked: false, current_player_id: 'p2', turn_elapsed_secs: null,
    players: [
      { id: 'p1', name: 'Sam', is_host: true, connected: true },
      { id: 'p2', name: 'Alice', is_host: false, connected: true },
    ],
  }
}

function setup() {
  setActivePinia(createPinia())
  const s = useRoomStore()
  s.me = { playerId: 'p1', token: 't' }
  s._handle({ type: 'room_state', room: room() })
  const w = mount(HostSheet, { props: { open: true }, global: { plugins: [i18n] } })
  return { s, w }
}

describe('HostSheet', () => {
  it('is hidden when open is false', () => {
    setActivePinia(createPinia())
    const w = mount(HostSheet, { props: { open: false }, global: { plugins: [i18n] } })
    expect(w.find('.sheet').exists()).toBe(false)
  })

  it('renders the four host actions and the current player header', () => {
    const { w } = setup()
    expect(w.text()).toContain('Skip this turn')
    expect(w.text()).toContain('Undo last turn')
    expect(w.text()).toContain('Reorder players')
    expect(w.text()).toContain('Remove from room')
    expect(w.text()).toContain('Alice') // current-player header
  })

  it('skip targets the current player and closes', async () => {
    const { s, w } = setup()
    const spy = vi.spyOn(s, 'skipPlayer')
    await w.findAll('button.item')[0].trigger('click')
    expect(spy).toHaveBeenCalledWith('p2')
    expect(w.emitted('close')).toBeTruthy()
  })

  it('reorder emits a reorder event (not a store action)', async () => {
    const { w } = setup()
    const reorderBtn = w.findAll('button.item').find((b) => b.text().includes('Reorder'))
    await reorderBtn!.trigger('click')
    expect(w.emitted('reorder')).toBeTruthy()
  })

  it('remove targets the current player', async () => {
    const { s, w } = setup()
    const spy = vi.spyOn(s, 'removePlayer')
    const removeBtn = w.findAll('button.item').find((b) => b.text().includes('Remove'))
    await removeBtn!.trigger('click')
    expect(spy).toHaveBeenCalledWith('p2')
  })
})

describe('HostSheet shuffle', () => {
  it('offers shuffling the order', () => {
    const { w } = setup()
    expect(w.text()).toContain('Shuffle order')
  })

  it('shuffling dispatches a new order and closes the sheet', async () => {
    const { s, w } = setup()
    const spy = vi.spyOn(s, 'shufflePlayers')
    await w.get('[data-test="shuffle"]').trigger('click')
    expect(spy).toHaveBeenCalled()
    expect(w.emitted('close')).toBeTruthy()
  })
})

describe('HostSheet when the host is alone', () => {
  function soloSetup() {
    setActivePinia(createPinia())
    const s = useRoomStore()
    s.me = { playerId: 'p1', token: 't' }
    s._handle({
      type: 'room_state',
      room: {
        code: 'GR7K9P', state: 'active', locked: false, current_player_id: 'p1',
        turn_elapsed_secs: 0,
        players: [{ id: 'p1', name: 'Sam', is_host: true, connected: true }],
      },
    })
    return { s, w: mount(HostSheet, { props: { open: true }, global: { plugins: [i18n] } }) }
  }

  it('calls the action what it does — closing the room, not removing a player', () => {
    const { w } = soloSetup()
    expect(w.text()).toContain('Close room')
    expect(w.text()).not.toContain('Remove from room')
  })

  it('still says remove when there is someone to remove', () => {
    const { w } = setup()
    expect(w.text()).toContain('Remove from room')
    expect(w.text()).not.toContain('Close room')
  })
})

