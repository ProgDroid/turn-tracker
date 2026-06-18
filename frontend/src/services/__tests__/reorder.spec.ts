import { describe, it, expect } from 'vitest'
import { reorder, targetIndex } from '@/services/reorder'

describe('reorder', () => {
  it('moves an item forward', () => {
    expect(reorder(['a', 'b', 'c'], 0, 2)).toEqual(['b', 'c', 'a'])
  })
  it('moves an item backward', () => {
    expect(reorder(['a', 'b', 'c'], 2, 0)).toEqual(['c', 'a', 'b'])
  })
  it('is a no-op when from === to', () => {
    expect(reorder(['a', 'b', 'c'], 1, 1)).toEqual(['a', 'b', 'c'])
  })
  it('does not mutate the input', () => {
    const input = ['a', 'b', 'c']
    reorder(input, 0, 2)
    expect(input).toEqual(['a', 'b', 'c'])
  })
})

describe('targetIndex', () => {
  const rects = [
    { top: 0, height: 50 },
    { top: 60, height: 50 },
    { top: 120, height: 50 },
  ] as DOMRect[]

  it('returns the row whose midpoint is below the pointer', () => {
    expect(targetIndex(10, rects)).toBe(0) // above row 0 midpoint (25)
    expect(targetIndex(80, rects)).toBe(1) // below row 0/1 start, under row 1 mid (85)
    expect(targetIndex(130, rects)).toBe(2)
  })
  it('clamps past the last row to the last index', () => {
    expect(targetIndex(9999, rects)).toBe(2)
  })
  it('returns 0 for an empty list', () => {
    expect(targetIndex(42, [])).toBe(0)
  })
})
