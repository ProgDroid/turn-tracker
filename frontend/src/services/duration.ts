/**
 * Render whole seconds as `m:ss`. Minutes are unbounded — a turn that runs past
 * an hour reads `60:05`, which stays scannable at a glance where `1:00:05`
 * invites a misread as one minute.
 */
export function formatElapsed(secs: number): string {
  const total = Math.max(0, Math.floor(secs))
  const mins = Math.floor(total / 60)
  const rem = total % 60
  return `${mins}:${String(rem).padStart(2, '0')}`
}
