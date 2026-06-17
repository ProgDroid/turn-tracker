import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { createRouter, createMemoryHistory } from 'vue-router'
import LandingView from '@/views/LandingView.vue'
import { i18n } from '@/i18n'

const routes = [
  { path: '/', component: LandingView },
  { path: '/room/:code', component: { template: '<div>room</div>' } },
]

function mountView() {
  const router = createRouter({ history: createMemoryHistory(), routes })
  return { wrapper: mount(LandingView, { global: { plugins: [createPinia(), router, i18n] } }), router }
}

describe('LandingView', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    localStorage.clear()
    vi.restoreAllMocks()
  })

  it('creates a room and navigates to it', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue({
      ok: true,
      json: async () => ({ room_code: 'GR7K9P', player_id: 'p1', token: 'tok' }),
    }))
    const { wrapper, router } = mountView()
    await router.isReady()
    await wrapper.get('[data-test=name]').setValue('Sam')
    await wrapper.get('[data-test=create]').trigger('click')
    await flushPromises()
    expect(localStorage.getItem('tt:token:GR7K9P')).toBe('tok')
    expect(router.currentRoute.value.fullPath).toContain('/room/GR7K9P')
  })
})
