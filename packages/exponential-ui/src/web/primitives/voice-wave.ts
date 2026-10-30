// NEW-77: THE voice level (catalog/style.json `voiceWave`): three closed
// polar waves around one optional centre child. The geometry is the core's
// `voiceWave` (src/voice-wave.ts, fixtures/voice-wave.json; the painters
// import it from `@exponential_app/eui`, never through here, so this barrel
// does not carry style.json); this module holds what both web painters
// share: the size enum's px (the neutral theme's `VoiceWave/root` edges),
// the clock that drives the phase, the accessible name and the class
// recipes. Colours follow the theme's
// recipe: outer = info (blue-500, the exponential theme's `info`), mid +
// inner = primary, all muted-foreground and the whole wave at .35 opacity
// while idle or muted.

import type { VoiceWaveActivity, VoiceWaveRingName } from "../../voice-wave"

export type { VoiceWaveActivity, VoiceWaveRingName } from "../../voice-wave"

export const VOICE_WAVE_SIZES = [`sm`, `md`, `lg`, `xl`] as const
export type VoiceWaveSize = (typeof VOICE_WAVE_SIZES)[number]

/** The box edge per size (px): the `VoiceWave/root` recipe's width. */
export const VOICE_WAVE_SIZE_PX: Readonly<Record<VoiceWaveSize, number>> = { sm: 32, md: 56, lg: 160, xl: 200 }

/** Whether `activity` moves the waves (listening, thinking, speaking). */
export function voiceWaveAnimates(activity: VoiceWaveActivity): boolean {
  return activity === `listening` || activity === `thinking` || activity === `speaking`
}

/** Idle and muted paint quiet: muted colours, the wave at .35 opacity (an
 *  unknown activity is idle, as in the core). */
export function voiceWaveQuiet(activity: VoiceWaveActivity): boolean {
  return !voiceWaveAnimates(activity)
}

const NAMED: readonly string[] = [`idle`, `listening`, `thinking`, `speaking`, `muted`]

/** The accessible name (a11y.json `VoiceWave`): the label with the activity
 *  appended (`Voice, listening`); none without a label (decorative). */
export function voiceWaveName(label: string | undefined, activity: VoiceWaveActivity): string | undefined {
  return label ? `${label}, ${NAMED.includes(activity) ? activity : `idle`}` : undefined
}

/** Calls `onFrame(elapsed seconds)` on every animation frame after the
 *  first (the first frame is the clock's zero, already painted); returns
 *  the stop. Without `requestAnimationFrame` (SSR) it never ticks. */
export function voiceWaveClock(onFrame: (elapsed: number) => void): () => void {
  if (typeof requestAnimationFrame !== `function`) return () => {}
  let start: number | null = null
  let id = 0
  const tick = (now: number) => {
    if (start === null) start = now
    else onFrame((now - start) / 1000)
    id = requestAnimationFrame(tick)
  }
  id = requestAnimationFrame(tick)
  return () => cancelAnimationFrame(id)
}

/** THE CHILD RULE's painter half (catalog/style.json `voiceWave`): the child
 *  is laid out centred at its own size; this measures its own (untransformed)
 *  border box and draws it scaled about its centre (= the box centre) by
 *  `scaleOf(its size)` (the core's `voiceWaveChildBox(size, child).scale`:
 *  min(1, childSize / its longer edge), 0 at `sm`). Re-measures on every
 *  resize (ResizeObserver: before the first paint); without one (SSR,
 *  jsdom) it measures once. Returns the stop. */
export function voiceWaveFitChild(el: HTMLElement, scaleOf: (child: { width: number; height: number }) => number): () => void {
  const apply = (width: number, height: number) => {
    const s = scaleOf({ width, height })
    el.style.transform = s === 1 ? `` : `scale(${s})`
  }
  if (typeof ResizeObserver !== `function`) {
    apply(el.offsetWidth, el.offsetHeight)
    return () => {}
  }
  const ro = new ResizeObserver((entries) => {
    const e = entries[entries.length - 1]!
    const b = e.borderBoxSize?.[0]
    if (b) apply(b.inlineSize, b.blockSize)
    else apply(el.offsetWidth, el.offsetHeight)
  })
  ro.observe(el, { box: `border-box` })
  return () => ro.disconnect()
}

export const VOICE_WAVE_CLASSES = {
  root: `relative inline-flex shrink-0 items-center justify-center`,
  quiet: `opacity-35`,
  size: { sm: `size-8`, md: `size-14`, lg: `size-40`, xl: `size-50` } satisfies Record<VoiceWaveSize, string>,
  svg: `pointer-events-none absolute inset-0 size-full overflow-visible`,
  ring: { outer: `text-blue-500`, mid: `text-primary`, inner: `text-primary` } satisfies Record<VoiceWaveRingName, string>,
  ringQuiet: `text-muted-foreground`,
  child: `relative flex shrink-0 items-center justify-center text-foreground`,
  /** At `sm` (childSize 0): kept, painted nothing, no hit target. */
  childHidden: `invisible pointer-events-none`,
  childMuted: `text-muted-foreground`,
} as const
