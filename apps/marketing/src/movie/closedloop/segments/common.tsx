// closedloop/segments/common.tsx — shared scaffolding for the five per-flow
// clips (EXP-337). Every segment is authored at LOCAL frame 0; the Reel
// passes the segment-local frame down as a prop. Each clip (except the
// first, whose f0 is the poster) rises from the bare canvas over ~6f and
// settles back to it before its last frame, so stepper seeks always land on
// a clean frame and the hard segment switch in the Reel is invisible.

import React from "react"
import { AbsoluteFill, interpolate } from "remotion"
import { EASE, WIN } from "../../ships/theme"

export const CLAMP = {
  extrapolateLeft: "clamp",
  extrapolateRight: "clamp",
} as const
export const CLAMP_EASE = { ...CLAMP, easing: EASE } as const

export type SegmentProps = { frame: number; textScale: number }

// Center-pane geometry (window-local, post-EXP-253/282 shell: expanded rail
// 164 + issue-list tool window 520; tabs live in the 34px titlebar).
export const CENTER_X = WIN.rail + WIN.sidebar // 684
export const CENTER_W = WIN.w - CENTER_X // 884
export const CONTENT_TOP = WIN.titleBar // 34

const RISE_DUR = 6
const SETTLE_DUR = 6

// Opacity envelope for a clip: rise-from-canvas at the start (skipped for
// the poster-bearing first clip) and settle-to-canvas at the end.
export const segmentO = (
  frame: number,
  dur: number,
  opts?: { openComposed?: boolean }
): number => {
  const rise = opts?.openComposed
    ? 1
    : interpolate(frame, [0, RISE_DUR], [0, 1], CLAMP_EASE)
  const settle = interpolate(
    frame,
    [dur - SETTLE_DUR, dur - 1],
    [1, 0],
    CLAMP_EASE
  )
  return rise * settle
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
