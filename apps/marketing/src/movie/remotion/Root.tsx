// The Remotion studio/render root. The site itself never imports this file —
// it embeds the composition directly through @remotion/player
// (../LoopMoviePlayer); this root exists so `bun run movie:studio` and
// `movie:render` can preview and render the SAME ClosedLoop component.
// The Seg-* compositions preview each per-flow clip at its own local
// timeline (EXP-337) — studio-only authoring aids, never rendered.
// Everything is registered twice: once wide (1920×1080), once `portrait`
// (the 1080×1350 phone framing, EXP-482). movie:render and movie:poster keep
// targeting the wide ids, so their output is unaffected; movie:poster:portrait
// renders ClosedLoop-Portrait, and the -pt previews are where the portrait
// camera shots get tuned.
import "./reset.css"
import React from "react"
import { Composition, useCurrentFrame } from "remotion"
import { ClosedLoop, DURATION_IN_FRAMES, FPS } from "../closedloop"
import { SEGMENTS } from "../closedloop/timeline"
import type { SegmentProps } from "../closedloop/segments/common"
import { BoardLiveSegment } from "../closedloop/segments/boardlive"
import { CodeEverywhereSegment } from "../closedloop/segments/codeeverywhere"
import { ReviewMergeSegment } from "../closedloop/segments/reviewmerge"
import { FeedbackSegment } from "../closedloop/segments/feedback"
import { PlatformsSegment } from "../closedloop/segments/platforms"

// The two canvases (keep in lockstep with LoopMoviePlayer + loop.css).
const WIDE = { width: 1920, height: 1080 } as const
const PORTRAIT = { width: 1080, height: 1350 } as const

const SEGMENT_COMPONENTS: Record<string, React.FC<SegmentProps>> = {
  "board-live": BoardLiveSegment,
  "code-everywhere": CodeEverywhereSegment,
  "review-merge": ReviewMergeSegment,
  feedback: FeedbackSegment,
  platforms: PlatformsSegment,
}

const segmentPreview = (id: string, portrait: boolean): React.FC => {
  const Segment = SEGMENT_COMPONENTS[id]
  const Preview: React.FC = () => {
    const frame = useCurrentFrame()
    return <Segment frame={frame} portrait={portrait} />
  }
  return Preview
}

export const RemotionRoot: React.FC = () => (
  <>
    <Composition
      id="ClosedLoop"
      component={ClosedLoop}
      defaultProps={{ portrait: false }}
      durationInFrames={DURATION_IN_FRAMES}
      fps={FPS}
      width={WIDE.width}
      height={WIDE.height}
    />
    <Composition
      id="ClosedLoop-Portrait"
      component={ClosedLoop}
      defaultProps={{ portrait: true }}
      durationInFrames={DURATION_IN_FRAMES}
      fps={FPS}
      width={PORTRAIT.width}
      height={PORTRAIT.height}
    />
    {SEGMENTS.flatMap((seg) =>
      [false, true].map((portrait) => (
        <Composition
          key={`${seg.id}-${portrait}`}
          id={`Seg-${seg.id}${portrait ? `-pt` : ``}`}
          component={segmentPreview(seg.id, portrait)}
          durationInFrames={seg.dur}
          fps={FPS}
          width={portrait ? PORTRAIT.width : WIDE.width}
          height={portrait ? PORTRAIT.height : WIDE.height}
        />
      ))
    )}
  </>
)
