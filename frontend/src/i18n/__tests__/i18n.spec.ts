import { describe, it, expect } from 'vitest'
import { i18n, errorKey } from '@/i18n'

describe('i18n', () => {
  it('resolves a known key', () => {
    expect(i18n.global.t('landing.createTitle')).toContain('game room')
  })
  it('maps server error codes to message keys', () => {
    expect(i18n.global.t(errorKey('not_your_turn'))).toMatch(/not your turn/i)
    expect(i18n.global.t(errorKey('nudge_cooldown'))).toMatch(/cooldown|seconds/i)
  })
})
