// Delayed capture (FEED-18): hold the shot for a few seconds so the reporter
// can open the menu/popup they want in the picture. Ticks once per second
// with the whole seconds left, then `0` right before resolving so the caller
// can take its countdown UI off screen; the trailing frame + settle wait
// lets that removal paint before the engine grabs the frame (a display-media
// frame cannot exclude the widget by selector the way snapDOM does).
const settleMs = 150

function nextFrame(): Promise<void> {
  return new Promise((resolve) => {
    if (typeof requestAnimationFrame === `function`) {
      requestAnimationFrame(() => resolve())
      return
    }
    setTimeout(resolve, 0)
  })
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms))
}

export async function runCountdown(
  delayMs: number,
  onTick?: (secondsLeft: number) => void
): Promise<void> {
  if (!(delayMs > 0)) return
  let remaining = Math.ceil(delayMs / 1000)
  while (remaining > 0) {
    onTick?.(remaining)
    // The last tick may be a fraction of a second (e.g. 2500ms → 3, 2, 1
    // with the first wait being 500ms).
    const step = delayMs - (remaining - 1) * 1000
    await sleep(step)
    delayMs -= step
    remaining -= 1
  }
  onTick?.(0)
  await nextFrame()
  await sleep(settleMs)
}
