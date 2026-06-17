import { describe, it, expect } from 'vitest'
import { mount } from '@vue/test-utils'
import { setActivePinia, createPinia } from 'pinia'
import ActiveSubview from '@/components/active/ActiveSubview.vue'
import { useRoomStore } from '@/stores/room'
import { i18n } from '@/i18n'
import type { PublicRoom } from '@/types/wire'

function room(current: string): PublicRoom {
  return {
    code: 'GR7K9P', state: 'active', current_player_id: current,
    players: [
      { id: 'p1', name: 'Sam', is_host: true, connected: true },
      { id: 'p2', name: 'Alice', is_host: false, connected: true },
      { id: 'p3', name: 'Bob', is_host: false, connected: true },
    ],
  }
}

function setup(meId: string, current: string) {
  setActivePinia(createPinia())
  const s = useRoomStore()
  s.me = { playerId: meId, token: 't' }
  s._handle({ type: 'room_state', room: room(current) })
  return mount(ActiveSubview, { global: { plugins: [i18n] } })
}

describe('ActiveSubview', () => {
  it('shows YOUR TURN hero when it is my turn', () => {
    const w = setup('p2', 'p2')
    expect(w.text().toLowerCase()).toContain("it's your turn")
  })
  it('shows claim screen when I am next', () => {
    const w = setup('p3', 'p2') // current p2 idx1, next idx2 = p3
    expect(w.text()).toContain('Claim my turn')
  })
  it('shows current-player view otherwise', () => {
    const w = setup('p1', 'p2') // p1 is 2 away
    expect(w.text()).toContain("Alice's turn")
  })
  it('clicking "Wait my turn" dismisses the claim prompt and shows the watching view', async () => {
    const w = setup('p3', 'p2') // p3 is next -> claim screen
    expect(w.text()).toContain('Claim my turn')
    const waitBtn = w.findAll('button').find((b) => b.text() === 'Wait my turn')
    expect(waitBtn).toBeTruthy()
    await waitBtn!.trigger('click')
    expect(w.text()).not.toContain('Claim my turn')
    expect(w.text()).toContain("Alice's turn") // fell through to the passive watch view
  })
})
