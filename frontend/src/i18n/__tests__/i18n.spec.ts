import { describe, it, expect } from 'vitest'
import { i18n, errorKey } from '@/i18n'

// Mirrors every `code:` the server can send that a player can do something
// about — src/ws/dispatch.rs (error_message) and src/ws/connection.rs.
const ACTIONABLE_CODES = [
  'not_authorized', 'not_found', 'wrong_state', 'not_your_turn',
  'nudge_cooldown', 'room_full', 'room_locked', 'bad_name', 'not_joined',
]

describe('i18n', () => {
  it('resolves a known key', () => {
    expect(i18n.global.t('landing.createTitle')).toContain('game room')
  })

  it('maps server error codes to message keys', () => {
    expect(i18n.global.t(errorKey('not_your_turn'))).toMatch(/not your turn/i)
    expect(i18n.global.t(errorKey('nudge_cooldown'))).toMatch(/cooldown|seconds/i)
  })

  it.each(ACTIONABLE_CODES)('gives %s its own message rather than the fallback', (code) => {
    expect(errorKey(code)).toBe(`errors.${code}`)
    expect(i18n.global.t(errorKey(code))).not.toBe(i18n.global.t('errors.generic'))
  })

  it('falls back for bad_message, which the user cannot act on', () => {
    // A frame the server could not parse is a client bug; "something went
    // wrong" is the honest thing to say, so this fallback is deliberate.
    expect(errorKey('bad_message')).toBe('errors.generic')
  })

  it('falls back for a code it has never heard of', () => {
    expect(errorKey('some_future_code')).toBe('errors.generic')
  })
})
