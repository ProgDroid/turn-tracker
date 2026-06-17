import { describe, it, expect, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import { setActivePinia, createPinia } from 'pinia'
import ReorderSheet from '@/components/overlays/ReorderSheet.vue'
import PlayerRow from '@/components/player/PlayerRow.vue'
import { useRoomStore } from '@/stores/room'
import { i18n } from '@/i18n'
import type { PublicRoom } from '@/types/wire'

function room(): PublicRoom {
  return {
    code: 'GR7K9P', state: 'active', current_player_id: 'p1',
    players: [
      { id: 'p1', name: 'Sam', is_host: true, connected: true },
      { id: 'p2', name: 'Alice', is_host: false, connected: true },
      { id: 'p3', name: 'Bob', is_host: false, connected: true },
    ],
  }
}

function setup() {
  setActivePinia(createPinia())
  const s = useRoomStore()
  s.me = { playerId: 'p1', token: 't' }
  s._handle({ type: 'room_state', room: room() })
  const w = mount(ReorderSheet, { props: { open: true }, global: { plugins: [i18n] } })
  return { s, w }
}

describe('ReorderSheet', () => {
  it('lists every player as a draggable row', () => {
    const { w } = setup()
    expect(w.text()).toContain('Sam')
    expect(w.text()).toContain('Alice')
    expect(w.text()).toContain('Bob')
  })

  it('committing a drag sends the reordered ids via set_order', async () => {
    const { s, w } = setup()
    const spy = vi.spyOn(s, 'setOrder')
    const rows = w.findAllComponents(PlayerRow)
    // drag the first row (p1) onto the third slot (p3)
    await rows[0].trigger('dragstart')
    await rows[2].trigger('drop')
    expect(spy).toHaveBeenCalledWith(['p2', 'p3', 'p1'])
  })

  it('Done emits close', async () => {
    const { w } = setup()
    await w.find('button.done').trigger('click')
    expect(w.emitted('close')).toBeTruthy()
  })
})
