import { describe, it, expect, beforeEach } from 'vitest'
import { saveToken, loadToken, clearToken } from '@/services/tokenStore'

describe('tokenStore', () => {
  beforeEach(() => localStorage.clear())

  it('saves and loads a token per room code', () => {
    saveToken('GR7K9P', 'tok-1')
    expect(loadToken('GR7K9P')).toBe('tok-1')
    expect(loadToken('OTHER')).toBeNull()
  })

  it('clears a token', () => {
    saveToken('GR7K9P', 'tok-1')
    clearToken('GR7K9P')
    expect(loadToken('GR7K9P')).toBeNull()
  })
})
