/* The lazy Remotion chunk for LoopClip — like LoopMoviePlayer, the only
   modules allowed to import remotion/@video. Chapter-id → frame resolution
   happens HERE (CHAPTERS carries the frame numbers via timeline.ts, which
   imports remotion), so the static wrapper stays remotion-free. */
import { useEffect, useRef } from "react"
import { Player, type PlayerRef } from "@remotion/player"
import {
  CHAPTERS,
  ClosedLoop,
  DURATION_IN_FRAMES,
  FPS,
} from "@video/closedloop"

/* Trims that keep a clip's in/out points clear of the film's whip-pan blur
   windows (f251–259 and f731–739), so no clip opens or closes on a smeared
   frame. */
const CLIP_TRIM: Record<string, { start?: number; end?: number }> = {
  feedback: { end: 5 },
  issue: { start: 5 },
  merge: { end: 5 },
  shipped: { start: 5 },
}

export default function LoopClipPlayer({
  chapter,
  playing,
}: {
  chapter: string
  playing: boolean
}) {
  const playerRef = useRef<PlayerRef>(null)

  const index = CHAPTERS.findIndex((c) => c.id === chapter)
  const start = index >= 0 ? CHAPTERS[index].frame : 0
  const nextStart =
    index >= 0 && index + 1 < CHAPTERS.length
      ? CHAPTERS[index + 1].frame
      : DURATION_IN_FRAMES
  const trim = CLIP_TRIM[chapter] ?? {}
  const inFrame = start + (trim.start ?? 0)
  const outFrame = nextStart - 1 - (trim.end ?? 0)

  useEffect(() => {
    const player = playerRef.current
    if (!player) return
    if (playing) {
      player.play()
    } else {
      player.pause()
    }
  }, [playing])

  return (
    <Player
      ref={playerRef}
      component={ClosedLoop}
      durationInFrames={DURATION_IN_FRAMES}
      fps={FPS}
      compositionWidth={1920}
      compositionHeight={1080}
      inFrame={inFrame}
      outFrame={outFrame}
      initialFrame={inFrame}
      autoPlay
      loop
      controls={false}
      clickToPlay={false}
      style={{ width: `100%`, height: `100%` }}
    />
  )
}
