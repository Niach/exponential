// The Remotion studio/render root. The site itself never imports this file —
// it embeds the composition directly through @remotion/player
// (../LoopMoviePlayer); this root exists so `bun run movie:studio` and
// `movie:render` can preview and render the SAME ClosedLoop component.
// The Seg-* compositions preview each per-flow clip at its own local
// timeline (EXP-337) — studio-only authoring aids, never rendered.
// Everything is registered twice: once wide, once `small` (the phone framing,
// EXP-392). movie:render and movie:poster keep targeting the wide ids, so
// their output is unaffected; movie:poster:small renders ClosedLoop-Small,
// and the -sm previews are where the mobile camera shots get tuned.
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

const SEGMENT_COMPONENTS: Record<string, React.FC<SegmentProps>> = {
  "board-live": BoardLiveSegment,
  "code-everywhere": CodeEverywhereSegment,
  "review-merge": ReviewMergeSegment,
  feedback: FeedbackSegment,
  platforms: PlatformsSegment,
}

const segmentPreview = (id: string, small: boolean): React.FC => {
  const Segment = SEGMENT_COMPONENTS[id]
  const Preview: React.FC = () => {
    const frame = useCurrentFrame()
    return <Segment frame={frame} small={small} />
  }
  return Preview
}

export const RemotionRoot: React.FC = () => (
  <>
    <Composition
      id="ClosedLoop"
      component={ClosedLoop}
      defaultProps={{ small: false }}
      durationInFrames={DURATION_IN_FRAMES}
      fps={FPS}
      width={1920}
      height={1080}
    />
    <Composition
      id="ClosedLoop-Small"
      component={ClosedLoop}
      defaultProps={{ small: true }}
      durationInFrames={DURATION_IN_FRAMES}
      fps={FPS}
      width={1920}
      height={1080}
    />
    {SEGMENTS.flatMap((seg) =>
      [false, true].map((small) => (
        <Composition
          key={`${seg.id}-${small}`}
          id={`Seg-${seg.id}${small ? `-sm` : ``}`}
          component={segmentPreview(seg.id, small)}
          durationInFrames={seg.dur}
          fps={FPS}
          width={1920}
          height={1080}
        />
      ))
    )}
  </>
)
