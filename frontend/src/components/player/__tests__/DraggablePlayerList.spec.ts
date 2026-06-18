import { describe, it, expect } from 'vitest'
import { mount } from '@vue/test-utils'
import DraggablePlayerList from '@/components/player/DraggablePlayerList.vue'
import PlayerRow from '@/components/player/PlayerRow.vue'
import { i18n } from '@/i18n'
import type { PublicPlayer } from '@/types/wire'

const players: PublicPlayer[] = [
  { id: 'p1', name: 'Sam', is_host: true, connected: true },
  { id: 'p2', name: 'Bob', is_host: false, connected: true },
  { id: 'p3', name: 'Eve', is_host: false, connected: true },
]

function mountList() {
  return mount(DraggablePlayerList, {
    props: { players, meId: 'p1' },
    global: { plugins: [i18n] },
  })
}

// jsdom has no layout, so feed each row a fake rect (50px tall, 10px gaps).
function stubRects(w: ReturnType<typeof mountList>) {
  const tops = [0, 60, 120]
  w.findAll('.drag-item').forEach((iw, i) => {
    ;(iw.element as HTMLElement).getBoundingClientRect = () =>
      ({ top: tops[i], height: 50, bottom: tops[i] + 50, left: 0, right: 0, width: 0, x: 0, y: tops[i], toJSON() {} }) as DOMRect
  })
}

describe('DraggablePlayerList', () => {
  it('renders a draggable row with a grip handle per player', () => {
    const w = mountList()
    expect(w.findAllComponents(PlayerRow)).toHaveLength(3)
    expect(w.findAll('.handle')).toHaveLength(3)
  })

  it('dragging the first row down past the others emits the reordered ids', async () => {
    const w = mountList()
    stubRects(w)
    // Press the first row's handle, then release low enough to land in the last slot.
    // (clientY is set on the pointerup; the pointerdown baseline is unused here.)
    await w.findAllComponents(PlayerRow)[0].find('.handle').trigger('pointerdown')
    window.dispatchEvent(new MouseEvent('pointerup', { clientY: 130 }))
    await w.vm.$nextTick()
    expect(w.emitted('reorder')?.[0]?.[0]).toEqual(['p2', 'p3', 'p1'])
  })

  it('does not emit when the row is dropped in place', async () => {
    const w = mountList()
    stubRects(w)
    await w.findAllComponents(PlayerRow)[0].find('.handle').trigger('pointerdown')
    window.dispatchEvent(new MouseEvent('pointerup', { clientY: 10 }))
    await w.vm.$nextTick()
    expect(w.emitted('reorder')).toBeUndefined()
  })
})
