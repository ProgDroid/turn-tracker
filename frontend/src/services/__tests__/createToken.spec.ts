import { describe, it, expect, beforeEach } from 'vitest'
import { captureCreateTokenFromUrl, loadCreateToken } from '@/services/createToken'

function setUrl(url: string) {
  window.history.replaceState({}, '', url)
}

describe('createToken', () => {
  beforeEach(() => {
    localStorage.clear()
    setUrl('/')
  })

  it('returns null when nothing has been captured', () => {
    expect(loadCreateToken()).toBeNull()
  })

  it('captures ?k= and persists it', () => {
    setUrl('/?k=abc123')
    captureCreateTokenFromUrl()
    expect(loadCreateToken()).toBe('abc123')
  })

  it('strips the token from the URL so it is not shared or logged', () => {
    setUrl('/?k=abc123')
    captureCreateTokenFromUrl()
    expect(window.location.search).not.toContain('abc123')
  })

  it('preserves other query parameters while stripping k', () => {
    setUrl('/?k=abc123&debug=1')
    captureCreateTokenFromUrl()
    expect(window.location.search).toContain('debug=1')
    expect(window.location.search).not.toContain('abc123')
  })

  it('keeps a previously stored token when the URL has none', () => {
    setUrl('/?k=abc123')
    captureCreateTokenFromUrl()
    setUrl('/')
    captureCreateTokenFromUrl()
    expect(loadCreateToken()).toBe('abc123')
  })

  it('overwrites a stored token when a new one arrives', () => {
    setUrl('/?k=old')
    captureCreateTokenFromUrl()
    setUrl('/?k=new')
    captureCreateTokenFromUrl()
    expect(loadCreateToken()).toBe('new')
  })
})
