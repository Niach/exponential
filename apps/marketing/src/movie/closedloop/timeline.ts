// closedloop/timeline.ts — the segment manifest (EXP-337). The film is five
// self-contained per-flow clips played back to back inside ONE stable frame;
// every clip is authored at LOCAL frame 0 in its segments/<id>.tsx file, and
// this manifest derives the composition-global start frames by accumulation —
// segment start frames are never hand-authored. Beat tables, camera keys and
// caption windows live in the segment files (local frames), not here.

import { FLOW_INFO } from "./chapters"

export const FPS = 30

// Per-clip frame budgets (~5–8.5s each @ 30fps; EXP-385 order).
export const SEGMENT_DURATIONS: Record<string, number> = {
  "board-live": 245,
  "code-everywhere": 260,
  "review-merge": 235,
  feedback: 235,
  platforms: 150,
}

export type Segment = {
  id: string
  label: string
  from: number
  dur: number
}

export const SEGMENTS: Segment[] = (() => {
  let from = 0
  return FLOW_INFO.map(({ id, label }) => {
    const dur = SEGMENT_DURATIONS[id]
    if (dur === undefined) throw new Error(`segment ${id} has no duration`)
    const seg = { id, label, from, dur }
    from += dur
    return seg
  })
})()

// Chapter markers for the marketing player stepper — id/label metadata
// lives in chapters.ts (remotion-free, shared with the stepper); the frame is
// the segment's derived start.
export type Chapter = { id: string; label: string; frame: number }
export const CHAPTERS: Chapter[] = SEGMENTS.map(({ id, label, from }) => ({
  id,
  label,
  frame: from,
}))

// The authored story — the last clip has fully settled to the bare canvas by
// here; the END_HOLD tail rests on that canvas so the Player loop breathes
// before wrapping back to the composed f0 IDE frame (the poster).
export const STORY_FRAMES = SEGMENTS.reduce((sum, s) => sum + s.dur, 0)
export const END_HOLD = 15
export const DURATION_IN_FRAMES = STORY_FRAMES + END_HOLD
