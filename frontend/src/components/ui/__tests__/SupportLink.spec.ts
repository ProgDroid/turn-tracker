import { describe, it, expect } from 'vitest'
import { mount } from '@vue/test-utils'
import SupportLink from '@/components/ui/SupportLink.vue'
import { i18n } from '@/i18n'

function mountLink() {
  return mount(SupportLink, { global: { plugins: [i18n] } })
}

describe('SupportLink', () => {
  it('points at the Ko-fi page', () => {
    expect(mountLink().get('a').attributes('href')).toBe('https://ko-fi.com/progdroid1984')
  })

  it('opens in a new tab without leaking the opener', () => {
    const a = mountLink().get('a')
    expect(a.attributes('target')).toBe('_blank')
    // noopener stops the new tab reaching back via window.opener; noreferrer
    // also withholds the Referer, which would carry the room code in the path.
    const rel = a.attributes('rel') ?? ''
    expect(rel).toContain('noopener')
    expect(rel).toContain('noreferrer')
  })

  it('renders translated copy rather than a bare literal', () => {
    // Guards the project's i18n rule: every user-visible string goes through t().
    expect(mountLink().text()).toBe(i18n.global.t('support.kofi'))
    expect(mountLink().text().length).toBeGreaterThan(0)
  })

  it('carries an accessible label, since the icon alone is decorative', () => {
    expect(mountLink().get('a').attributes('aria-label')).toBeTruthy()
  })

  it('hides the inline icon from assistive tech', () => {
    // The icon repeats what the text already says; announcing it is noise.
    expect(mountLink().get('svg').attributes('aria-hidden')).toBe('true')
  })
})
