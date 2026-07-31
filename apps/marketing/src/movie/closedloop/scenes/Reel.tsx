// closedloop/scenes/Reel.tsx — the per-flow reel (EXP-337, replaced the
// continuous Film): five self-contained clips played back to back inside one
// stable frame. Exactly ONE segment renders at a time with its LOCAL frame
// passed down; the segments' own rise/settle envelopes make the hard switch
// invisible. Past STORY_FRAMES the END_HOLD tail rests on the bare canvas so
// the Player loop breathes before wrapping to the composed f0 poster frame.

import React from "react"
import { useCurrentFrame } from "remotion"
import { SEGMENTS, STORY_FRAMES } from "../timeline"
import type { SegmentProps } from "../segments/common"
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

export const Reel: React.FC<{ small?: boolean }> = ({ small = false }) => {
  const frame = useCurrentFrame()
  if (frame >= STORY_FRAMES) return null // END_HOLD rest tail

  let active = SEGMENTS[0]
  for (const seg of SEGMENTS) {
    if (frame >= seg.from) active = seg
  }
  const Segment = SEGMENT_COMPONENTS[active.id]
  if (!Segment) throw new Error(`no segment component for ${active.id}`)
  return <Segment frame={frame - active.from} small={small} />
}
