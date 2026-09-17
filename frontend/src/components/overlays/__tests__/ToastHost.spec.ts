import { describe, it, expect, beforeEach } from 'vitest'
import { mount } from '@vue/test-utils'
import { setActivePinia, createPinia } from 'pinia'
import ToastHost from '@/components/overlays/ToastHost.vue'
import { useRoomStore } from '@/stores/room'
import { i18n } from '@/i18n'

describe('ToastHost', () => {
  beforeEach(() => setActivePinia(createPinia()))

  it('renders a localized message for the current error code', async () => {
    const store = useRoomStore()
    const w = mount(ToastHost, { global: { plugins: [i18n] } })
    store.lastError = { code: 'not_your_turn' }
    await w.vm.$nextTick()
    expect(w.text()).toContain("not your turn")
  })

  it('shows reconnecting banner when status is reconnecting', async () => {
    const store = useRoomStore()
    const w = mount(ToastHost, { global: { plugins: [i18n] } })
    store.connStatus = 'reconnecting'
    await w.vm.$nextTick()
    expect(w.text()).toContain('Reconnecting')
  })
})
