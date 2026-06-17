type SentinelLike = { release: () => Promise<void> }
let sentinel: SentinelLike | null = null

export async function acquire(): Promise<void> {
  const wl = (navigator as Navigator & { wakeLock?: { request: (t: 'screen') => Promise<SentinelLike> } }).wakeLock
  if (!wl) return
  try { sentinel = await wl.request('screen') } catch { /* denied / unsupported */ }
}

export async function release(): Promise<void> {
  try { await sentinel?.release() } catch { /* ignore */ }
  sentinel = null
}
