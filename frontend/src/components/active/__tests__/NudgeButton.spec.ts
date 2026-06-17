import { describe, it, expect, beforeEach, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import { setActivePinia, createPinia } from 'pinia'
import NudgeButton from '@/components/active/NudgeButton.vue'
import { useRoomStore } from '@/stores/room'
import { i18n } from '@/i18n'

describe('NudgeButton', () => {
  beforeEach(() => { setActivePinia(createPinia()); vi.restoreAllMocks() })

  it('calls store.nudge on click and shows label', () => {
    const store = useRoomStore()
    const spy = vi.spyOn(store, 'nudge')
    const w = mount(NudgeButton, { props: { name: 'Alice' }, global: { plugins: [i18n] } })
    expect(w.text()).toContain('Nudge Alice')
    w.get('button').trigger('click')
    expect(spy).toHaveBeenCalled()
  })

  it('shows the wait countdown when cooling down', async () => {
    const store = useRoomStore()
    store.nudgeCooldownRemaining = 6
    const w = mount(NudgeButton, { props: { name: 'Alice' }, global: { plugins: [i18n] } })
    expect(w.text()).toContain('Wait 6s')
  })
})
