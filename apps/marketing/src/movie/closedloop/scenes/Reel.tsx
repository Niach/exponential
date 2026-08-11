// closedloop/scenes/Reel.tsx — the per-flow reel (EXP-337, replaced the
// continuous Film): five self-contained clips played back to back inside one
// stable frame. Boundaries CROSS-FADE (EXP-482): for the first OVERLAP
// frames of a clip the previous one keeps rendering past its duration
// UNDERNEATH it — the outgoing envelope holds then fades in that overrun
// tail (segments/common.tsx) while the incoming rises on top, so the canvas
// never shows through. The loop wrap raises boardlive's STATIC f0 (the
// poster frame) under the platforms outro and holds it through END_HOLD, so
// the Player's wrap to frame 0 is pixel-identical instead of a hard pop.

import React from "react"
import { AbsoluteFill, interpolate, useCurrentFrame } from "remotion"
import {
  OVERLAP,
  SEGMENTS,
  STORY_FRAMES,
  WRAP_FADE_FROM,
  WRAP_FADE_TO,
} from "../timeline"
import { CLAMP_EASE, type SegmentProps } from "../segments/common"
import { BoardLiveSegment } from "../segments/boardlive"
import { CodeEverywhereSegment } from "../segments/codeeverywhere"
import { ReviewMergeSegment } from "../segments/reviewmerge"
import { FeedbackSegment } from "../segments/feedback"
import { PlatformsSegment } from "../segments/platforms"

const SEGMENT_COMPONENTS: Record<string, React.FC<SegmentProps>> = {
  "board-live": BoardLiveSegment,
  "code-everywhere": CodeEverywhereSegment,
  "review-merge": ReviewMergeSegment,
  feedback: FeedbackSegment,
  platforms: PlatformsSegment,
}

export const Reel: React.FC<{ portrait?: boolean }> = ({
  portrait = false,
}) => {
  const frame = useCurrentFrame()

  // The wrap layer: boardlive's composed f0 under the platforms outro, at
  // full opacity through the END_HOLD tail. `frame={0}` is constant, so the
  // held frame is fully static.
  const wrapO =
    frame >= WRAP_FADE_FROM
      ? interpolate(frame, [WRAP_FADE_FROM, WRAP_FADE_TO], [0, 1], CLAMP_EASE)
      : 0
  const wrapLayer =
    wrapO > 0 ? (
      <AbsoluteFill style={{ opacity: wrapO }}>
        <BoardLiveSegment frame={0} portrait={portrait} />
      </AbsoluteFill>
    ) : null

  if (frame >= STORY_FRAMES) return wrapLayer // END_HOLD: the composed rest

  let activeIndex = 0
  for (let i = 0; i < SEGMENTS.length; i++) {
    if (frame >= SEGMENTS[i].from) activeIndex = i
  }
  const active = SEGMENTS[activeIndex]
  const Segment = SEGMENT_COMPONENTS[active.id]
  if (!Segment) throw new Error(`no segment component for ${active.id}`)

  // Cross-fade window: the previous clip renders its overrun tail UNDER the
  // incoming clip (DOM order: previous first) while its envelope fades.
  const previous =
    frame - active.from < OVERLAP && activeIndex > 0
      ? SEGMENTS[activeIndex - 1]
      : null
  const Previous = previous ? SEGMENT_COMPONENTS[previous.id] : null

  return (
    <>
      {wrapLayer}
      {previous && Previous ? (
        <Previous frame={frame - previous.from} portrait={portrait} />
      ) : null}
      <Segment frame={frame - active.from} portrait={portrait} />
    </>
  )
}
