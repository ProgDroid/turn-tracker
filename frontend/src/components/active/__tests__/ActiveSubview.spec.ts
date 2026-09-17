import { describe, it, expect } from 'vitest'
import { mount } from '@vue/test-utils'
import { setActivePinia, createPinia } from 'pinia'
import ActiveSubview from '@/components/active/ActiveSubview.vue'
import TurnEmblem from '@/components/active/TurnEmblem.vue'
import HostSheet from '@/components/overlays/HostSheet.vue'
import { useRoomStore } from '@/stores/room'
import { i18n } from '@/i18n'
import type { PublicRoom } from '@/types/wire'

function room(current: string, elapsed: number | null = 0): PublicRoom {
  return {
    code: 'GR7K9P', state: 'active', locked: false, current_player_id: current,
    turn_elapsed_secs: elapsed,
    players: [
      { id: 'p1', name: 'Sam', is_host: true, connected: true },
      { id: 'p2', name: 'Alice', is_host: false, connected: true },
      { id: 'p3', name: 'Bob', is_host: false, connected: true },
    ],
  }
}

function setup(meId: string, current: string, elapsed: number | null = 0) {
  setActivePinia(createPinia())
  const s = useRoomStore()
  s.me = { playerId: meId, token: 't' }
  s._handle({ type: 'room_state', room: room(current, elapsed) })
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

  it('renders the Up next strip on the watching screen', () => {
    const w = setup('p1', 'p2') // p1 watching; up next = Bob, Sam(you)
    expect(w.text()).toContain('Up next')
  })

  it('shows the host control button only for the host', () => {
    const host = setup('p1', 'p2') // p1 is host, watching
    expect(host.find('.host-btn').exists()).toBe(true)
    const guest = setup('p3', 'p2') // p3 not host
    expect(guest.find('.host-btn').exists()).toBe(false)
  })

  it('host button opens the host override sheet', async () => {
    const w = setup('p1', 'p2')
    expect(w.findComponent(HostSheet).props('open')).toBe(false)
    await w.find('.host-btn').trigger('click')
    expect(w.findComponent(HostSheet).props('open')).toBe(true)
  })

  it('speeds up the emblem pulse for the current player on a received nudge', async () => {
    const w = setup('p2', 'p2') // my turn -> hero with emblem
    expect(w.findComponent(TurnEmblem).props('fast')).toBe(false)
    const s = useRoomStore()
    s._handle({ type: 'nudged' })
    await w.vm.$nextTick()
    expect(w.findComponent(TurnEmblem).props('fast')).toBe(true)
  })
})

describe('ActiveSubview turn clock', () => {
  it('shows how long my own turn has run', () => {
    const w = setup('p2', 'p2', 84)
    expect(w.get('[data-test="turn-clock"]').text()).toContain('1:24')
  })

  it('shows how long the current player has been taking, to everyone watching', () => {
    const w = setup('p1', 'p2', 125)
    expect(w.get('[data-test="turn-clock"]').text()).toContain('2:05')
  })

  it('gives the bare number context for screen readers', () => {
    const w = setup('p2', 'p2', 84)
    const clock = w.get('[data-test="turn-clock"]')
    expect(clock.attributes('datetime')).toBe('PT84S')
    expect(clock.get('.tt-sr-only').text()).toBe('on this turn')
  })

  it('shows the clock to the player who is up next', () => {
    // The claim screen is where "how long have they been at it?" matters most:
    // it is the information you weigh before pulling your turn forward.
    const w = setup('p3', 'p2', 42)
    expect(w.text()).toContain('Claim my turn')
    expect(w.get('[data-test="turn-clock"]').text()).toContain('0:42')
  })

  it('omits the clock entirely when the server reports no turn in progress', () => {
    const w = setup('p1', 'p2', null)
    expect(w.find('[data-test="turn-clock"]').exists()).toBe(false)
  })
})
