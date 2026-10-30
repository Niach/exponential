import { Index, Show, children as resolveChildren, createEffect, createMemo, createSignal, onCleanup, type JSX } from "solid-js"
import { voiceWave, voiceWaveChildBox, voiceWavePath } from "@exponential_app/eui"
import {
  VOICE_WAVE_CLASSES,
  VOICE_WAVE_SIZE_PX,
  voiceWaveAnimates,
  voiceWaveClock,
  voiceWaveFitChild,
  voiceWaveName,
  voiceWaveQuiet,
  type VoiceWaveActivity,
  type VoiceWaveSize,
} from "@exponential_app/eui/web/primitives"

import { cn } from "./cn"
import { createMediaQuery } from "./use-media-query"

export type { VoiceWaveActivity, VoiceWaveSize } from "@exponential_app/eui/web/primitives"

export interface VoiceWaveProps {
  activity?: VoiceWaveActivity
  /** Loudness 0..1. */
  level?: number
  /** Up to 8 band levels 0..1. */
  bands?: readonly number[]
  size?: VoiceWaveSize
  /** The accessible name (the activity is appended); none = decorative. */
  label?: string
  class?: string
  /** The centre, at its own size, drawn scaled down to the wave's child
   *  box (never up; kept but not painted at `sm`). */
  children?: JSX.Element
}

/** NEW-77 (React's primitives/voice-wave.tsx): the Exponential UI
 *  `VoiceWave` native as a primitive: three closed waves (the core's
 *  `voiceWave`) around an optional centre child, moving while listening,
 *  thinking or speaking (not under reduced motion). */
export function VoiceWave(props: VoiceWaveProps) {
  const reducedMotion = createMediaQuery(`(prefers-reduced-motion: reduce)`)
  const activity = () => props.activity ?? `idle`
  const size = () => props.size ?? `md`
  const px = () => VOICE_WAVE_SIZE_PX[size()] ?? VOICE_WAVE_SIZE_PX.md
  const [elapsed, setElapsed] = createSignal(0)
  const animates = createMemo(() => voiceWaveAnimates(activity()) && !reducedMotion())
  createEffect(() => {
    if (!animates()) {
      setElapsed(0)
      return
    }
    onCleanup(voiceWaveClock(setElapsed))
  })
  const wave = createMemo(() => voiceWave({ size: px(), activity: activity(), level: props.level ?? 0, bands: props.bands, reducedMotion: reducedMotion(), elapsed: elapsed() }))
  const quiet = () => voiceWaveQuiet(activity())
  const name = () => voiceWaveName(props.label, activity())
  const child = resolveChildren(() => props.children)
  const hasChild = () => {
    const c = child()
    return c !== undefined && c !== null && (!Array.isArray(c) || c.length > 0)
  }
  return (
    <span
      data-slot="voice-wave"
      data-activity={activity()}
      data-size={size()}
      role={name() ? `img` : undefined}
      aria-label={name()}
      aria-hidden={name() ? undefined : `true`}
      class={cn(VOICE_WAVE_CLASSES.root, VOICE_WAVE_CLASSES.size[size()], quiet() && VOICE_WAVE_CLASSES.quiet, props.class)}
    >
      <svg viewBox={`0 0 ${px()} ${px()}`} width={px()} height={px()} aria-hidden="true" class={VOICE_WAVE_CLASSES.svg}>
        <Index each={wave().rings}>
          {(ring) => (
            <path
              data-slot="voice-wave-ring"
              data-ring={ring().part}
              class={quiet() ? VOICE_WAVE_CLASSES.ringQuiet : VOICE_WAVE_CLASSES.ring[ring().part]}
              d={voiceWavePath(ring())}
              fill="currentColor"
              fill-opacity={ring().fillAlpha}
              stroke="currentColor"
              stroke-opacity={ring().strokeAlpha}
              stroke-width={ring().strokeWidth}
              stroke-linejoin="round"
            />
          )}
        </Index>
      </svg>
      <Show when={hasChild()}>
        <span
          ref={(el) =>
            createEffect(() => {
              const edge = px()
              onCleanup(voiceWaveFitChild(el, (c) => voiceWaveChildBox(edge, c).scale))
            })
          }
          data-slot="voice-wave-child"
          aria-hidden={wave().childSize > 0 ? undefined : `true`}
          class={cn(VOICE_WAVE_CLASSES.child, activity() === `muted` && VOICE_WAVE_CLASSES.childMuted, wave().childSize > 0 ? undefined : VOICE_WAVE_CLASSES.childHidden)}
        >
          {child()}
        </span>
      </Show>
    </span>
  )
}
