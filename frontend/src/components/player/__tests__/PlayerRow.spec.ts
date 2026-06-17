import { describe, it, expect } from 'vitest'
import { mount } from '@vue/test-utils'
import PlayerRow from '@/components/player/PlayerRow.vue'
import { i18n } from '@/i18n'

const player = { id: 'p1', name: 'Sam', is_host: true, connected: true }

describe('PlayerRow', () => {
  it('shows host badge and a connected label', () => {
    const w = mount(PlayerRow, { props: { player }, global: { plugins: [i18n] } })
    expect(w.text()).toContain('Sam')
    expect(w.text()).toContain('Host')
  })

  it('shows a disconnected label when not connected', () => {
    const w = mount(PlayerRow, {
      props: { player: { ...player, is_host: false, connected: false } },
      global: { plugins: [i18n] },
    })
    expect(w.text().toLowerCase()).toContain('disconnected')
  })

  it('shows skipped state with label and amber-framed row', () => {
    const w = mount(PlayerRow, {
      props: { player: { ...player, is_host: false }, skipped: true },
      global: { plugins: [i18n] },
    })
    expect(w.text().toUpperCase()).toContain('SKIPPED')
    expect(w.find('.row').classes()).toContain('skipped')
  })
})
