// closedloop/segments/common.tsx — shared scaffolding for the five per-flow
// clips (EXP-337). Every segment is authored at LOCAL frame 0; the Reel
// passes the segment-local frame down as a prop. Clips CROSS-FADE (EXP-482):
// each (except the first, whose f0 is the poster) rises over ~10f while the
// previous clip keeps rendering past its duration underneath (the Reel's
// OVERLAP window) and only fades out under the risen newcomer — the canvas
// never shows through at a boundary.

import React from "react"
import { AbsoluteFill, interpolate } from "remotion"
import { EASE, WIN } from "../../ships/theme"
import { OVERLAP } from "../timeline"

export const CLAMP = {
  extrapolateLeft: "clamp",
  extrapolateRight: "clamp",
} as const
export const CLAMP_EASE = { ...CLAMP, easing: EASE } as const

// `portrait` is the phone flag (EXP-482, viewport.ts SMALL_MEDIA — it
// replaced the EXP-392 `small` landscape crops). Under it the composition is
// 1080×1350 (4:5): it drives the screen-space caption treatment and each
// clip's portrait camera framing — per-clip decisions rather than one global
// multiplier, because a single scale cannot express a per-clip crop.
export type SegmentProps = { frame: number; portrait: boolean }

// Screen-space caption size. On the 1080-wide portrait stage 56px lands at
// the same CSS size a 94px caption had on the old 1920-wide phone framing
// (EXP-176/200/392 history) — the narrative line stays readable at ~375px.
export const captionSize = (portrait: boolean): number => (portrait ? 56 : 72)

// Center-pane geometry (window-local, post-EXP-253/282 shell: expanded rail
// 164 + issue-list tool window 520; tabs live in the 34px titlebar).
export const CENTER_X = WIN.rail + WIN.sidebar // 684
export const CENTER_W = WIN.w - CENTER_X // 884
export const CONTENT_TOP = WIN.titleBar // 34

const RISE_DUR = 10
const OUTRO_HOLD = 4 // stay fully opaque under the newcomer's fast rise

// Opacity envelope for a clip (EXP-482 cross-fade): rise at the start
// (skipped for the poster-bearing first clip), then HOLD at 1 through the
// whole authored window — the fade-out lives entirely in the OVERRUN tail
// [dur, dur+OVERLAP) the Reel keeps rendering underneath the next clip. The
// short hold lets the incoming clip (on top) reach near-full opacity before
// this one lets go, so combined coverage never dips toward the canvas.
export const segmentO = (
  frame: number,
  dur: number,
  opts?: { openComposed?: boolean }
): number => {
  const rise = opts?.openComposed
    ? 1
    : interpolate(frame, [0, RISE_DUR], [0, 1], CLAMP_EASE)
  const outro = interpolate(
    frame,
    [dur + OUTRO_HOLD, dur + OVERLAP - 1],
    [1, 0],
    CLAMP_EASE
  )
  return rise * outro
}

// The clip wrapper — applies the envelope around the whole scene.
export const SegmentShell: React.FC<{
  frame: number
  dur: number
  openComposed?: boolean
  children: React.ReactNode
}> = ({ frame, dur, openComposed, children }) => (
  <AbsoluteFill style={{ opacity: segmentO(frame, dur, { openComposed }) }}>
    {children}
  </AbsoluteFill>
)
