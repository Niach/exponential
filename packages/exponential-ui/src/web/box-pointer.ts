// #EDE-247: a Box's `auxPress` and `scroll` in the DOM, shared by the React
// and Solid Box (fixtures/interactions-round1.json `boxPointer`). The middle
// button fires on press, the right one on release (its context menu is
// swallowed); a wheel fires `scroll` once per step: a notch (3 lines) in
// line mode, `WHEEL_PIXEL_STEP` accumulated pixels in pixel mode.

export type BoxAuxButton = `middle` | `right`
export type BoxScrollDirection = `up` | `down` | `left` | `right`

/** A typical browser notch in pixel mode. */
export const WHEEL_PIXEL_STEP = 100
/** Lines per notch in line mode. */
export const WHEEL_LINES_PER_NOTCH = 3

/** The pixel remainder a Box carries between wheel events, per axis. */
export interface WheelAccumulator {
  x: number
  y: number
}

/** The steps of one wheel event (DOM: deltaY < 0 = up, deltaX < 0 = left);
 *  pixel mode adds into `acc` (a flip of direction starts it over). */
export function wheelSteps(e: { deltaX: number; deltaY: number; deltaMode: number }, acc: WheelAccumulator): BoxScrollDirection[] {
  const out: BoxScrollDirection[] = []
  const axis = (delta: number, key: `x` | `y`, neg: BoxScrollDirection, pos: BoxScrollDirection) => {
    if (!delta) return
    const dir = delta < 0 ? neg : pos
    let n: number
    if (e.deltaMode === 0) {
      const prev = acc[key]
      acc[key] = (Math.sign(prev) === Math.sign(delta) ? prev : 0) + delta
      n = Math.floor(Math.abs(acc[key]) / WHEEL_PIXEL_STEP)
      acc[key] -= Math.sign(acc[key]) * n * WHEEL_PIXEL_STEP
    } else {
      // Line (1): a notch per 3 lines, at least one; page (2): one per page.
      n = Math.max(1, Math.round(Math.abs(delta) / (e.deltaMode === 1 ? WHEEL_LINES_PER_NOTCH : 1)))
    }
    for (let i = 0; i < n; i++) out.push(dir)
  }
  axis(e.deltaY, `y`, `up`, `down`)
  axis(e.deltaX, `x`, `left`, `right`)
  return out
}

/** Attaches the Box's native listeners (wheel non-passive, so it can keep the
 *  page still); only the handlers the Box has. Returns the detach. */
export function attachBoxPointer(el: HTMLElement, has: { auxPress: boolean; scroll: boolean }, emit: (event: `auxPress` | `scroll`, payload: Record<string, unknown>) => void): () => void {
  const off: (() => void)[] = []
  const on = <K extends keyof HTMLElementEventMap>(type: K, fn: (e: HTMLElementEventMap[K]) => void, opts?: AddEventListenerOptions) => {
    el.addEventListener(type, fn, opts)
    off.push(() => el.removeEventListener(type, fn, opts))
  }
  if (has.auxPress) {
    on(`mousedown`, (e) => {
      if (e.button !== 1) return
      e.preventDefault()
      e.stopPropagation()
      emit(`auxPress`, { button: `middle` })
    })
    on(`mouseup`, (e) => {
      if (e.button !== 2) return
      e.stopPropagation()
      emit(`auxPress`, { button: `right` })
    })
    on(`contextmenu`, (e) => {
      e.preventDefault()
      e.stopPropagation()
    })
  }
  if (has.scroll) {
    const acc: WheelAccumulator = { x: 0, y: 0 }
    on(
      `wheel`,
      (e) => {
        e.preventDefault()
        e.stopPropagation()
        for (const direction of wheelSteps(e, acc)) emit(`scroll`, { direction })
      },
      { passive: false },
    )
  }
  return () => {
    for (const f of off) f()
  }
}
