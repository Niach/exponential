import { useEffect, useLayoutEffect, useRef, type ReactNode } from "react"
import { voiceWave, voiceWaveChildBox, voiceWavePath } from "@exponential_app/ui"
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
} from "@exponential_app/ui/web/primitives"

import { cn } from "./cn"
import { useMediaQuery } from "./use-media-query"

export type { VoiceWaveActivity, VoiceWaveSize } from "@exponential_app/ui/web/primitives"

export interface VoiceWaveProps {
  activity?: VoiceWaveActivity
  /** Loudness 0..1. */
  level?: number
  /** Up to 8 band levels 0..1. */
  bands?: readonly number[]
  size?: VoiceWaveSize
  /** The accessible name (the activity is appended); none = decorative. */
  label?: string
  className?: string
  /** The centre, at its own size, drawn scaled down to the wave's child
   *  box (never up; kept but not painted at `sm`). */
  children?: ReactNode
}

const hasChild = (c: ReactNode) => c !== undefined && c !== null && c !== false && !(Array.isArray(c) && c.length === 0)

/** NEW-77: the Exponential UI `VoiceWave` native as a primitive: three
 *  closed waves (the core's `voiceWave`) around an optional centre child,
 *  moving while listening, thinking or speaking (not under reduced motion).
 *  The frames write the paths' `d` directly, never a re-render. */
export function VoiceWave({ activity = `idle`, level = 0, bands, size = `md`, label, className, children }: VoiceWaveProps) {
  const px = VOICE_WAVE_SIZE_PX[size] ?? VOICE_WAVE_SIZE_PX.md
  const reducedMotion = useMediaQuery(`(prefers-reduced-motion: reduce)`)
  const input = { size: px, activity, level, bands, reducedMotion }
  const latest = useRef(input)
  latest.current = input
  const elapsed = useRef(0)
  const paths = useRef<(SVGPathElement | null)[]>([])
  const animates = voiceWaveAnimates(activity) && !reducedMotion
  useEffect(() => {
    if (!animates) {
      elapsed.current = 0
      return
    }
    return voiceWaveClock((t) => {
      elapsed.current = t
      voiceWave({ ...latest.current, elapsed: t }).rings.forEach((ring, k) => paths.current[k]?.setAttribute(`d`, voiceWavePath(ring)))
    })
  }, [animates])
  const childEl = useRef<HTMLSpanElement | null>(null)
  const present = hasChild(children)
  useLayoutEffect(() => (present && childEl.current ? voiceWaveFitChild(childEl.current, (c) => voiceWaveChildBox(px, c).scale) : undefined), [present, px])
  const wave = voiceWave({ ...input, elapsed: elapsed.current })
  const quiet = voiceWaveQuiet(activity)
  const name = voiceWaveName(label, activity)
  return (
    <span
      data-slot="voice-wave"
      data-activity={activity}
      data-size={size}
      role={name ? `img` : undefined}
      aria-label={name}
      aria-hidden={name ? undefined : true}
      className={cn(VOICE_WAVE_CLASSES.root, VOICE_WAVE_CLASSES.size[size], quiet && VOICE_WAVE_CLASSES.quiet, className)}
    >
      <svg viewBox={`0 0 ${px} ${px}`} width={px} height={px} aria-hidden className={VOICE_WAVE_CLASSES.svg}>
        {wave.rings.map((ring, k) => (
          <path
            key={ring.part}
            ref={(el) => {
              paths.current[k] = el
            }}
            data-slot="voice-wave-ring"
            data-ring={ring.part}
            className={quiet ? VOICE_WAVE_CLASSES.ringQuiet : VOICE_WAVE_CLASSES.ring[ring.part]}
            d={voiceWavePath(ring)}
            fill="currentColor"
            fillOpacity={ring.fillAlpha}
            stroke="currentColor"
            strokeOpacity={ring.strokeAlpha}
            strokeWidth={ring.strokeWidth}
            strokeLinejoin="round"
          />
        ))}
      </svg>
      {present ? (
        <span
          ref={childEl}
          data-slot="voice-wave-child"
          aria-hidden={wave.childSize > 0 ? undefined : true}
          className={cn(VOICE_WAVE_CLASSES.child, activity === `muted` && VOICE_WAVE_CLASSES.childMuted, wave.childSize > 0 ? undefined : VOICE_WAVE_CLASSES.childHidden)}
        >
          {children}
        </span>
      ) : null}
    </span>
  )
}
