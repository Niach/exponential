/* The lazy Remotion chunk — the ONLY site module that may import remotion or
   the ClosedLoop composition (LoopMovie's SSR contract: the prerenderer must
   never see these imports in the static graph). Loaded via React.lazy when
   the loop section scrolls near. */
import { useEffect, useRef, useState } from "react"
import { Player, type CallbackListener, type PlayerRef } from "@remotion/player"
import {
  CHAPTERS,
  ClosedLoop,
  DURATION_IN_FRAMES,
  FPS,
} from "./closedloop"
import type { LoopMovieController } from "./LoopMovie"
import { SMALL_MEDIA } from "./viewport"

const chapterIndexForFrame = (frame: number): number => {
  let index = 0
  for (let i = 0; i < CHAPTERS.length; i++) {
    if (frame >= CHAPTERS[i].frame) index = i
  }
  return index
}

export default function LoopMoviePlayer({
  autoPlay,
  onController,
  onChapterProgress,
  onReady,
  onPlayingChange,
}: {
  autoPlay: boolean
  onController: (controller: LoopMovieController) => void
  /** Active chapter index + its playback progress as an integer percent. */
  onChapterProgress: (index: number, percent: number) => void
  onReady: () => void
  onPlayingChange: (playing: boolean) => void
}) {
  const playerRef = useRef<PlayerRef>(null)
  const chapterRef = useRef(-1)
  const percentRef = useRef(-1)

  /* This chunk is client-only (React.lazy), so matchMedia is safe in the
     lazy initializer. On phone widths the composition enlarges its
     screen-space captions and cuts to tight per-clip camera framings — the
     film scales down to ~360px there, where the mocked UI's 11-13px type is
     otherwise unreadable (EXP-176/200/392). SMALL_MEDIA is shared with the
     poster <source> in LoopMovie so the two can't disagree at the boundary. */
  const [small, setSmall] = useState(
    () => window.matchMedia(SMALL_MEDIA).matches
  )

  useEffect(() => {
    const mq = window.matchMedia(SMALL_MEDIA)
    const apply = () => setSmall(mq.matches)
    mq.addEventListener(`change`, apply)
    return () => mq.removeEventListener(`change`, apply)
  }, [])

  useEffect(() => {
    const player = playerRef.current
    if (!player) return

    onController({
      seekToChapter: (index) => {
        const chapter = CHAPTERS[index]
        if (chapter) player.seekTo(chapter.frame)
      },
      play: () => player.play(),
      pause: () => player.pause(),
    })
    onReady()

    // Percent-quantized so the rail's progress fill re-renders ~1×/percent,
    // not every frame.
    const handleFrame: CallbackListener<`frameupdate`> = (event) => {
      const frame = event.detail.frame
      const index = chapterIndexForFrame(frame)
      const start = CHAPTERS[index].frame
      const end =
        index + 1 < CHAPTERS.length
          ? CHAPTERS[index + 1].frame
          : DURATION_IN_FRAMES
      const percent = Math.min(
        100,
        Math.max(0, Math.round(((frame - start) / (end - start)) * 100)),
      )
      if (index !== chapterRef.current || percent !== percentRef.current) {
        chapterRef.current = index
        percentRef.current = percent
        onChapterProgress(index, percent)
      }
    }
    const handlePlay: CallbackListener<`play`> = () => onPlayingChange(true)
    const handlePause: CallbackListener<`pause`> = () => onPlayingChange(false)

    player.addEventListener(`frameupdate`, handleFrame)
    player.addEventListener(`play`, handlePlay)
    player.addEventListener(`pause`, handlePause)
    return () => {
      player.removeEventListener(`frameupdate`, handleFrame)
      player.removeEventListener(`play`, handlePlay)
      player.removeEventListener(`pause`, handlePause)
    }
  }, [onController, onChapterProgress, onReady, onPlayingChange])

  return (
    <Player
      ref={playerRef}
      component={ClosedLoop}
      durationInFrames={DURATION_IN_FRAMES}
      fps={FPS}
      compositionWidth={1920}
      compositionHeight={1080}
      inputProps={{ small }}
      autoPlay={autoPlay}
      loop
      /* The composition is silent, and an UNMUTED Player anchors its clock
         to a shared AudioContext that Chrome keeps suspended until the
         first user gesture — the frame loop then waits on resume() forever
         and autoplay freezes at frame 0 on cold visits (EXP-200). Muted,
         no AudioContext is created and frames advance off performance.now,
         which needs no user activation. controls={false} means nothing can
         ever unmute it. */
      initiallyMuted
      numberOfSharedAudioTags={0}
      controls={false}
      clickToPlay={false}
      style={{ width: `100%`, height: `100%` }}
    />
  )
}
