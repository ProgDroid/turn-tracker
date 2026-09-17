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

/** Inclusive-upper-bound index source, injectable so shuffling is testable. */
export type IndexSource = (maxInclusive: number) => number

const randomIndex: IndexSource = (max) => Math.floor(Math.random() * (max + 1))

/**
 * Fisher-Yates: return a uniformly random permutation of `arr`, leaving the
 * input untouched. `rnd` is asked for an index in `[0, i]` on each step.
 */
export function shuffle<T>(arr: readonly T[], rnd: IndexSource = randomIndex): T[] {
  const out = arr.slice()
  for (let i = out.length - 1; i > 0; i--) {
    const j = rnd(i)
    ;[out[i], out[j]] = [out[j], out[i]]
  }
  return out
}
