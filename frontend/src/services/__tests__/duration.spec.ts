import { describe, it, expect } from 'vitest'
import { formatElapsed } from '@/services/duration'

describe('formatElapsed', () => {
  it('renders a fresh turn as zero', () => {
    expect(formatElapsed(0)).toBe('0:00')
  })
  it('zero-pads seconds under ten', () => {
    expect(formatElapsed(7)).toBe('0:07')
  })
  it('rolls into minutes', () => {
    expect(formatElapsed(84)).toBe('1:24')
  })
  it('keeps counting minutes past an hour rather than adding a field', () => {
    expect(formatElapsed(3605)).toBe('60:05')
  })
  it('clamps a negative input to zero', () => {
    expect(formatElapsed(-5)).toBe('0:00')
  })
})
