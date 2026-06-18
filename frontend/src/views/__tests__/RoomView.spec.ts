import { describe, it, expect, beforeEach, vi } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import { setActivePinia, createPinia } from 'pinia'
import { createRouter, createMemoryHistory } from 'vue-router'
import RoomView from '@/views/RoomView.vue'
import { useRoomStore } from '@/stores/room'
import { i18n } from '@/i18n'

const routes = [
  { path: '/', component: { template: '<div>landing</div>' } },
  { path: '/room/:code', component: RoomView, props: true },
]

const fakeSocket = () => ({
  connect: vi.fn(),
  send: vi.fn(),
  close: vi.fn(),
  onStatusChange: vi.fn(),
})

async function mountAt(code: string) {
  setActivePinia(createPinia())
  // Inject a fake socket BEFORE mount so onMounted's connect() never constructs
  // a real WebSocket (jsdom has none).
  const store = useRoomStore()
  store._setSocket(fakeSocket() as any)
  const router = createRouter({ history: createMemoryHistory(), routes })
  router.push(`/room/${code}`)
  await router.isReady()
  const wrapper = mount(RoomView, { props: { code }, global: { plugins: [router, i18n] } })
  // Drain the microtask queue so onMounted's queueMicrotask(() => store.join())
  // has run and the join is observable on the fake socket.
  await flushPromises()
  return { wrapper, router, store }
}

describe('RoomView', () => {
  beforeEach(() => { localStorage.clear(); vi.restoreAllMocks() })

  it('connects on mount and rejoins by token when one is saved', async () => {
    localStorage.setItem('tt:token:GR7K9P', 'tok')
    const { store } = await mountAt('GR7K9P')
    expect(store.codeInView).toBe('GR7K9P')
    expect(store.socket!.connect).toHaveBeenCalled()
    // A saved token → join is dispatched after connect via queueMicrotask.
    expect(store.socket!.send).toHaveBeenCalled()
  })

  it('prompts for a name (and does not join) for a tokenless shared-link visitor', async () => {
    const { store, wrapper } = await mountAt('GR7K9P')
    expect(store.socket!.connect).toHaveBeenCalled()
    expect(store.socket!.send).not.toHaveBeenCalled()
    expect(wrapper.find('[data-test=join-name]').exists()).toBe(true)
    await wrapper.find('[data-test=join-name]').setValue('Sam')
    await wrapper.find('[data-test=join-submit]').trigger('click')
    expect(store.socket!.send).toHaveBeenCalled()
  })

  it('routes home when the room is gone', async () => {
    const { store, router } = await mountAt('GR7K9P')
    store.roomGone = true
    await flushPromises()
    expect(router.currentRoute.value.path).toBe('/')
  })
})
