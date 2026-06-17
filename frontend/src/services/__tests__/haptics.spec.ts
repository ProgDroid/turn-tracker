import { describe, it, expect, vi, afterEach } from 'vitest'
import { vibrate } from '@/services/haptics'

describe('haptics', () => {
  afterEach(() => {
    vi.restoreAllMocks()
  })

  it('calls navigator.vibrate when available', () => {
    const spy = vi.fn()
    vi.stubGlobal('navigator', { vibrate: spy })
    vibrate([0, 80, 40, 80])
    expect(spy).toHaveBeenCalledWith([0, 80, 40, 80])
  })

  it('is a no-op when vibrate is missing', () => {
    vi.stubGlobal('navigator', {})
    expect(() => vibrate([10])).not.toThrow()
  })
})
