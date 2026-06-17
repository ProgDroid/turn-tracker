let ctx: AudioContext | null = null

/** Short two-tone chime via Web Audio — no asset file. No-op if unsupported. */
export function chime(): void {
  const AC = (window.AudioContext || (window as any).webkitAudioContext) as
    | typeof AudioContext
    | undefined
  if (!AC) return
  ctx ??= new AC()
  const now = ctx.currentTime
  for (const [i, freq] of [880, 1320].entries()) {
    const osc = ctx.createOscillator()
    const gain = ctx.createGain()
    osc.frequency.value = freq
    osc.connect(gain)
    gain.connect(ctx.destination)
    const start = now + i * 0.12
    gain.gain.setValueAtTime(0.0001, start)
    gain.gain.exponentialRampToValueAtTime(0.2, start + 0.01)
    gain.gain.exponentialRampToValueAtTime(0.0001, start + 0.18)
    osc.start(start)
    osc.stop(start + 0.2)
  }
}
