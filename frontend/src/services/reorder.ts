/** Move the item at `from` to position `to`, returning a new array. */
export function reorder<T>(arr: readonly T[], from: number, to: number): T[] {
  const copy = arr.slice()
  const [moved] = copy.splice(from, 1)
  copy.splice(to, 0, moved)
  return copy
}

/**
 * Pick the insertion index for a pointer at vertical position `y`, given the
 * bounding rects of each row (in DOM order). Returns the first row whose
 * vertical midpoint sits below the pointer, else the last index. Empty → 0.
 */
export function targetIndex(y: number, rowRects: readonly DOMRect[]): number {
  for (let i = 0; i < rowRects.length; i++) {
    const r = rowRects[i]
    if (y < r.top + r.height / 2) return i
  }
  return Math.max(0, rowRects.length - 1)
}
