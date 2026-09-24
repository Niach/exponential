import * as React from "react"

import { cn } from "./cn"

// EXP-1051 — the SEGMENTED bar: one track carrying N coloured slices drawn
// left to right, with hairline ticks laid over them. `Meter` draws ONE value
// with a severity tone; this draws a LAYOUT — where a run's context window
// actually went — so the tone is the layer's identity, never its danger.
//
// Hand-mirrored ×4 (desktop `crates/ui/src/usage_sheet.rs`, iOS
// `SegmentedTrack`, Android `SegmentedTrack`); the segments themselves come
// from `lib/context-layout.ts` and its ONE contract fixture.
//
// Two rules the ×4 mirrors share:
//  • The slices are NEVER scaled to fit. The device's estimates can overshoot
//    the measured total, and a silently rescaled bar would lie about every
//    layer's size to hide the overshoot — so the track CLIPS at 100% instead
//    (`shrink-0` + `overflow-hidden`, not `flex-1`).
//  • A tone the caller does not know reads as `neutral`: an older client draws
//    a layer it cannot colour rather than dropping it out of the geometry.

/** tone → fill class. The hues are the avatar palette's (`--avatar-*`,
 *  tokens.json `avatar`), so the bar shares the app's ONE set of identity
 *  colours instead of inventing a seventh palette. The legend's swatch reads
 *  this same map — one square and one slice can never disagree. */
export const SEGMENT_TONE_CLASS: Record<string, string> = {
  neutral: `bg-foreground/30`,
  // avatar-1 orange, -2 yellow, -3 green, -5 blue, -6 violet, -7 pink.
  orange: `bg-avatar-1`,
  yellow: `bg-avatar-2`,
  green: `bg-avatar-3`,
  blue: `bg-avatar-5`,
  violet: `bg-avatar-6`,
  pink: `bg-avatar-7`,
  // `track` is the empty remainder — the bar's own background, so a `free`
  // legend swatch matches the part of the track nothing painted.
  track: `bg-glass-stroke-strong`,
}

/** The class for `tone`, falling back to `neutral`. */
export function segmentToneClass(tone: string): string {
  return SEGMENT_TONE_CLASS[tone] ?? SEGMENT_TONE_CLASS.neutral
}

export interface SegmentedBarSegment {
  /** Stable across renders — the slice that grows must not re-key. */
  key: string
  tone: string
  /** Percent of the WHOLE track, 0-100 (two decimals: a 600-token layer in a
   *  200k window is a hairline rather than nothing). */
  percent: number
}

export function SegmentedBar({
  segments,
  ticks = [],
  className,
  ...props
}: Omit<React.ComponentProps<`div`>, `children`> & {
  segments: readonly SegmentedBarSegment[]
  /** Hairline marks, in percent — thresholds the reader measures against. */
  ticks?: readonly number[]
}) {
  return (
    <div
      data-slot="segmented-bar"
      className={cn(
        `relative flex h-1 w-full overflow-hidden rounded-full bg-glass-stroke-strong`,
        className
      )}
      {...props}
    >
      {segments.map((segment) => (
        <div
          key={segment.key}
          data-slot="segmented-bar-segment"
          data-key={segment.key}
          data-tone={segment.tone}
          className={cn(`h-full shrink-0`, segmentToneClass(segment.tone))}
          style={{ width: `${Math.max(0, segment.percent)}%` }}
        />
      ))}
      {ticks.map((tick) => (
        <span
          key={tick}
          data-slot="segmented-bar-tick"
          data-tick={tick}
          aria-hidden
          className="absolute inset-y-0 w-px bg-background/80"
          style={{ left: `${tick}%` }}
        />
      ))}
    </div>
  )
}
