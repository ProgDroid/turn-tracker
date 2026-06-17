export function vibrate(pattern: number | number[]): void {
  const nav = navigator as Navigator & { vibrate?: (p: number | number[]) => boolean }
  if (typeof nav.vibrate === 'function') nav.vibrate(pattern)
}
